use std::{
    sync::{atomic::AtomicBool, Arc},
    // thread::{self, JoinHandle},
};

use crate::{
    db::{find_first_gap, DbConnection},
    errors::{BlockchainError, Result},
    indexer::{
        batch_service::{BatchIndexConfig, BatchIndexer},
        quick_service::{QuickIndexConfig, QuickIndexer},
    },
    repositories::index_metadata::{
        get_index_metadata, set_initial_indexing_status, update_backfilling_block_number_query,
        IndexMetadataDto,
    },
    router,
    rpc::{EthereumJsonRpcClient, EthereumRpcProvider},
    types::BlockNumber,
};
use tokio::task::JoinHandle;
use tracing::{error, info};

#[derive(Debug)]
pub struct IndexingConfig {
    pub db_conn_string: String,
    pub node_conn_string: String,
    pub should_index_txs: bool,
    pub max_retries: u8,
    pub poll_interval: u32,
    pub rpc_timeout: u32,
    pub rpc_max_retries: u32,
    pub index_batch_size: u32,
    pub start_block_offset: Option<u64>,
}

impl IndexingConfig {
    #[must_use]
    pub const fn builder() -> IndexingConfigBuilder {
        IndexingConfigBuilder::new()
    }
}

pub struct IndexingConfigBuilder {
    db_conn_string: Option<String>,
    node_conn_string: Option<String>,
    should_index_txs: bool,
    max_retries: u8,
    poll_interval: u32,
    rpc_timeout: u32,
    rpc_max_retries: u32,
    index_batch_size: u32,
    start_block_offset: Option<u64>,
}

impl IndexingConfigBuilder {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            db_conn_string: None,
            node_conn_string: None,
            should_index_txs: false,
            max_retries: 10,
            poll_interval: 10,
            rpc_timeout: 300,
            rpc_max_retries: 5,
            index_batch_size: 100,
            start_block_offset: None,
        }
    }

    #[must_use]
    pub const fn development() -> Self {
        Self::new()
            .max_retries(3)
            .poll_interval(5)
            .rpc_timeout(60)
            .rpc_max_retries(3)
            .index_batch_size(10)
    }

    #[must_use]
    pub const fn testing() -> Self {
        Self::new()
            .max_retries(1)
            .poll_interval(1)
            .rpc_timeout(30)
            .rpc_max_retries(1)
            .index_batch_size(5)
    }

    #[must_use]
    pub const fn production() -> Self {
        Self::new()
            .max_retries(10)
            .poll_interval(10)
            .rpc_timeout(300)
            .rpc_max_retries(5)
            .index_batch_size(100)
    }

    #[must_use]
    pub fn db_conn_string<S: Into<String>>(mut self, db_conn_string: S) -> Self {
        self.db_conn_string = Some(db_conn_string.into());
        self
    }

    #[must_use]
    pub fn node_conn_string<S: Into<String>>(mut self, node_conn_string: S) -> Self {
        self.node_conn_string = Some(node_conn_string.into());
        self
    }

    #[must_use]
    pub const fn should_index_txs(mut self, should_index_txs: bool) -> Self {
        self.should_index_txs = should_index_txs;
        self
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
    pub const fn rpc_max_retries(mut self, rpc_max_retries: u32) -> Self {
        self.rpc_max_retries = rpc_max_retries;
        self
    }

    #[must_use]
    pub const fn index_batch_size(mut self, index_batch_size: u32) -> Self {
        self.index_batch_size = index_batch_size;
        self
    }

    #[must_use]
    pub const fn start_block_offset(mut self, start_block_offset: u64) -> Self {
        self.start_block_offset = Some(start_block_offset);
        self
    }

    pub fn build(self) -> Result<IndexingConfig> {
        let db_conn_string = self.db_conn_string.ok_or_else(|| {
            BlockchainError::configuration(
                "db_conn_string",
                "Database connection string is required",
            )
        })?;

        let node_conn_string = self.node_conn_string.ok_or_else(|| {
            BlockchainError::configuration("node_conn_string", "Node connection string is required")
        })?;

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

        Ok(IndexingConfig {
            db_conn_string,
            node_conn_string,
            should_index_txs: self.should_index_txs,
            max_retries: self.max_retries,
            poll_interval: self.poll_interval,
            rpc_timeout: self.rpc_timeout,
            rpc_max_retries: self.rpc_max_retries,
            index_batch_size: self.index_batch_size,
            start_block_offset: self.start_block_offset,
        })
    }
}

impl Default for IndexingConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn start_indexing_services(
    indexing_config: IndexingConfig,
    should_terminate: Arc<AtomicBool>,
) -> Result<()> {
    let (db, rpc_client) = setup_database_and_rpc(&indexing_config).await?;

    initialize_index_metadata(db.clone(), rpc_client.clone(), &indexing_config).await?;

    let router_handle = spawn_router_service(should_terminate.clone());
    let quick_indexer_handle = spawn_quick_indexer_service(
        &indexing_config,
        db.clone(),
        rpc_client.clone(),
        should_terminate.clone(),
    )?;
    let batch_indexer_handle = spawn_batch_indexer_service(
        &indexing_config,
        db.clone(),
        rpc_client.clone(),
        should_terminate.clone(),
    )?;

    wait_for_thread_completion(vec![
        router_handle,
        quick_indexer_handle,
        batch_indexer_handle,
    ])
    .await?;

    Ok(())
}

async fn setup_database_and_rpc(
    indexing_config: &IndexingConfig,
) -> Result<(Arc<DbConnection>, Arc<EthereumJsonRpcClient>)> {
    info!("Connecting to DB");
    let db = DbConnection::new(indexing_config.db_conn_string.clone()).await?;

    let rpc_client = Arc::new(EthereumJsonRpcClient::new(
        indexing_config.node_conn_string.clone(),
        indexing_config.rpc_max_retries,
    ));

    info!("Run migrations");
    sqlx::migrate!().run(&db.pool).await.map_err(|e| {
        BlockchainError::database_connection(format!("Failed to run database migrations: {e}"))
    })?;

    info!("Starting Indexer");

    Ok((db, rpc_client))
}

fn spawn_router_service(should_terminate: Arc<AtomicBool>) -> tokio::task::JoinHandle<Result<()>> {
    tokio::spawn(async move {
        info!("[router] Starting router service");
        match router::initialize_router(should_terminate.clone()).await {
            Ok(()) => {
                info!("[router] Router service completed normally");
                Ok(())
            }
            Err(e) => {
                error!(
                    "[router] CRITICAL: Router service failed with error: {:?}",
                    e
                );
                error!("[router] This router failure may have caused the indexer to become unreachable");
                Err(BlockchainError::internal(format!(
                    "Router service failed: {e}"
                )))
            }
        }
    })
}

fn spawn_quick_indexer_service(
    indexing_config: &IndexingConfig,
    db: Arc<DbConnection>,
    rpc_client: Arc<EthereumJsonRpcClient>,
    should_terminate: Arc<AtomicBool>,
) -> Result<tokio::task::JoinHandle<Result<()>>> {
    let quick_config = QuickIndexConfig::builder()
        .should_index_txs(indexing_config.should_index_txs)
        .index_batch_size(indexing_config.index_batch_size)
        .max_retries(indexing_config.max_retries)
        .poll_interval(indexing_config.poll_interval)
        .rpc_timeout(indexing_config.rpc_timeout)
        .build()?;

    let quick_indexer = QuickIndexer::new(quick_config, db, rpc_client, should_terminate);

    Ok(tokio::spawn(async move {
        info!("[quick_index] Starting quick indexer service");
        match quick_indexer.index().await {
            Ok(()) => {
                info!("[quick_index] Quick indexer completed normally");
                Ok(())
            }
            Err(e) => {
                error!(
                    "[quick_index] CRITICAL: Quick indexer failed with error: {:?}",
                    e
                );
                error!("[quick_index] Quick indexer handles real-time block indexing - this failure stops new block processing");
                Err(BlockchainError::internal(format!(
                    "Quick indexer failed: {e}"
                )))
            }
        }
    }))
}

fn spawn_batch_indexer_service(
    indexing_config: &IndexingConfig,
    db: Arc<DbConnection>,
    rpc_client: Arc<EthereumJsonRpcClient>,
    should_terminate: Arc<AtomicBool>,
) -> Result<tokio::task::JoinHandle<Result<()>>> {
    let batch_config = BatchIndexConfig::builder()
        .should_index_txs(indexing_config.should_index_txs)
        .index_batch_size(indexing_config.index_batch_size)
        .max_retries(indexing_config.max_retries)
        .poll_interval(indexing_config.poll_interval)
        .rpc_timeout(indexing_config.rpc_timeout)
        .max_concurrent_requests(10)
        .task_timeout(300)
        .build()?;

    let batch_indexer = BatchIndexer::new(batch_config, db, rpc_client, should_terminate);

    Ok(tokio::spawn(async move {
        info!("[batch_index] Starting batch indexer service");
        match batch_indexer.index().await {
            Ok(()) => {
                info!("[batch_index] Batch indexer completed normally");
                Ok(())
            }
            Err(e) => {
                error!(
                    "[batch_index] CRITICAL: Batch indexer failed with error: {:?}",
                    e
                );
                error!("[batch_index] Batch indexer handles historical block indexing - this failure stops backfilling");
                Err(BlockchainError::internal(format!(
                    "Batch indexer failed: {e}"
                )))
            }
        }
    }))
}

async fn initialize_index_metadata(
    db: Arc<DbConnection>,
    rpc_client: Arc<EthereumJsonRpcClient>,
    indexing_config: &IndexingConfig,
) -> Result<IndexMetadataDto> {
    let latest_block_number = rpc_client.get_latest_finalized_blocknumber(None).await?;

    // Check if we have existing metadata
    if let Some(mut metadata) = get_index_metadata(db.clone()).await? {
        // For existing databases, we need to ensure backfill is properly configured
        // This handles cases where the database has gaps that need to be filled
        let needs_backfill_setup = should_setup_backfill_for_existing_db(
            db.clone(),
            &metadata,
            latest_block_number,
            indexing_config,
        )
        .await?;

        if needs_backfill_setup {
            info!(
                "[initialize] Existing database detected with potential gaps. Setting up backfill."
            );
            setup_backfill_for_existing_database(
                db.clone(),
                &mut metadata,
                latest_block_number,
                indexing_config,
            )
            .await?;
        }

        return Ok(metadata);
    }

    // For new databases, set up initial metadata

    let backfill_from_block = indexing_config.start_block_offset.map_or_else(
        || {
            // Default: start backfilling from block before latest (latest - 1)
            if latest_block_number.value() > 0 {
                latest_block_number - 1
            } else {
                BlockNumber::from_trusted(0)
            }
        },
        |offset| latest_block_number - i64::try_from(offset).unwrap_or(0),
    );

    // indexing_starting_block_number is the minimum block to backfill to (typically 0)
    let indexing_starting_block_number = 0;

    set_initial_indexing_status(
        db.clone(),
        latest_block_number.value(),
        indexing_starting_block_number,
        true,
    )
    .await?;

    // Set backfilling_block_number to start backfilling from the appropriate block
    if backfill_from_block.value() > indexing_starting_block_number {
        let mut tx = db.pool.begin().await.map_err(|e| {
            BlockchainError::database_connection(format!("Failed to begin transaction: {e}"))
        })?;

        sqlx::query("UPDATE index_metadata SET backfilling_block_number = $1")
            .bind(backfill_from_block.value())
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                BlockchainError::database_connection(format!(
                    "Failed to update backfilling_block_number: {e}"
                ))
            })?;

        tx.commit().await.map_err(|e| {
            BlockchainError::database_connection(format!("Failed to commit transaction: {e}"))
        })?;
    }

    if let Some(metadata) = get_index_metadata(db).await? {
        return Ok(metadata);
    }

    Err(BlockchainError::internal("Failed to get indexer metadata"))
}

#[allow(clippy::cognitive_complexity)]
async fn wait_for_thread_completion(handles: Vec<JoinHandle<Result<()>>>) -> Result<()> {
    let mut has_errors = false;

    for (index, handle) in handles.into_iter().enumerate() {
        let service_name = match index {
            0 => "router",
            1 => "quick_indexer",
            2 => "batch_indexer",
            _ => "unknown_service",
        };

        match handle.await {
            Ok(Ok(())) => {
                info!("[{}] Thread completed successfully", service_name);
            }
            Ok(Err(e)) => {
                error!(
                    "[{}] CRITICAL: Thread completed with an error: {:?}",
                    service_name, e
                );
                error!(
                    "[{}] Error details: {}",
                    service_name,
                    format_error_details(&e)
                );
                has_errors = true;
            }
            Err(e) => {
                error!("[{}] CRITICAL: Thread panicked: {:?}", service_name, e);
                if e.is_panic() {
                    error!("[{}] This was a panic - check for unwrap(), expect(), or other panic sources", service_name);
                }
                if e.is_cancelled() {
                    error!("[{}] Task was cancelled", service_name);
                }
                has_errors = true;
            }
        }
    }

    if has_errors {
        error!(
            "INDEXER SHUTDOWN: One or more services failed - this explains why the indexer stopped"
        );
        return Err(BlockchainError::internal(
            "One or more indexing services failed",
        ));
    }

    info!("All indexing services completed successfully");
    Ok(())
}

fn format_error_details(error: &BlockchainError) -> String {
    let mut details = Vec::new();
    details.push(format!("Error: {error}"));

    let mut current_error: &dyn std::error::Error = error;
    while let Some(source) = current_error.source() {
        details.push(format!("Caused by: {source}"));
        current_error = source;
    }

    details.join("\n  ")
}

/// Determines if backfill setup is needed for an existing database
///
/// This function performs gap analysis to determine if the existing database
/// needs backfill configuration to handle missing blocks.
async fn should_setup_backfill_for_existing_db(
    _db: Arc<DbConnection>,
    metadata: &IndexMetadataDto,
    latest_block_number: BlockNumber,
    _indexing_config: &IndexingConfig,
) -> Result<bool> {
    // If backfilling is already disabled and backfilling_block_number is None or 0,
    // it might indicate backfill was completed or never set up properly
    let backfill_block = metadata.backfilling_block_number.unwrap_or(0);

    if !metadata.is_backfilling && backfill_block <= metadata.indexing_starting_block_number {
        // Check if there are any gaps in a sample range to determine if we need backfill
        let sample_end = std::cmp::min(
            latest_block_number.value(),
            metadata.indexing_starting_block_number + 10000, // Sample first 10k blocks
        );

        if sample_end > metadata.indexing_starting_block_number {
            let has_gaps = find_first_gap(
                BlockNumber::from_trusted(metadata.indexing_starting_block_number),
                BlockNumber::from_trusted(sample_end),
            )
            .await?;

            if has_gaps.is_some() {
                info!(
                    "[initialize] Gap detected in sample range {} to {}, full backfill needed",
                    metadata.indexing_starting_block_number, sample_end
                );
                return Ok(true);
            }
        }
    }

    // Also check if the current_latest_block_number is way behind the actual latest
    // This could indicate the database is significantly out of sync
    let block_gap = latest_block_number.value() - metadata.current_latest_block_number;
    if block_gap > 1000 {
        info!(
            "[initialize] Database is {} blocks behind current latest, ensuring backfill is active",
            block_gap
        );
        return Ok(true);
    }

    Ok(false)
}

/// Sets up backfill configuration for an existing database with gaps
///
/// This function configures the metadata to enable proper backfilling of missing blocks
/// in databases that already exist but have gaps.
async fn setup_backfill_for_existing_database(
    db: Arc<DbConnection>,
    metadata: &mut IndexMetadataDto,
    latest_block_number: BlockNumber,
    indexing_config: &IndexingConfig,
) -> Result<()> {
    // Calculate appropriate backfill starting point
    let backfill_from_block = indexing_config.start_block_offset.map_or_else(
        || {
            // For existing databases, start from the latest block in the network
            // and work backwards. This ensures we don't miss recent blocks while
            // also setting up historical backfill.
            if latest_block_number.value() > 1000 {
                latest_block_number - 1000 // Start 1000 blocks back for safety
            } else {
                BlockNumber::from_trusted(0)
            }
        },
        |offset| latest_block_number - i64::try_from(offset).unwrap_or(0),
    );

    // Update the metadata to enable backfilling
    let mut tx = db.pool.begin().await.map_err(|e| {
        BlockchainError::database_connection(format!("Failed to begin transaction: {e}"))
    })?;

    // Set is_backfilling to true to enable the batch indexer
    sqlx::query("UPDATE index_metadata SET is_backfilling = $1, updated_at = CURRENT_TIMESTAMP")
        .bind(true)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            BlockchainError::database_connection(format!("Failed to enable backfilling: {e}"))
        })?;

    // Set the backfilling_block_number to start backfill from appropriate point
    update_backfilling_block_number_query(&mut tx, backfill_from_block.value()).await?;

    // Update current_latest_block_number if needed
    if latest_block_number.value() > metadata.current_latest_block_number {
        sqlx::query("UPDATE index_metadata SET current_latest_block_number = $1, updated_at = CURRENT_TIMESTAMP")
            .bind(latest_block_number.value())
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                BlockchainError::database_connection(format!("Failed to update latest block: {e}"))
            })?;
    }

    tx.commit().await.map_err(|e| {
        BlockchainError::database_connection(format!("Failed to commit backfill setup: {e}"))
    })?;

    // Update the metadata struct to reflect changes
    metadata.is_backfilling = true;
    metadata.backfilling_block_number = Some(backfill_from_block.value());
    if latest_block_number.value() > metadata.current_latest_block_number {
        metadata.current_latest_block_number = latest_block_number.value();
    }

    info!(
        "[initialize] Backfill configured: starting from block {}, target: block {}",
        backfill_from_block.value(),
        metadata.indexing_starting_block_number
    );

    Ok(())
}
