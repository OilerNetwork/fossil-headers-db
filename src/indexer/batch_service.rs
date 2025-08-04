use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use eyre::{eyre, Result};
use futures::future::join_all;
use tokio::{sync::Semaphore, task};
use tracing::{error, info, warn};

use crate::{
    db::DbConnection,
    errors::BlockchainError,
    repositories::{
        block_header::{insert_block_header_only_query, insert_block_header_query},
        index_metadata::{
            get_index_metadata, set_is_backfilling, update_backfilling_block_number_query,
        },
    },
    rpc::EthereumRpcProvider,
    types::BlockNumber,
};

#[derive(Debug)]
pub struct BatchIndexConfig {
    // TODO: maybe reconsidering these variables? Since we share the same with quick for now
    pub max_retries: u8,
    pub poll_interval: u32,
    pub rpc_timeout: u32,
    pub index_batch_size: u32,
    pub should_index_txs: bool,
    pub max_concurrent_requests: usize,
    pub task_timeout: u32,
}

impl BatchIndexConfig {
    #[must_use]
    pub const fn builder() -> BatchIndexConfigBuilder {
        BatchIndexConfigBuilder::new()
    }
}

impl Default for BatchIndexConfig {
    fn default() -> Self {
        Self {
            max_retries: 10,
            poll_interval: 10,
            rpc_timeout: 300,
            index_batch_size: 50,
            should_index_txs: false,
            max_concurrent_requests: 10,
            task_timeout: 300,
        }
    }
}

pub struct BatchIndexConfigBuilder {
    max_retries: u8,
    poll_interval: u32,
    rpc_timeout: u32,
    index_batch_size: u32,
    should_index_txs: bool,
    max_concurrent_requests: usize,
    task_timeout: u32,
}

impl BatchIndexConfigBuilder {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            max_retries: 10,
            poll_interval: 10,
            rpc_timeout: 300,
            index_batch_size: 50,
            should_index_txs: false,
            max_concurrent_requests: 10,
            task_timeout: 300,
        }
    }

    #[must_use]
    pub const fn high_throughput() -> Self {
        Self::new()
            .index_batch_size(200)
            .max_concurrent_requests(50)
            .task_timeout(600)
            .rpc_timeout(600)
    }

    #[must_use]
    pub const fn conservative() -> Self {
        Self::new()
            .index_batch_size(10)
            .max_concurrent_requests(3)
            .task_timeout(120)
            .max_retries(5)
    }

    #[must_use]
    pub const fn testing() -> Self {
        Self::new()
            .index_batch_size(2)
            .max_concurrent_requests(1)
            .task_timeout(30)
            .max_retries(1)
    }

    #[must_use]
    pub const fn max_retries(mut self, max_retries: u8) -> Self {
        self.max_retries = max_retries;
        self
    }

    #[must_use]
    pub const fn poll_interval(mut self, poll_interval: u32) -> Self {
        self.poll_interval = poll_interval;
        self
    }

    #[must_use]
    pub const fn rpc_timeout(mut self, rpc_timeout: u32) -> Self {
        self.rpc_timeout = rpc_timeout;
        self
    }

    #[must_use]
    pub const fn index_batch_size(mut self, index_batch_size: u32) -> Self {
        self.index_batch_size = index_batch_size;
        self
    }

    #[must_use]
    pub const fn should_index_txs(mut self, should_index_txs: bool) -> Self {
        self.should_index_txs = should_index_txs;
        self
    }

    #[must_use]
    pub const fn max_concurrent_requests(mut self, max_concurrent_requests: usize) -> Self {
        self.max_concurrent_requests = max_concurrent_requests;
        self
    }

    #[must_use]
    pub const fn task_timeout(mut self, task_timeout: u32) -> Self {
        self.task_timeout = task_timeout;
        self
    }

    pub fn build(self) -> Result<BatchIndexConfig, BlockchainError> {
        if self.max_retries == 0 {
            return Err(BlockchainError::configuration(
                "max_retries",
                "Max retries must be greater than 0",
            ));
        }

        if self.poll_interval == 0 {
            return Err(BlockchainError::configuration(
                "poll_interval",
                "Poll interval must be greater than 0",
            ));
        }

        if self.rpc_timeout == 0 {
            return Err(BlockchainError::configuration(
                "rpc_timeout",
                "RPC timeout must be greater than 0",
            ));
        }

        if self.index_batch_size == 0 {
            return Err(BlockchainError::configuration(
                "index_batch_size",
                "Index batch size must be greater than 0",
            ));
        }

        if self.max_concurrent_requests == 0 {
            return Err(BlockchainError::configuration(
                "max_concurrent_requests",
                "Max concurrent requests must be greater than 0",
            ));
        }

        if self.task_timeout == 0 {
            return Err(BlockchainError::configuration(
                "task_timeout",
                "Task timeout must be greater than 0",
            ));
        }

        Ok(BatchIndexConfig {
            max_retries: self.max_retries,
            poll_interval: self.poll_interval,
            rpc_timeout: self.rpc_timeout,
            index_batch_size: self.index_batch_size,
            should_index_txs: self.should_index_txs,
            max_concurrent_requests: self.max_concurrent_requests,
            task_timeout: self.task_timeout,
        })
    }
}

impl Default for BatchIndexConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct BatchIndexer<T> {
    config: BatchIndexConfig,
    db: Arc<DbConnection>,
    rpc_provider: Arc<T>,
    should_terminate: Arc<AtomicBool>,
}

impl<T> BatchIndexer<T>
where
    T: EthereumRpcProvider + Send + Sync + 'static,
{
    pub const fn new(
        config: BatchIndexConfig,
        db: Arc<DbConnection>,
        rpc_provider: Arc<T>,
        should_terminate: Arc<AtomicBool>,
    ) -> Self {
        Self {
            config,
            db,
            rpc_provider,
            should_terminate,
        }
    }

    // TODO: Since this is similar to the quick indexer with the only exception being the block logic,
    // maybe we could DRY this?
    pub async fn index(&self) -> Result<()> {
        // Batch indexer loop. does the following until terminated:
        // 1. check if finished indexing (backfilling block is 0)
        // 2. check current starting block and backfilling block
        // 3. if not fully indexed, index the block in batches (20 seems to be good for batch, but should be adjustable)
        // 4. if it is fully indexed, do nothing (maybe we could exit this too?)
        while !self.should_terminate.load(Ordering::Relaxed) {
            let current_index_metadata = self.get_current_metadata().await?;

            let index_start_block_number = current_index_metadata.indexing_starting_block_number;
            let current_backfilling_block_number = current_index_metadata
                .backfilling_block_number
                .unwrap_or(index_start_block_number);

            if Self::is_backfilling_complete(
                current_backfilling_block_number,
                index_start_block_number,
            ) {
                break;
            }

            let (starting_block, ending_block) = self
                .calculate_block_range(current_backfilling_block_number, index_start_block_number);

            self.index_block_range(starting_block, ending_block, &self.should_terminate)
                .await?;
        }
        Ok(())
    }

    async fn get_current_metadata(
        &self,
    ) -> Result<crate::repositories::index_metadata::IndexMetadataDto> {
        match get_index_metadata(self.db.clone()).await {
            Ok(Some(metadata)) => Ok(metadata),
            Ok(None) => {
                error!("[batch_index] Error getting index metadata");
                Err(eyre!("Error getting index metadata: metadata not found."))
            }
            Err(e) => {
                error!("[batch_index] Error getting index metadata: {}", e);
                Err(e)
            }
        }
    }

    fn is_backfilling_complete(
        current_backfilling_block_number: i64,
        index_start_block_number: i64,
    ) -> bool {
        if current_backfilling_block_number <= index_start_block_number {
            info!("[batch_index] Backfilling complete, reached starting block {}. Terminating backfilling process.", index_start_block_number);
            return true;
        }
        false
    }

    fn calculate_block_range(
        &self,
        current_backfilling_block_number: i64,
        index_start_block_number: i64,
    ) -> (i64, i64) {
        let backfilling_target_block =
            current_backfilling_block_number - i64::from(self.config.index_batch_size);
        let starting_block_number: i64 = if backfilling_target_block < index_start_block_number {
            index_start_block_number
        } else {
            backfilling_target_block
        };
        (starting_block_number, current_backfilling_block_number)
    }

    // Indexing a block range, inclusive.
    async fn index_block_range(
        &self,
        starting_block: i64,
        ending_block: i64,
        should_terminate: &AtomicBool,
    ) -> Result<()> {
        let block_range: Vec<i64> = (starting_block..=ending_block).collect();
        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent_requests));

        for i in 0..self.config.max_retries {
            if should_terminate.load(Ordering::Relaxed) {
                info!("[batch_index] Termination requested. Stopping quick indexing.");
                break;
            }

            let (block_headers, error_count, failed_blocks) =
                self.fetch_block_headers(&block_range, &semaphore).await?;

            self.apply_backpressure_if_needed(
                error_count,
                &block_range,
                starting_block,
                ending_block,
                &failed_blocks,
            )
            .await;

            if error_count == 0 {
                self.save_blocks_to_database(block_headers, starting_block, ending_block)
                    .await?;
                return Ok(());
            }

            // If there's an error during rpc, retry.
            error!("[batch_index] Error encountered during rpc, retry no. {}. Re-running from block: {}", i, starting_block);

            self.apply_retry_backoff(i).await;
        }

        Err(eyre!("Max retries reached. Stopping batch indexing."))
    }

    async fn fetch_block_headers(
        &self,
        block_range: &[i64],
        semaphore: &Arc<Semaphore>,
    ) -> Result<(Vec<crate::rpc::BlockHeader>, i32, Vec<i64>)> {
        let rpc_block_headers_futures = self.create_block_fetch_tasks(block_range, semaphore);
        let rpc_block_headers_response = join_all(rpc_block_headers_futures).await;

        Ok(Self::process_block_responses(
            rpc_block_headers_response,
            block_range[0],
        ))
    }

    fn create_block_fetch_tasks(
        &self,
        block_range: &[i64],
        semaphore: &Arc<Semaphore>,
    ) -> Vec<task::JoinHandle<Result<crate::rpc::BlockHeader, eyre::Report>>> {
        let timeout = self.config.rpc_timeout;
        let should_index_txs = self.config.should_index_txs;
        let task_timeout = self.config.task_timeout;

        block_range
            .iter()
            .map(|&block_number| {
                let provider = self.rpc_provider.clone();
                let permit = semaphore.clone();

                task::spawn(async move {
                    let _permit = permit
                        .acquire()
                        .await
                        .map_err(|_| eyre!("Semaphore closed"))?;

                    // Add timeout to the entire RPC operation
                    tokio::time::timeout(
                        Duration::from_secs(task_timeout.into()),
                        provider.get_full_block_by_number(
                            BlockNumber::from_trusted(block_number),
                            should_index_txs,
                            Some(timeout.into()),
                        ),
                    )
                    .await
                    .map_err(|_| eyre!("Task timeout getting block {}", block_number))?
                    .map_err(|e| eyre!("RPC error: {}", e))
                })
            })
            .collect()
    }

    fn process_block_responses(
        responses: Vec<
            Result<Result<crate::rpc::BlockHeader, eyre::Report>, tokio::task::JoinError>,
        >,
        starting_block: i64,
    ) -> (Vec<crate::rpc::BlockHeader>, i32, Vec<i64>) {
        let mut block_headers = Vec::with_capacity(responses.len());
        let mut error_count = 0;
        let mut failed_blocks = Vec::new();

        for (idx, join_result) in responses.into_iter().enumerate() {
            let block_num = starting_block + i64::try_from(idx).unwrap_or(0);
            match join_result {
                Ok(Ok(header)) => {
                    block_headers.push(header);
                }
                Ok(Err(e)) => {
                    error_count += 1;
                    failed_blocks.push(block_num);
                    warn!(
                        "[batch_index] Error retrieving block {}. error: {}",
                        block_num, e
                    );
                }
                Err(e) => {
                    error_count += 1;
                    failed_blocks.push(block_num);
                    warn!(
                        "[batch_index] Error retrieving block {}. error: {}",
                        block_num, e
                    );
                }
            }
        }

        (block_headers, error_count, failed_blocks)
    }

    #[allow(clippy::cast_precision_loss)]
    async fn apply_backpressure_if_needed(
        &self,
        error_count: i32,
        block_range: &[i64],
        starting_block: i64,
        ending_block: i64,
        failed_blocks: &[i64],
    ) {
        let error_rate = f64::from(error_count) / block_range.len() as f64;
        if error_rate > 0.2 {
            warn!(
                "[batch_index] High error rate ({:.1}%) in range {} to {}. Failed blocks: {:?}",
                error_rate * 100.0,
                starting_block,
                ending_block,
                failed_blocks
            );

            // Apply exponential backoff for high error rates
            let backoff_secs = if error_rate > 0.5 { 10 } else { 5 };
            tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
        }
    }

    async fn save_blocks_to_database(
        &self,
        block_headers: Vec<crate::rpc::BlockHeader>,
        starting_block: i64,
        ending_block: i64,
    ) -> Result<()> {
        let mut db_tx = self.db.pool.begin().await?;

        if self.config.should_index_txs {
            insert_block_header_query(&mut db_tx, block_headers).await?;
        } else {
            insert_block_header_only_query(&mut db_tx, block_headers).await?;
        }

        update_backfilling_block_number_query(&mut db_tx, starting_block).await?;

        if starting_block == 0 {
            set_is_backfilling(&mut db_tx, false).await?;
        }

        // Commit at the end
        db_tx.commit().await?;

        info!(
            "[batch_index] Indexing block range from {} to {} complete.",
            starting_block, ending_block
        );
        Ok(())
    }

    async fn apply_retry_backoff(&self, attempt: u8) {
        let backoff = u64::from(attempt).pow(2) * 5;
        tokio::time::sleep(Duration::from_secs(backoff)).await;
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
    #[serial_test::serial]
    async fn test_batch_indexer_new(
        _pool_options: PoolOptions<sqlx::Postgres>,
        connect_options: impl ConnectOptions,
    ) {
        let config = BatchIndexConfig::default();
        let url = connect_options.to_url_lossy().to_string();
        let db = DbConnection::new(url).await.unwrap();
        let mock_rpc = Arc::new(MockRpcProvider::new());
        let should_terminate = Arc::new(AtomicBool::new(false));

        let indexer = BatchIndexer::new(config, db, mock_rpc, should_terminate);

        assert_eq!(indexer.config.max_retries, 10);
        assert_eq!(indexer.config.poll_interval, 10);
        assert_eq!(indexer.config.rpc_timeout, 300);
        assert_eq!(indexer.config.index_batch_size, 50);
        assert!(!indexer.config.should_index_txs);
    }

    #[sqlx::test]
    #[serial_test::serial]
    async fn test_batch_index_default_headers_only(
        _pool_options: PoolOptions<sqlx::Postgres>,
        connect_options: impl ConnectOptions,
    ) {
        let block_fixtures = get_fixtures_for_tests().await;
        let url = connect_options.to_url_lossy().to_string();

        let config = BatchIndexConfig::default();
        let db = DbConnection::new(url).await.unwrap();
        let mock_rpc = Arc::new(MockRpcProvider::new_with_data(
            vec![].into(),
            vec![
                block_fixtures[5].clone(),
                block_fixtures[4].clone(),
                block_fixtures[3].clone(),
                block_fixtures[2].clone(),
                block_fixtures[1].clone(),
                block_fixtures[0].clone(),
            ]
            .into(),
        ));
        let should_terminate = Arc::new(AtomicBool::new(false));

        // Set initial db state, setting to 5 as current and 0 as starting to enable backfilling
        set_initial_indexing_status(db.clone(), 5, 0, true)
            .await
            .unwrap();

        // Manually set backfilling_block_number to enable backfilling work
        let mut tx = db.pool.begin().await.unwrap();
        sqlx::query("UPDATE index_metadata SET backfilling_block_number = 5")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let indexer = BatchIndexer::new(config, db.clone(), mock_rpc, should_terminate.clone());

        task::spawn(async move {
            indexer.index().await.unwrap();
        });

        // Wait until its indexed
        sleep(Duration::from_secs(1)).await;
        should_terminate.store(true, Ordering::Relaxed);

        let metadata = get_index_metadata(db.clone()).await.unwrap().unwrap();
        assert_eq!(metadata.backfilling_block_number, Some(0));
        assert!(!metadata.is_backfilling);

        // Since quick indexes from the latest, it should only index up to 5.
        // Check if the block is indexed correctly.
        let result: Result<Vec<BlockHeaderDto>, sqlx::Error> =
            sqlx::query_as("SELECT * FROM blockheaders order by number asc")
                .fetch_all(&mut *db.pool.acquire().await.unwrap())
                .await;
        assert!(result.is_ok());

        let result = result.unwrap();
        assert_eq!(result.len(), block_fixtures.len());
        // If the block is indexed correctly, the hash should match.
        assert_eq!(result[0].block_hash, block_fixtures[0].hash);
        assert_eq!(result[1].block_hash, block_fixtures[1].hash);
        assert_eq!(result[2].block_hash, block_fixtures[2].hash);
        assert_eq!(result[3].block_hash, block_fixtures[3].hash);
        assert_eq!(result[4].block_hash, block_fixtures[4].hash);
        assert_eq!(result[5].block_hash, block_fixtures[5].hash);

        // Check if the transactions are not indexed.
        let tx_result: Result<Vec<TransactionDto>, sqlx::Error> =
            sqlx::query_as("SELECT * FROM transactions")
                .fetch_all(&mut *db.pool.acquire().await.unwrap())
                .await;
        assert!(tx_result.is_ok());
        assert!(tx_result.unwrap().is_empty());
    }

    #[sqlx::test]
    #[serial_test::serial]
    async fn test_batch_index_with_tx(
        _pool_options: PoolOptions<sqlx::Postgres>,
        connect_options: impl ConnectOptions,
    ) {
        let block_fixtures = get_fixtures_for_tests().await;
        let url = connect_options.to_url_lossy().to_string();

        let config = BatchIndexConfig {
            should_index_txs: true,
            ..BatchIndexConfig::default()
        };
        let db = DbConnection::new(url).await.unwrap();
        let mock_rpc = Arc::new(MockRpcProvider::new_with_data(
            vec![].into(),
            vec![
                block_fixtures[5].clone(),
                block_fixtures[4].clone(),
                block_fixtures[3].clone(),
                block_fixtures[2].clone(),
                block_fixtures[1].clone(),
                block_fixtures[0].clone(),
            ]
            .into(),
        ));
        let should_terminate = Arc::new(AtomicBool::new(false));

        // Set initial db state, setting to 5 as current and 0 as starting to enable backfilling
        set_initial_indexing_status(db.clone(), 5, 0, true)
            .await
            .unwrap();

        // Manually set backfilling_block_number to enable backfilling work
        let mut tx = db.pool.begin().await.unwrap();
        sqlx::query("UPDATE index_metadata SET backfilling_block_number = 5")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let indexer = BatchIndexer::new(config, db.clone(), mock_rpc, should_terminate.clone());

        task::spawn(async move {
            indexer.index().await.unwrap();
        });

        // Wait until its indexed
        sleep(Duration::from_secs(1)).await;
        should_terminate.store(true, Ordering::Relaxed);

        let metadata = get_index_metadata(db.clone()).await.unwrap().unwrap();
        assert_eq!(metadata.backfilling_block_number, Some(0));
        assert!(!metadata.is_backfilling);

        // Since quick indexes from the latest, it should only index up to 5.
        // Check if the block is indexed correctly.
        let result: Result<Vec<BlockHeaderDto>, sqlx::Error> =
            sqlx::query_as("SELECT * FROM blockheaders order by number asc")
                .fetch_all(&mut *db.pool.acquire().await.unwrap())
                .await;
        assert!(result.is_ok());

        let result = result.unwrap();
        assert_eq!(result.len(), block_fixtures.len());
        // If the block is indexed correctly, the hash should match.
        assert_eq!(result[0].block_hash, block_fixtures[0].hash);
        assert_eq!(result[1].block_hash, block_fixtures[1].hash);
        assert_eq!(result[2].block_hash, block_fixtures[2].hash);
        assert_eq!(result[3].block_hash, block_fixtures[3].hash);
        assert_eq!(result[4].block_hash, block_fixtures[4].hash);
        assert_eq!(result[5].block_hash, block_fixtures[5].hash);

        // Check if the transactions are indexed correctly.
        for block_fixture in &block_fixtures {
            let tx_result: Result<Vec<TransactionDto>, sqlx::Error> =
                sqlx::query_as("SELECT * FROM transactions WHERE block_number = $1")
                    .bind(convert_hex_string_to_i64(&block_fixture.number).unwrap())
                    .fetch_all(&mut *db.pool.acquire().await.unwrap())
                    .await;
            assert!(tx_result.is_ok());

            let tx_result = tx_result.unwrap();
            assert_eq!(tx_result.len(), block_fixture.transactions.len());
            for (j, tx) in tx_result.iter().enumerate() {
                assert_eq!(
                    tx.block_number,
                    convert_hex_string_to_i64(&block_fixture.number).unwrap()
                );
                let fixtures_tx: Transaction =
                    block_fixture.transactions[j].clone().try_into().unwrap();
                assert_eq!(tx.transaction_hash, fixtures_tx.hash);
            }
        }
    }

    #[sqlx::test]
    #[serial_test::serial]
    async fn test_quick_index_always_index_away_from_latest_blocknumber_with_tx(
        _pool_options: PoolOptions<sqlx::Postgres>,
        connect_options: impl ConnectOptions,
    ) {
        let block_fixtures = get_fixtures_for_tests().await;
        let url = connect_options.to_url_lossy().to_string();

        let config = BatchIndexConfig {
            should_index_txs: true,
            ..BatchIndexConfig::default()
        };
        let db = DbConnection::new(url).await.unwrap();
        let should_terminate = Arc::new(AtomicBool::new(false));

        // In this case let's simulate that the latest block number starts from 3 instead.
        let mock_rpc = Arc::new(MockRpcProvider::new_with_data(
            vec![].into(),
            vec![
                block_fixtures[3].clone(),
                block_fixtures[2].clone(),
                block_fixtures[1].clone(),
                block_fixtures[0].clone(),
            ]
            .into(),
        ));

        // Set initial db state, setting to 3 as current and 0 as starting to enable backfilling
        set_initial_indexing_status(db.clone(), 3, 0, true)
            .await
            .unwrap();

        // Manually set backfilling_block_number to enable backfilling work
        let mut tx = db.pool.begin().await.unwrap();
        sqlx::query("UPDATE index_metadata SET backfilling_block_number = 3")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let indexer = BatchIndexer::new(config, db.clone(), mock_rpc, should_terminate.clone());

        task::spawn(async move {
            indexer.index().await.unwrap();
        });

        // Wait until its indexed
        sleep(Duration::from_secs(1)).await;
        should_terminate.store(true, Ordering::Relaxed);

        let metadata = get_index_metadata(db.clone()).await.unwrap().unwrap();
        assert_eq!(metadata.backfilling_block_number, Some(0));
        assert!(!metadata.is_backfilling);

        // Since quick indexes from the latest, it should only index up to 5.
        // Check if the block is indexed correctly.
        let result: Result<Vec<BlockHeaderDto>, sqlx::Error> =
            sqlx::query_as("SELECT * FROM blockheaders order by number asc")
                .fetch_all(&mut *db.pool.acquire().await.unwrap())
                .await;
        assert!(result.is_ok());

        let result = result.unwrap();
        assert_eq!(result.len(), 4);
        // If the block is indexed correctly, the hash should match.
        assert_eq!(result[0].block_hash, block_fixtures[0].hash);
        assert_eq!(result[1].block_hash, block_fixtures[1].hash);
        assert_eq!(result[2].block_hash, block_fixtures[2].hash);
        assert_eq!(result[3].block_hash, block_fixtures[3].hash);

        // Check if the transactions are indexed correctly.
        for block_fixture in &block_fixtures[0..=3] {
            let tx_result: Result<Vec<TransactionDto>, sqlx::Error> =
                sqlx::query_as("SELECT * FROM transactions WHERE block_number = $1")
                    .bind(convert_hex_string_to_i64(&block_fixture.number).unwrap())
                    .fetch_all(&mut *db.pool.acquire().await.unwrap())
                    .await;
            assert!(tx_result.is_ok());

            let tx_result = tx_result.unwrap();
            assert_eq!(tx_result.len(), block_fixture.transactions.len());
            for (j, tx) in tx_result.iter().enumerate() {
                assert_eq!(
                    tx.block_number,
                    convert_hex_string_to_i64(&block_fixture.number).unwrap()
                );
                let fixtures_tx: Transaction =
                    block_fixture.transactions[j].clone().try_into().unwrap();
                assert_eq!(tx.transaction_hash, fixtures_tx.hash);
            }
        }
    }

    #[sqlx::test]
    #[serial_test::serial]
    async fn test_failed_to_get_index_metadata(
        _pool_options: PoolOptions<sqlx::Postgres>,
        connect_options: impl ConnectOptions,
    ) {
        let config = BatchIndexConfig::default();
        let url = connect_options.to_url_lossy().to_string();

        let db = DbConnection::new(url).await.unwrap();
        let mock_rpc = Arc::new(MockRpcProvider::new());
        let should_terminate = Arc::new(AtomicBool::new(false));

        // The metadata table would not exist without initializing it.
        let indexer = BatchIndexer::new(config, db.clone(), mock_rpc, should_terminate.clone());

        let result = indexer.index().await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Error getting index metadata: metadata not found."
        );
    }

    #[sqlx::test]
    #[serial_test::serial]
    async fn test_should_retry_when_rpc_error_happens(
        _pool_options: PoolOptions<sqlx::Postgres>,
        connect_options: impl ConnectOptions,
    ) {
        let url = connect_options.to_url_lossy().to_string();

        // Reduce the max tries to 2 to speed up the test.
        let config = BatchIndexConfig {
            max_retries: 2,
            ..BatchIndexConfig::default()
        };
        let db = DbConnection::new(url).await.unwrap();

        // Empty the rpc response for get_block_by_number, so it will attempt to retry
        let mock_rpc = Arc::new(MockRpcProvider::new_with_data(vec![].into(), vec![].into()));
        let should_terminate = Arc::new(AtomicBool::new(false));

        // Set initial db state, setting up backfilling work that will trigger RPC calls
        set_initial_indexing_status(db.clone(), 5, 0, true)
            .await
            .unwrap();

        // Manually set backfilling_block_number to something higher than starting block to force work
        let mut tx = db.pool.begin().await.unwrap();
        sqlx::query("UPDATE index_metadata SET backfilling_block_number = 10")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let indexer = BatchIndexer::new(config, db.clone(), mock_rpc, should_terminate.clone());

        let result = indexer.index().await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Max retries reached. Stopping batch indexing."
        );
    }
}
