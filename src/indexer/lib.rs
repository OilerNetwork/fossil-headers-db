use std::{
    sync::{atomic::AtomicBool, Arc},
    // thread::{self, JoinHandle},
};

use crate::{
    db::DbConnection,
    errors::{BlockchainError, Result},
    indexer::{
        batch_service::{BatchIndexConfig, BatchIndexer},
        quick_service::{QuickIndexConfig, QuickIndexer},
    },
    repositories::index_metadata::{
        get_index_metadata, set_initial_indexing_status, IndexMetadataDto,
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
        if let Err(e) = router::initialize_router(should_terminate.clone()).await {
            error!("[router] unexpected error {}", e);
        }
        info!("[router] shutting down");
        Ok(())
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
        info!("Starting quick indexer");
        if let Err(e) = quick_indexer.index().await {
            error!("[quick_index] unexpected error {}", e);
        }
        Ok(())
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
        info!("Starting batch indexer");
        if let Err(e) = batch_indexer.index().await {
            error!("[batch_index] unexpected error {}", e);
        }
        Ok(())
    }))
}

async fn initialize_index_metadata(
    db: Arc<DbConnection>,
    rpc_client: Arc<EthereumJsonRpcClient>,
    indexing_config: &IndexingConfig,
) -> Result<IndexMetadataDto> {
    if let Some(metadata) = get_index_metadata(db.clone()).await? {
        return Ok(metadata);
    }

    let latest_block_number = rpc_client.get_latest_finalized_blocknumber(None).await?;

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

async fn wait_for_thread_completion(handles: Vec<JoinHandle<Result<()>>>) -> Result<()> {
    for handle in handles {
        match handle.await {
            Ok(Ok(())) => {
                info!("Thread completed successfully");
            }
            Ok(Err(e)) => {
                error!("Thread completed with an error: {:?}", e);
            }
            Err(e) => {
                error!("Thread panicked: {:?}", e);
            }
        }
    }

    Ok(())
}
