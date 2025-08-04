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
    errors::BlockchainError,
    repositories::{
        block_header::{insert_block_header_only_query, insert_block_header_query},
        index_metadata::{get_index_metadata, update_latest_quick_index_block_number_query},
    },
    rpc::EthereumRpcProvider,
    types::BlockNumber,
};

#[derive(Debug)]
pub struct QuickIndexConfig {
    pub max_retries: u8,
    pub poll_interval: u32,
    pub rpc_timeout: u32,
    pub index_batch_size: u32,
    pub should_index_txs: bool,
}

impl QuickIndexConfig {
    #[must_use]
    pub const fn builder() -> QuickIndexConfigBuilder {
        QuickIndexConfigBuilder::new()
    }
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

pub struct QuickIndexConfigBuilder {
    max_retries: u8,
    poll_interval: u32,
    rpc_timeout: u32,
    index_batch_size: u32,
    should_index_txs: bool,
}

impl QuickIndexConfigBuilder {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            max_retries: 10,
            poll_interval: 10,
            rpc_timeout: 300,
            index_batch_size: 20,
            should_index_txs: false,
        }
    }

    #[must_use]
    pub const fn fast_polling() -> Self {
        Self::new()
            .poll_interval(2)
            .index_batch_size(50)
            .rpc_timeout(120)
    }

    #[must_use]
    pub const fn slow_polling() -> Self {
        Self::new()
            .poll_interval(60)
            .index_batch_size(5)
            .max_retries(5)
    }

    #[must_use]
    pub const fn testing() -> Self {
        Self::new()
            .poll_interval(1)
            .index_batch_size(1)
            .rpc_timeout(30)
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

    pub fn build(self) -> Result<QuickIndexConfig, BlockchainError> {
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

        Ok(QuickIndexConfig {
            max_retries: self.max_retries,
            poll_interval: self.poll_interval,
            rpc_timeout: self.rpc_timeout,
            index_batch_size: self.index_batch_size,
            should_index_txs: self.should_index_txs,
        })
    }
}

impl Default for QuickIndexConfigBuilder {
    fn default() -> Self {
        Self::new()
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
    pub const fn new(
        config: QuickIndexConfig,
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

    pub async fn index(&self) -> Result<()> {
        // Quick indexer loop, does the following until terminated:
        // 1. check current latest block
        // 2. check if there's a new blocks finalized
        // 3. if yes, index the blocks
        // 4. if not, sleep for a period of time and do nothing
        while !self.should_terminate.load(Ordering::Relaxed) {
            self.process_indexing_cycle().await?;
        }

        info!("[quick_index] Process terminating.");
        Ok(())
    }

    async fn process_indexing_cycle(&self) -> Result<()> {
        let last_block_number = self.get_last_indexed_block().await?;
        let new_latest_block = self.get_latest_finalized_block().await?;

        if Self::has_new_blocks(last_block_number, new_latest_block) {
            self.index_new_blocks(last_block_number, new_latest_block)
                .await
        } else {
            self.wait_for_new_blocks(new_latest_block).await
        }
    }

    async fn get_last_indexed_block(&self) -> Result<i64> {
        match get_index_metadata(self.db.clone()).await {
            Ok(Some(metadata)) => Ok(metadata.current_latest_block_number),
            Ok(None) => {
                error!("[quick_index] Error getting index metadata");
                Err(eyre!("Error getting index metadata: metadata not found."))
            }
            Err(e) => {
                error!("[quick_index] Error getting index metadata: {}", e);
                Err(e)
            }
        }
    }

    async fn get_latest_finalized_block(&self) -> Result<BlockNumber> {
        self.rpc_provider
            .get_latest_finalized_blocknumber(Some(self.config.rpc_timeout.into()))
            .await
            .map_err(|e| eyre::eyre!("Failed to get latest finalized block: {}", e))
    }

    fn has_new_blocks(last_block_number: i64, new_latest_block: BlockNumber) -> bool {
        new_latest_block > BlockNumber::from_trusted(last_block_number)
    }

    async fn index_new_blocks(
        &self,
        last_block_number: i64,
        new_latest_block: BlockNumber,
    ) -> Result<()> {
        let ending_block_number = self.calculate_ending_block(last_block_number, new_latest_block);

        self.index_block_range(
            last_block_number + 1, // index from recorded last block + 1
            ending_block_number,
            &self.should_terminate,
        )
        .await
    }

    fn calculate_ending_block(&self, last_block_number: i64, new_latest_block: BlockNumber) -> i64 {
        let block_difference = new_latest_block - BlockNumber::from_trusted(last_block_number);
        let batch_size = BlockNumber::from_trusted(self.config.index_batch_size.into());

        if block_difference > batch_size {
            last_block_number + i64::from(self.config.index_batch_size)
        } else {
            new_latest_block.value()
        }
    }

    async fn wait_for_new_blocks(&self, new_latest_block: BlockNumber) -> Result<()> {
        info!(
            "No new block finalized. Latest: {}. Sleeping for {}s...",
            new_latest_block, self.config.poll_interval
        );
        tokio::time::sleep(Duration::from_secs(self.config.poll_interval.into())).await;
        Ok(())
    }

    // Indexing a block range, inclusive.
    async fn index_block_range(
        &self,
        starting_block: i64,
        ending_block: i64,
        should_terminate: &AtomicBool,
    ) -> Result<()> {
        let block_range: Vec<i64> = (starting_block..=ending_block).collect();
        self.retry_with_backoff(&block_range, starting_block, ending_block, should_terminate)
            .await
    }

    async fn retry_with_backoff(
        &self,
        block_range: &[i64],
        starting_block: i64,
        ending_block: i64,
        should_terminate: &AtomicBool,
    ) -> Result<()> {
        for retry_attempt in 0..self.config.max_retries {
            if Self::check_termination_signal(should_terminate) {
                break;
            }

            if matches!(
                self.try_fetch_and_store(block_range, starting_block, ending_block)
                    .await,
                Ok(())
            ) {
                Self::log_indexing_success(starting_block, ending_block);
                return Ok(());
            }

            self.log_and_backoff(retry_attempt, starting_block).await;
        }

        Err(eyre!("Max retries reached. Stopping quick indexing."))
    }

    fn check_termination_signal(should_terminate: &AtomicBool) -> bool {
        if should_terminate.load(Ordering::Relaxed) {
            info!("[quick_index] Termination requested. Stopping quick indexing.");
            true
        } else {
            false
        }
    }

    async fn try_fetch_and_store(
        &self,
        block_range: &[i64],
        starting_block: i64,
        ending_block: i64,
    ) -> Result<()> {
        self.fetch_and_store_blocks(block_range, starting_block, ending_block)
            .await
    }

    fn log_indexing_success(starting_block: i64, ending_block: i64) {
        info!(
            "[quick_index] Indexing block range from {} to {} complete.",
            starting_block, ending_block
        );
    }

    async fn log_and_backoff(&self, retry_attempt: u8, starting_block: i64) {
        error!(
            "[quick_index] Error encountered during rpc, retry no. {}. Re-running from block: {}.",
            retry_attempt, starting_block
        );
        self.apply_exponential_backoff(retry_attempt).await;
    }

    async fn fetch_and_store_blocks(
        &self,
        block_range: &[i64],
        starting_block: i64,
        ending_block: i64,
    ) -> Result<()> {
        let block_headers = self
            .fetch_block_headers(block_range, starting_block, ending_block)
            .await?;
        self.store_block_headers(block_headers, ending_block).await
    }

    async fn fetch_block_headers(
        &self,
        block_range: &[i64],
        starting_block: i64,
        ending_block: i64,
    ) -> Result<Vec<crate::rpc::BlockHeader>> {
        let rpc_futures = self.create_rpc_futures(block_range);
        let rpc_responses = try_join_all(rpc_futures).await?;
        Self::process_rpc_responses(rpc_responses, starting_block, ending_block)
    }

    fn create_rpc_futures(
        &self,
        block_range: &[i64],
    ) -> Vec<task::JoinHandle<Result<crate::rpc::BlockHeader, crate::errors::BlockchainError>>>
    {
        let timeout = self.config.rpc_timeout;
        let should_index_txs = self.config.should_index_txs;

        block_range
            .iter()
            .map(|&block_number| {
                let provider = self.rpc_provider.clone();
                task::spawn(async move {
                    provider
                        .get_full_block_by_number(
                            BlockNumber::from_trusted(block_number),
                            should_index_txs,
                            Some(timeout.into()),
                        )
                        .await
                })
            })
            .collect()
    }

    fn process_rpc_responses(
        rpc_responses: Vec<Result<crate::rpc::BlockHeader, crate::errors::BlockchainError>>,
        starting_block: i64,
        ending_block: i64,
    ) -> Result<Vec<crate::rpc::BlockHeader>> {
        let mut block_headers = Vec::with_capacity(rpc_responses.len());
        let mut has_err = false;

        for response in rpc_responses {
            match response {
                Ok(header) => block_headers.push(header),
                Err(e) => {
                    has_err = true;
                    warn!(
                        "[quick_index] Error retrieving block in range from {} to {}. error: {}",
                        starting_block, ending_block, e
                    );
                }
            }
        }

        if has_err {
            return Err(eyre!("One or more RPC calls failed"));
        }

        Ok(block_headers)
    }

    async fn store_block_headers(
        &self,
        block_headers: Vec<crate::rpc::BlockHeader>,
        ending_block: i64,
    ) -> Result<()> {
        let mut db_tx = self.db.pool.begin().await?;

        if self.config.should_index_txs {
            insert_block_header_query(&mut db_tx, block_headers).await?;
        } else {
            insert_block_header_only_query(&mut db_tx, block_headers).await?;
        }

        update_latest_quick_index_block_number_query(&mut db_tx, ending_block).await?;
        db_tx.commit().await?;

        Ok(())
    }

    async fn apply_exponential_backoff(&self, retry_count: u8) {
        let backoff = (u64::from(retry_count)).pow(2) * 5;
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
    async fn test_quick_indexer_new(
        _pool_options: PoolOptions<sqlx::Postgres>,
        connect_options: impl ConnectOptions,
    ) {
        let config = QuickIndexConfig::default();
        let url = connect_options.to_url_lossy().to_string();
        let db = DbConnection::new(url).await.unwrap();
        let mock_rpc = Arc::new(MockRpcProvider::new());
        let should_terminate = Arc::new(AtomicBool::new(false));

        let indexer = QuickIndexer::new(config, db, mock_rpc, should_terminate);

        assert_eq!(indexer.config.max_retries, 10);
        assert_eq!(indexer.config.poll_interval, 10);
        assert_eq!(indexer.config.rpc_timeout, 300);
        assert_eq!(indexer.config.index_batch_size, 20);
        assert!(!indexer.config.should_index_txs);
    }

    #[sqlx::test]
    #[serial_test::serial]
    async fn test_quick_index_default_headers_only(
        _pool_options: PoolOptions<sqlx::Postgres>,
        connect_options: impl ConnectOptions,
    ) {
        let block_fixtures = get_fixtures_for_tests().await;
        let url = connect_options.to_url_lossy().to_string();

        let config = QuickIndexConfig::default();
        let db = DbConnection::new(url).await.unwrap();
        let mock_rpc = Arc::new(MockRpcProvider::new_with_data(
            vec![BlockNumber::from_trusted(5); block_fixtures.len()].into(),
            vec![block_fixtures[5].clone(); 5].into(),
        ));
        let should_terminate = Arc::new(AtomicBool::new(false));

        // Set initial db state, setting to 4 as we want the indexer to start from 5.
        set_initial_indexing_status(db.clone(), 4, 4, true)
            .await
            .unwrap();

        let indexer = QuickIndexer::new(config, db.clone(), mock_rpc, should_terminate.clone());

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
    #[serial_test::serial]
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
            vec![BlockNumber::from_trusted(5); block_fixtures.len()].into(),
            vec![block_fixtures[5].clone(); 5].into(),
        ));
        let should_terminate = Arc::new(AtomicBool::new(false));

        // Set initial db state, setting to 4 as we want the indexer to start from 5.
        set_initial_indexing_status(db.clone(), 4, 4, true)
            .await
            .unwrap();

        let indexer = QuickIndexer::new(config, db.clone(), mock_rpc, should_terminate.clone());

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
    #[serial_test::serial]
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
            vec![BlockNumber::from_trusted(5), BlockNumber::from_trusted(5)].into(),
            vec![block_fixtures[4].clone(), block_fixtures[5].clone()].into(),
        ));

        // Set initial db state, setting to 3 as we want the indexer to start from 4.
        set_initial_indexing_status(db.clone(), 3, 3, true)
            .await
            .unwrap();

        let indexer = QuickIndexer::new(config, db.clone(), mock_rpc, should_terminate.clone());

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
    #[serial_test::serial]
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
        let indexer = QuickIndexer::new(config, db.clone(), mock_rpc, should_terminate.clone());

        let result = indexer.index().await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Error getting index metadata: metadata not found."
        );
    }

    #[sqlx::test]
    #[serial_test::serial]
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

        let indexer = QuickIndexer::new(config, db.clone(), mock_rpc, should_terminate.clone());

        let result = indexer.index().await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Failed to get latest finalized block: Block not found: Failed to get latest finalized block number"
        );
    }

    #[sqlx::test]
    #[serial_test::serial]
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
            vec![BlockNumber::from_trusted(5); block_fixtures.len()].into(),
            vec![].into(),
        ));
        let should_terminate = Arc::new(AtomicBool::new(false));

        // Set initial db state, setting to 4 as we want the indexer to start from 5.
        set_initial_indexing_status(db.clone(), 4, 4, true)
            .await
            .unwrap();

        let indexer = QuickIndexer::new(config, db.clone(), mock_rpc, should_terminate.clone());

        let result = indexer.index().await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Max retries reached. Stopping quick indexing."
        );
    }
}
