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
        index_metadata::{get_index_metadata, set_is_backfilling},
    },
    rpc::EthereumRpcProvider,
    types::BlockNumber,
    utils::convert_hex_string_to_i64,
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
        info!("[batch_index] Starting SMART gap-filling batch indexer");

        // Debug initial state
        let initial_metadata = self.get_current_metadata().await?;
        info!("[batch_index] Initial metadata - latest: {}, starting: {}, is_backfilling: {}, backfilling_block_number: {:?}",
              initial_metadata.current_latest_block_number,
              initial_metadata.indexing_starting_block_number,
              initial_metadata.is_backfilling,
              initial_metadata.backfilling_block_number);

        while !self.should_terminate.load(Ordering::Relaxed) {
            if self.process_missing_blocks_cycle().await? {
                break; // Backfill complete
            }

            // Sleep before next scan to avoid overwhelming the system
            tokio::time::sleep(Duration::from_millis(u64::from(self.config.poll_interval))).await;
        }
        Ok(())
    }

    #[allow(clippy::cognitive_complexity)]
    async fn process_missing_blocks_cycle(&self) -> Result<bool> {
        let current_index_metadata = self.get_current_metadata().await?;
        let index_start_block_number = current_index_metadata.indexing_starting_block_number;
        let backfilling_block_number = current_index_metadata
            .backfilling_block_number
            .unwrap_or(current_index_metadata.current_latest_block_number);

        Self::validate_backfill_configuration(index_start_block_number);

        info!(
            "[batch_index] Scanning for missing blocks from {} to {} (batch indexer range)",
            index_start_block_number, backfilling_block_number
        );

        let missing_blocks = self
            .find_missing_blocks(index_start_block_number, backfilling_block_number)
            .await?;

        info!(
            "[batch_index] Found {} missing blocks: {:?}",
            missing_blocks.len(),
            missing_blocks
        );

        if missing_blocks.is_empty() {
            info!("[batch_index] No missing blocks found! Backfill complete.");
            self.mark_backfill_complete().await?;
            return Ok(true); // Backfill complete
        }

        info!(
            "[batch_index] Found {} missing blocks. Starting targeted gap filling.",
            missing_blocks.len()
        );

        self.process_missing_blocks_in_batches(&missing_blocks)
            .await?;

        info!("[batch_index] Finished processing missing blocks, checking completion for start_block {}", index_start_block_number);

        // Check if we've filled all blocks to the start - if so, we're done
        if self
            .is_backfill_complete(index_start_block_number, backfilling_block_number)
            .await?
        {
            info!("[batch_index] All missing blocks filled, marking backfill complete.");
            info!(
                "[batch_index] Setting backfilling_block_number to {}",
                index_start_block_number
            );
            // Set backfilling_block_number to the target (should be starting block)
            self.update_backfilling_progress(index_start_block_number)
                .await?;
            self.mark_backfill_complete().await?;
            return Ok(true); // Backfill complete
        }
        Ok(false) // Continue processing
    }

    fn validate_backfill_configuration(index_start_block_number: i64) {
        if index_start_block_number != 0 {
            warn!(
                "[batch_index] indexing_starting_block_number is {}, should be 0 for complete backfill",
                index_start_block_number
            );
        }
    }

    async fn find_missing_blocks(
        &self,
        start_block: i64,
        end_block: i64,
    ) -> Result<Vec<BlockNumber>> {
        // Use the indexer's own database connection instead of the global one
        self.find_missing_blocks_with_db(start_block, end_block)
            .await
    }

    async fn find_missing_blocks_with_db(
        &self,
        start_block: i64,
        end_block: i64,
    ) -> Result<Vec<BlockNumber>> {
        let mut missing_blocks = Vec::new();
        let mut current_start = start_block;
        let chunk_size = 10000i64;

        while current_start <= end_block {
            let current_end = std::cmp::min(current_start + chunk_size - 1, end_block);

            let result: Vec<(i64,)> = sqlx::query_as(
                r"
                SELECT n FROM generate_series($1::bigint, $2::bigint) AS n
                WHERE n NOT IN (SELECT number FROM blockheaders WHERE number BETWEEN $1 AND $2)
                ORDER BY n
                ",
            )
            .bind(current_start)
            .bind(current_end)
            .fetch_all(&self.db.pool)
            .await
            .map_err(|e| {
                eyre!(
                    "Failed to find missing blocks in range {}-{}: {}",
                    current_start,
                    current_end,
                    e
                )
            })?;

            missing_blocks.extend(
                result
                    .into_iter()
                    .map(|(block_num,)| BlockNumber::from_trusted(block_num)),
            );

            current_start = current_end + 1;
        }

        Ok(missing_blocks)
    }

    async fn process_missing_blocks_in_batches(
        &self,
        missing_blocks: &[BlockNumber],
    ) -> Result<()> {
        let batch_size = self.config.index_batch_size as usize;

        for batch in missing_blocks.chunks(batch_size) {
            if self.should_terminate.load(Ordering::Relaxed) {
                info!("[batch_index] Termination requested. Stopping gap filling.");
                break;
            }
            self.fill_missing_blocks(batch).await?;
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

    /// Marks backfill as complete by disabling backfilling flag
    async fn mark_backfill_complete(&self) -> Result<()> {
        let mut tx = self
            .db
            .pool
            .begin()
            .await
            .map_err(|e| eyre!("Failed to begin transaction: {}", e))?;

        set_is_backfilling(&mut tx, false).await?;

        tx.commit()
            .await
            .map_err(|e| eyre!("Failed to commit backfill completion: {}", e))?;

        info!("[batch_index] Backfill marked as complete");
        Ok(())
    }

    /// Fill specific missing blocks (smart gap filling)
    async fn fill_missing_blocks(&self, missing_blocks: &[BlockNumber]) -> Result<()> {
        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent_requests));

        for retry_attempt in 0..self.config.max_retries {
            if self.should_terminate.load(Ordering::Relaxed) {
                info!("[batch_index] Termination requested during gap filling");
                break;
            }

            if let Some(block_headers) = self
                .try_fetch_missing_blocks(missing_blocks, &semaphore, retry_attempt)
                .await?
            {
                self.store_gap_blocks(block_headers).await?;
                info!(
                    "[batch_index] Successfully filled {} missing blocks",
                    missing_blocks.len()
                );
                return Ok(());
            }

            // Apply backoff between retries
            self.apply_fetch_retry_backoff(retry_attempt).await;
        }

        Err(eyre!("Max retries reached filling missing blocks"))
    }

    async fn try_fetch_missing_blocks(
        &self,
        missing_blocks: &[BlockNumber],
        semaphore: &Arc<Semaphore>,
        retry_attempt: u8,
    ) -> Result<Option<Vec<crate::rpc::BlockHeader>>> {
        let (block_headers, error_count, failed_blocks) = self
            .fetch_specific_blocks(missing_blocks, semaphore)
            .await?;

        if error_count > 0 {
            self.log_fetch_errors(error_count, retry_attempt, &failed_blocks);

            if retry_attempt == self.config.max_retries - 1 {
                return Err(eyre!("Max retries reached. Stopping batch indexing."));
            }
            return Ok(None); // Retry needed
        }

        Ok(Some(block_headers)) // Success
    }

    fn log_fetch_errors(&self, error_count: i32, retry_attempt: u8, failed_blocks: &[i64]) {
        warn!(
            "[batch_index] {} blocks failed to fetch on attempt {}/{}. Failed blocks: {:?}",
            error_count,
            retry_attempt + 1,
            self.config.max_retries,
            failed_blocks
        );
    }

    async fn apply_fetch_retry_backoff(&self, retry_attempt: u8) {
        let backoff = (retry_attempt + 1).pow(2) * 5;
        tokio::time::sleep(Duration::from_secs(backoff.into())).await;
    }

    async fn is_backfill_complete(&self, start_block: i64, end_block: i64) -> Result<bool> {
        // Check if there are any missing blocks between start and end (inclusive)
        let missing_blocks = self.find_missing_blocks(start_block, end_block).await?;
        Ok(missing_blocks.is_empty())
    }

    async fn update_backfilling_progress(&self, target_block: i64) -> Result<()> {
        let mut tx = self
            .db
            .pool
            .begin()
            .await
            .map_err(|e| eyre!("Failed to begin transaction: {}", e))?;

        sqlx::query("UPDATE index_metadata SET backfilling_block_number = $1, updated_at = CURRENT_TIMESTAMP")
            .bind(target_block)
            .execute(&mut *tx)
            .await
            .map_err(|e| eyre!("Failed to update backfilling progress: {}", e))?;

        tx.commit()
            .await
            .map_err(|e| eyre!("Failed to commit backfilling progress: {}", e))?;

        Ok(())
    }

    /// Fetch specific blocks (not sequential ranges)
    async fn fetch_specific_blocks(
        &self,
        block_numbers: &[BlockNumber],
        semaphore: &Arc<Semaphore>,
    ) -> Result<(Vec<crate::rpc::BlockHeader>, i32, Vec<i64>)> {
        let rpc_block_headers_futures =
            self.create_specific_block_fetch_tasks(block_numbers, semaphore);
        let rpc_block_headers_response = join_all(rpc_block_headers_futures).await;

        Ok(Self::process_specific_block_responses(
            rpc_block_headers_response,
            block_numbers,
        ))
    }

    fn create_specific_block_fetch_tasks(
        &self,
        block_numbers: &[BlockNumber],
        semaphore: &Arc<Semaphore>,
    ) -> Vec<task::JoinHandle<Result<crate::rpc::BlockHeader, eyre::Report>>> {
        let timeout = self.config.rpc_timeout;
        let should_index_txs = self.config.should_index_txs;
        let task_timeout = self.config.task_timeout;

        block_numbers
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
                            block_number,
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

    fn process_specific_block_responses(
        responses: Vec<
            Result<Result<crate::rpc::BlockHeader, eyre::Report>, tokio::task::JoinError>,
        >,
        block_numbers: &[BlockNumber],
    ) -> (Vec<crate::rpc::BlockHeader>, i32, Vec<i64>) {
        let mut block_headers = Vec::with_capacity(responses.len());
        let mut error_count = 0;
        let mut failed_blocks = Vec::new();

        for (idx, join_result) in responses.into_iter().enumerate() {
            let expected_block_num = block_numbers[idx].value();

            if let Ok(header) = Self::process_single_block_response(join_result, expected_block_num)
            {
                block_headers.push(header);
            } else {
                error_count += 1;
                failed_blocks.push(expected_block_num);
            }
        }

        (block_headers, error_count, failed_blocks)
    }

    fn process_single_block_response(
        join_result: Result<Result<crate::rpc::BlockHeader, eyre::Report>, tokio::task::JoinError>,
        expected_block_num: i64,
    ) -> Result<crate::rpc::BlockHeader, ()> {
        match join_result {
            Ok(Ok(header)) => Self::validate_block_header(header, expected_block_num),
            Ok(Err(e)) => {
                warn!(
                    "[batch_index] Error retrieving block {}: {}",
                    expected_block_num, e
                );
                Err(())
            }
            Err(e) => {
                warn!(
                    "[batch_index] Task error for block {}: {}",
                    expected_block_num, e
                );
                Err(())
            }
        }
    }

    fn validate_block_header(
        header: crate::rpc::BlockHeader,
        expected_block_num: i64,
    ) -> Result<crate::rpc::BlockHeader, ()> {
        match convert_hex_string_to_i64(&header.number) {
            Ok(actual_block_num) if actual_block_num == expected_block_num => Ok(header),
            Ok(actual_block_num) => {
                warn!(
                    "[batch_index] Block number mismatch for block {}: expected {}, got {}",
                    expected_block_num, expected_block_num, actual_block_num
                );
                Err(())
            }
            Err(e) => {
                warn!(
                    "[batch_index] Invalid block number in header for block {}: {}",
                    expected_block_num, e
                );
                Err(())
            }
        }
    }

    /// Store gap-filled blocks (not necessarily sequential)
    async fn store_gap_blocks(&self, block_headers: Vec<crate::rpc::BlockHeader>) -> Result<()> {
        if block_headers.is_empty() {
            return Ok(());
        }

        // Sort blocks by number for consistent storage
        let mut sorted_headers = block_headers;
        sorted_headers.sort_by_key(|h| convert_hex_string_to_i64(&h.number).unwrap_or(0));

        // Begin atomic database transaction
        let mut db_tx = self.db.pool.begin().await?;

        if self.config.should_index_txs {
            insert_block_header_query(&mut db_tx, sorted_headers.clone()).await?;
        } else {
            insert_block_header_only_query(&mut db_tx, sorted_headers.clone()).await?;
        }

        // Commit transaction - if this fails, nothing was persisted
        db_tx.commit().await.map_err(|e| {
            error!(
                "[batch_index] Database transaction failed after gap block insertion: {}",
                e
            );
            eyre!("Database transaction commit failed: {}", e)
        })?;

        let block_numbers: Vec<i64> = sorted_headers
            .iter()
            .map(|h| convert_hex_string_to_i64(&h.number).unwrap_or(0))
            .collect();

        info!(
            "[batch_index] Stored {} gap blocks: {:?}",
            sorted_headers.len(),
            block_numbers
        );

        Ok(())
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
                "tests/fixtures/indexer/eth_getBlockByNumber_sepolia_{i}.json"
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
            {
                // Create enough mock data to handle retries (6 blocks * 15 attempts each)
                let mut mock_blocks = Vec::new();
                for _ in 0..15 {
                    mock_blocks.extend([
                        block_fixtures[0].clone(),
                        block_fixtures[1].clone(),
                        block_fixtures[2].clone(),
                        block_fixtures[3].clone(),
                        block_fixtures[4].clone(),
                        block_fixtures[5].clone(),
                    ]);
                }
                mock_blocks
            }
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

        let handle = task::spawn(async move {
            indexer.index().await.unwrap();
        });

        // Wait until its indexed
        sleep(Duration::from_secs(5)).await;
        should_terminate.store(true, Ordering::Relaxed);

        // Wait for the indexer to complete
        let _ = handle.await;

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
            {
                // Create enough mock data to handle retries (6 blocks * 15 attempts each)
                let mut mock_blocks = Vec::new();
                for _ in 0..15 {
                    mock_blocks.extend([
                        block_fixtures[0].clone(),
                        block_fixtures[1].clone(),
                        block_fixtures[2].clone(),
                        block_fixtures[3].clone(),
                        block_fixtures[4].clone(),
                        block_fixtures[5].clone(),
                    ]);
                }
                mock_blocks
            }
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

        let handle = task::spawn(async move {
            indexer.index().await.unwrap();
        });

        // Wait until its indexed
        sleep(Duration::from_secs(1)).await;
        should_terminate.store(true, Ordering::Relaxed);

        // Wait for the indexer to complete
        let _ = handle.await;

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
            {
                // Create enough mock data to handle retries
                // The batch indexer requests blocks 0,1,2,3 in order, so we need to provide them in that order
                let mut mock_blocks = Vec::new();
                for _ in 0..15 {
                    mock_blocks.extend([
                        block_fixtures[0].clone(),
                        block_fixtures[1].clone(),
                        block_fixtures[2].clone(),
                        block_fixtures[3].clone(),
                    ]);
                }
                mock_blocks
            }
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

        let handle = task::spawn(async move {
            indexer.index().await.unwrap();
        });

        // Wait until its indexed
        sleep(Duration::from_secs(1)).await;
        should_terminate.store(true, Ordering::Relaxed);

        // Wait for the indexer to complete
        let _ = handle.await;

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
