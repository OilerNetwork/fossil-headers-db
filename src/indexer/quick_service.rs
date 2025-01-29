use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use eyre::{eyre, Result};
use futures::future::try_join_all;
use tokio::task;
use tracing::{error, info, warn};

use crate::{
    db::DbConnection,
    repositories::{
        block_header::{insert_block_header_only_query, insert_block_header_query},
        index_metadata::{get_index_metadata, update_latest_quick_index_block_number_query},
    },
    rpc::EthereumRpcProvider,
};

#[derive(Debug)]
pub struct QuickIndexConfig {
    pub max_retries: u8,
    pub poll_interval: u32,
    pub rpc_timeout: u32,
    pub index_batch_size: u32,
    pub should_index_txs: bool,
}

impl Default for QuickIndexConfig {
    fn default() -> Self {
        Self {
            max_retries: 10,
            poll_interval: 10,
            rpc_timeout: 300,
            index_batch_size: 20,
            should_index_txs: false,
        }
    }
}

pub struct QuickIndexer<T> {
    config: QuickIndexConfig,
    db: Arc<DbConnection>,
    rpc_provider: Arc<T>,
    should_terminate: Arc<AtomicBool>,
}

impl<T> QuickIndexer<T>
where
    T: EthereumRpcProvider + Send + Sync + 'static,
{
    pub async fn new(
        config: QuickIndexConfig,
        db: Arc<DbConnection>,
        rpc_provider: Arc<T>,
        should_terminate: Arc<AtomicBool>,
    ) -> QuickIndexer<T> {
        Self {
            db,
            rpc_provider,
            config,
            should_terminate,
        }
    }

    pub async fn index(&self) -> Result<()> {
        // Quick indexer loop, does the following until terminated:
        // 1. check current latest block
        // 2. check if there's a new blocks finalized
        // 3. if yes, index the blocks
        // 4. if not, sleep for a period of time and do nothing
        while !self.should_terminate.load(Ordering::Relaxed) {
            let last_block_number = match get_index_metadata(self.db.clone()).await {
                Ok(metadata) => {
                    if let Some(metadata) = metadata {
                        metadata.current_latest_block_number
                    } else {
                        error!("[quick_index] Error getting index metadata");
                        return Err(eyre!("Error getting index metadata: metadata not found."));
                    }
                }
                Err(e) => {
                    error!("[quick_index] Error getting index metadata: {}", e);
                    return Err(e);
                }
            };

            let new_latest_block = self
                .rpc_provider
                .get_latest_finalized_blocknumber(Some(self.config.rpc_timeout.into()))
                .await?;

            if new_latest_block > last_block_number {
                let ending_block_number: i64 =
                    if new_latest_block - last_block_number > self.config.index_batch_size.into() {
                        last_block_number + i64::from(self.config.index_batch_size)
                    } else {
                        new_latest_block
                    };

                self.index_block_range(
                    last_block_number + 1, // index from recorded last block + 1
                    ending_block_number,
                    &self.should_terminate,
                )
                .await?;
            } else {
                info!(
                    "No new block finalized. Latest: {}. Sleeping for {}s...",
                    new_latest_block, self.config.poll_interval
                );
                tokio::time::sleep(Duration::from_secs(self.config.poll_interval.into())).await;
            }
        }

        info!("[quick_index] Process terminating.");
        Ok(())
    }

    // Indexing a block range, inclusive.
    async fn index_block_range(
        &self,
        starting_block: i64,
        ending_block: i64,
        should_terminate: &AtomicBool,
    ) -> Result<()> {
        let block_range: Vec<i64> = (starting_block..ending_block + 1).collect();

        for i in 0..self.config.max_retries {
            if should_terminate.load(Ordering::Relaxed) {
                info!("[quick_index] Termination requested. Stopping quick indexing.");
                break;
            }

            let timeout = self.config.rpc_timeout;

            let rpc_block_headers_futures: Vec<_> = block_range
                .clone()
                .into_iter()
                .map(|block_number| {
                    let provider = self.rpc_provider.clone();
                    let should_index_txs = self.config.should_index_txs;

                    task::spawn(async move {
                        provider
                            .get_full_block_by_number(
                                block_number,
                                should_index_txs,
                                Some(timeout.into()),
                            )
                            .await
                    })
                })
                .collect();

            let rpc_block_headers_response = try_join_all(rpc_block_headers_futures).await?;

            let mut block_headers = Vec::with_capacity(rpc_block_headers_response.len());
            let mut has_err = false;

            for header in rpc_block_headers_response.into_iter() {
                match header {
                    Ok(header) => {
                        block_headers.push(header);
                    }
                    Err(e) => {
                        has_err = true;
                        warn!(
                            "[quick_index] Error retrieving block in range from {} to {}. error: {}",
                            starting_block, ending_block, e
                        )
                    }
                }
            }

            if !has_err {
                let mut db_tx = self.db.pool.begin().await?;

                if self.config.should_index_txs {
                    insert_block_header_query(&mut db_tx, block_headers).await?;
                } else {
                    insert_block_header_only_query(&mut db_tx, block_headers).await?;
                }

                update_latest_quick_index_block_number_query(&mut db_tx, ending_block).await?;

                // Commit at the end
                db_tx.commit().await?;

                info!(
                    "[quick_index] Indexing block range from {} to {} complete.",
                    starting_block, ending_block
                );
                return Ok(());
            }

            // If there's an error during rpc, retry.
            error!("[quick_index] Error encountered during rpc, retry no. {}. Re-running from block: {}", i, starting_block);

            // Exponential backoff
            let backoff = (i as u64).pow(2) * 5;
            tokio::time::sleep(Duration::from_secs(backoff)).await;
        }

        Err(eyre!("Max retries reached. Stopping quick indexing."))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        indexer::test_utils::MockRpcProvider,
        repositories::{
            block_header::{BlockHeaderDto, TransactionDto},
            index_metadata::set_initial_indexing_status,
        },
        rpc::{BlockHeader, RpcResponse, Transaction},
        utils::convert_hex_string_to_i64,
    };
    use sqlx::{pool::PoolOptions, ConnectOptions};
    use tokio::{fs, time::sleep};

    use super::*;

    async fn get_fixtures_for_tests() -> Vec<BlockHeader> {
        let mut block_fixtures = vec![];

        for i in 0..=5 {
            let json_string = fs::read_to_string(format!(
                "tests/fixtures/indexer/eth_getBlockByNumber_sepolia_{}.json",
                i
            ))
            .await
            .unwrap();

            let block = serde_json::from_str::<RpcResponse<BlockHeader>>(&json_string).unwrap();
            block_fixtures.push(block.result);
        }

        block_fixtures
    }

    #[sqlx::test]
    async fn test_quick_indexer_new(
        _pool_options: PoolOptions<sqlx::Postgres>,
        connect_options: impl ConnectOptions,
    ) {
        let config = QuickIndexConfig::default();
        let url = connect_options.to_url_lossy().to_string();
        let db = DbConnection::new(url).await.unwrap();
        let mock_rpc = Arc::new(MockRpcProvider::new());
        let should_terminate = Arc::new(AtomicBool::new(false));

        let indexer = QuickIndexer::new(config, db, mock_rpc, should_terminate).await;

        assert_eq!(indexer.config.max_retries, 10);
        assert_eq!(indexer.config.poll_interval, 10);
        assert_eq!(indexer.config.rpc_timeout, 300);
        assert_eq!(indexer.config.index_batch_size, 20);
        assert!(!indexer.config.should_index_txs);
    }

    #[sqlx::test]
    async fn test_quick_index_default_headers_only(
        _pool_options: PoolOptions<sqlx::Postgres>,
        connect_options: impl ConnectOptions,
    ) {
        let block_fixtures = get_fixtures_for_tests().await;
        let url = connect_options.to_url_lossy().to_string();

        let config = QuickIndexConfig::default();
        let db = DbConnection::new(url).await.unwrap();
        let mock_rpc = Arc::new(MockRpcProvider::new_with_data(
            vec![5; block_fixtures.len()].into(),
            vec![block_fixtures[5].clone(); 5].into(),
        ));
        let should_terminate = Arc::new(AtomicBool::new(false));

        // Set initial db state, setting to 4 as we want the indexer to start from 5.
        set_initial_indexing_status(db.clone(), 4, 4, true)
            .await
            .unwrap();

        let indexer =
            QuickIndexer::new(config, db.clone(), mock_rpc, should_terminate.clone()).await;

        task::spawn(async move {
            indexer.index().await.unwrap();
        });

        // Wait until its indexed
        sleep(Duration::from_secs(1)).await;
        should_terminate.store(true, Ordering::Relaxed);

        let metadata = get_index_metadata(db.clone()).await.unwrap().unwrap();
        assert_eq!(metadata.current_latest_block_number, 5);

        // Since quick indexes from the latest, it should only index up to 5.
        // Check if the block is indexed correctly.
        let result: Result<Vec<BlockHeaderDto>, sqlx::Error> =
            sqlx::query_as("SELECT * FROM blockheaders")
                .fetch_all(&mut *db.pool.acquire().await.unwrap())
                .await;
        assert!(result.is_ok());

        let result = result.unwrap();
        assert_eq!(result.len(), 1);
        // If the block is indexed correctly, the hash should match.
        assert_eq!(result[0].block_hash, block_fixtures[5].hash);

        // Check if the transactions are not indexed.
        let tx_result: Result<Vec<TransactionDto>, sqlx::Error> =
            sqlx::query_as("SELECT * FROM transactions")
                .fetch_all(&mut *db.pool.acquire().await.unwrap())
                .await;
        assert!(tx_result.is_ok());
        assert!(tx_result.unwrap().is_empty());
    }

    #[sqlx::test]
    async fn test_quick_index_with_tx(
        _pool_options: PoolOptions<sqlx::Postgres>,
        connect_options: impl ConnectOptions,
    ) {
        let block_fixtures = get_fixtures_for_tests().await;
        let url = connect_options.to_url_lossy().to_string();

        let config = QuickIndexConfig {
            should_index_txs: true,
            ..QuickIndexConfig::default()
        };
        let db = DbConnection::new(url).await.unwrap();
        let mock_rpc = Arc::new(MockRpcProvider::new_with_data(
            vec![5; block_fixtures.len()].into(),
            vec![block_fixtures[5].clone(); 5].into(),
        ));
        let should_terminate = Arc::new(AtomicBool::new(false));

        // Set initial db state, setting to 4 as we want the indexer to start from 5.
        set_initial_indexing_status(db.clone(), 4, 4, true)
            .await
            .unwrap();

        let indexer =
            QuickIndexer::new(config, db.clone(), mock_rpc, should_terminate.clone()).await;

        task::spawn(async move {
            indexer.index().await.unwrap();
        });

        // Wait until its indexed
        sleep(Duration::from_secs(1)).await;
        should_terminate.store(true, Ordering::Relaxed);

        let metadata = get_index_metadata(db.clone()).await.unwrap().unwrap();
        assert_eq!(metadata.current_latest_block_number, 5);

        // Since quick indexes from the latest, it should only index up to 5.
        // Check if the block is indexed correctly.
        let result: Result<Vec<BlockHeaderDto>, sqlx::Error> =
            sqlx::query_as("SELECT * FROM blockheaders")
                .fetch_all(&mut *db.pool.acquire().await.unwrap())
                .await;
        assert!(result.is_ok());

        let result = result.unwrap();
        assert_eq!(result.len(), 1);
        // If the block is indexed correctly, the hash should match.
        assert_eq!(result[0].block_hash, block_fixtures[5].hash);

        // Check if the transactions are indexed correctly.
        let result: Result<Vec<TransactionDto>, sqlx::Error> =
            sqlx::query_as("SELECT * FROM transactions")
                .fetch_all(&mut *db.pool.acquire().await.unwrap())
                .await;
        assert!(result.is_ok());

        let result = result.unwrap();
        assert_eq!(result.len(), block_fixtures[5].transactions.len());
        for (i, tx) in result.iter().enumerate() {
            assert_eq!(
                tx.block_number,
                convert_hex_string_to_i64(&block_fixtures[5].number).unwrap()
            );

            let fixtures_tx: Transaction = block_fixtures[5].transactions[i]
                .clone()
                .try_into()
                .unwrap();
            assert_eq!(tx.transaction_hash, fixtures_tx.hash);
        }
    }

    #[sqlx::test]
    async fn test_quick_index_always_index_for_latest_blocknumber_with_tx(
        _pool_options: PoolOptions<sqlx::Postgres>,
        connect_options: impl ConnectOptions,
    ) {
        let block_fixtures = get_fixtures_for_tests().await;
        let url = connect_options.to_url_lossy().to_string();

        let config = QuickIndexConfig {
            should_index_txs: true,
            ..QuickIndexConfig::default()
        };
        let db = DbConnection::new(url).await.unwrap();
        let should_terminate = Arc::new(AtomicBool::new(false));

        // In this case let's simulate that the latest block number goes from 3-5.
        let mock_rpc = Arc::new(MockRpcProvider::new_with_data(
            vec![5, 5].into(),
            vec![block_fixtures[4].clone(), block_fixtures[5].clone()].into(),
        ));

        // Set initial db state, setting to 3 as we want the indexer to start from 4.
        set_initial_indexing_status(db.clone(), 3, 3, true)
            .await
            .unwrap();

        let indexer =
            QuickIndexer::new(config, db.clone(), mock_rpc, should_terminate.clone()).await;

        // With the above configuration, the indexer should first not index anything since it started at 3.
        // Then as it increases, it should index 4 and 5.
        task::spawn(async move {
            indexer.index().await.unwrap();
        });

        // Wait until its indexed
        sleep(Duration::from_secs(1)).await;
        should_terminate.store(true, Ordering::Relaxed);

        let metadata = get_index_metadata(db.clone()).await.unwrap().unwrap();
        assert_eq!(metadata.current_latest_block_number, 5);

        // Since quick indexes from the latest, it should only index up to 5.
        // Check if the block is indexed correctly.
        let result: Result<Vec<BlockHeaderDto>, sqlx::Error> =
            sqlx::query_as("SELECT * FROM blockheaders order by number asc")
                .fetch_all(&mut *db.pool.acquire().await.unwrap())
                .await;
        assert!(result.is_ok());

        let result = result.unwrap();
        assert_eq!(result.len(), 2);
        // If the block is indexed correctly, the hash should match.
        assert_eq!(result[0].block_hash, block_fixtures[4].hash);
        assert_eq!(result[1].block_hash, block_fixtures[5].hash);

        // Check if the transactions are indexed correctly.
        let block_4_result: Result<Vec<TransactionDto>, sqlx::Error> =
            sqlx::query_as("SELECT * FROM transactions WHERE block_number = $1")
                .bind(convert_hex_string_to_i64(&block_fixtures[4].number).unwrap())
                .fetch_all(&mut *db.pool.acquire().await.unwrap())
                .await;
        let block_5_result: Result<Vec<TransactionDto>, sqlx::Error> =
            sqlx::query_as("SELECT * FROM transactions WHERE block_number = $1")
                .bind(convert_hex_string_to_i64(&block_fixtures[5].number).unwrap())
                .fetch_all(&mut *db.pool.acquire().await.unwrap())
                .await;
        assert!(block_4_result.is_ok());
        assert!(block_5_result.is_ok());

        let block_4_result = block_4_result.unwrap();
        assert_eq!(block_4_result.len(), block_fixtures[4].transactions.len());
        for (i, tx) in block_4_result.iter().enumerate() {
            assert_eq!(
                tx.block_number,
                convert_hex_string_to_i64(&block_fixtures[4].number).unwrap()
            );
            let fixtures_tx: Transaction = block_fixtures[4].transactions[i]
                .clone()
                .try_into()
                .unwrap();
            assert_eq!(tx.transaction_hash, fixtures_tx.hash);
        }

        let block_5_result = block_5_result.unwrap();
        assert_eq!(block_5_result.len(), block_fixtures[5].transactions.len());
        for (i, tx) in block_5_result.iter().enumerate() {
            assert_eq!(
                tx.block_number,
                convert_hex_string_to_i64(&block_fixtures[5].number).unwrap()
            );
            let fixtures_tx: Transaction = block_fixtures[5].transactions[i]
                .clone()
                .try_into()
                .unwrap();
            assert_eq!(tx.transaction_hash, fixtures_tx.hash);
        }
    }

    #[sqlx::test]
    async fn test_failed_to_get_index_metadata(
        _pool_options: PoolOptions<sqlx::Postgres>,
        connect_options: impl ConnectOptions,
    ) {
        let config = QuickIndexConfig::default();
        let url = connect_options.to_url_lossy().to_string();

        let db = DbConnection::new(url).await.unwrap();
        let mock_rpc = Arc::new(MockRpcProvider::new());
        let should_terminate = Arc::new(AtomicBool::new(false));

        // The metadata table would not exist without initializing it.
        let indexer =
            QuickIndexer::new(config, db.clone(), mock_rpc, should_terminate.clone()).await;

        let result = indexer.index().await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Error getting index metadata: metadata not found."
        );
    }

    #[sqlx::test]
    async fn test_rpc_failed(
        _pool_options: PoolOptions<sqlx::Postgres>,
        connect_options: impl ConnectOptions,
    ) {
        let config = QuickIndexConfig::default();
        let url = connect_options.to_url_lossy().to_string();

        let db = DbConnection::new(url).await.unwrap();
        // Empty rpc would result in error here.
        let mock_rpc = Arc::new(MockRpcProvider::new());
        let should_terminate = Arc::new(AtomicBool::new(false));

        // Set initial db state, setting to 4 as we want the indexer to start from 5.
        set_initial_indexing_status(db.clone(), 4, 4, true)
            .await
            .unwrap();

        let indexer =
            QuickIndexer::new(config, db.clone(), mock_rpc, should_terminate.clone()).await;

        let result = indexer.index().await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Failed to get latest finalized block number"
        );
    }

    #[sqlx::test]
    async fn test_should_retry_when_rpc_error_happens(
        _pool_options: PoolOptions<sqlx::Postgres>,
        connect_options: impl ConnectOptions,
    ) {
        let block_fixtures = get_fixtures_for_tests().await;
        let url = connect_options.to_url_lossy().to_string();

        // Reduce the max tries to 2 to speed up the test.
        let config = QuickIndexConfig {
            max_retries: 2,
            ..QuickIndexConfig::default()
        };
        let db = DbConnection::new(url).await.unwrap();

        // Empty the rpc response for get_block_by_number, so it will attempt to retry
        let mock_rpc = Arc::new(MockRpcProvider::new_with_data(
            vec![5; block_fixtures.len()].into(),
            vec![].into(),
        ));
        let should_terminate = Arc::new(AtomicBool::new(false));

        // Set initial db state, setting to 4 as we want the indexer to start from 5.
        set_initial_indexing_status(db.clone(), 4, 4, true)
            .await
            .unwrap();

        let indexer =
            QuickIndexer::new(config, db.clone(), mock_rpc, should_terminate.clone()).await;

        let result = indexer.index().await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Max retries reached. Stopping quick indexing."
        );
    }
}
