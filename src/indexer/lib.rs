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
}

impl IndexingConfig {
    pub fn builder() -> IndexingConfigBuilder {
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
}

impl IndexingConfigBuilder {
    pub fn new() -> Self {
        Self {
            db_conn_string: None,
            node_conn_string: None,
            should_index_txs: false,
            max_retries: 10,
            poll_interval: 10,
            rpc_timeout: 300,
            rpc_max_retries: 5,
            index_batch_size: 100,
        }
    }

    pub fn development() -> Self {
        Self::new()
            .max_retries(3)
            .poll_interval(5)
            .rpc_timeout(60)
            .rpc_max_retries(3)
            .index_batch_size(10)
    }

    pub fn production() -> Self {
        Self::new()
            .max_retries(20)
            .poll_interval(30)
            .rpc_timeout(600)
            .rpc_max_retries(10)
            .index_batch_size(500)
    }

    pub fn testing() -> Self {
        Self::new()
            .max_retries(1)
            .poll_interval(1)
            .rpc_timeout(30)
            .rpc_max_retries(1)
            .index_batch_size(5)
    }

    pub fn db_conn_string<S: Into<String>>(mut self, db_conn_string: S) -> Self {
        self.db_conn_string = Some(db_conn_string.into());
        self
    }

    pub fn node_conn_string<S: Into<String>>(mut self, node_conn_string: S) -> Self {
        self.node_conn_string = Some(node_conn_string.into());
        self
    }

    pub fn should_index_txs(mut self, should_index_txs: bool) -> Self {
        self.should_index_txs = should_index_txs;
        self
    }

    pub fn max_retries(mut self, max_retries: u8) -> Self {
        self.max_retries = max_retries;
        self
    }

    pub fn poll_interval(mut self, poll_interval: u32) -> Self {
        self.poll_interval = poll_interval;
        self
    }

    pub fn rpc_timeout(mut self, rpc_timeout: u32) -> Self {
        self.rpc_timeout = rpc_timeout;
        self
    }

    pub fn rpc_max_retries(mut self, rpc_max_retries: u32) -> Self {
        self.rpc_max_retries = rpc_max_retries;
        self
    }

    pub fn index_batch_size(mut self, index_batch_size: u32) -> Self {
        self.index_batch_size = index_batch_size;
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
    // Setup database connection
    info!("Connecting to DB");
    let db = DbConnection::new(indexing_config.db_conn_string).await?;

    let rpc_client = Arc::new(EthereumJsonRpcClient::new(
        indexing_config.node_conn_string,
        indexing_config.rpc_max_retries,
    ));

    info!("Run migrations");
    sqlx::migrate!().run(&db.pool).await.map_err(|e| {
        BlockchainError::database_connection(format!("Failed to run database migrations: {e}"))
    })?;

    info!("Starting Indexer");
    // Start by checking and updating the current status in the db.
    initialize_index_metadata(db.clone(), rpc_client.clone()).await?;
    let router_terminator = Arc::clone(&should_terminate);

    // Setup the router which allows us to query health status and operations
    let router_handle = tokio::spawn(async move {
        if let Err(e) = router::initialize_router(router_terminator.clone()).await {
            error!("[router] unexpected error {}", e);
        }

        info!("[router] shutting down");

        Ok(())
    });

    // Start the quick indexer
    let quick_config = QuickIndexConfig::builder()
        .should_index_txs(indexing_config.should_index_txs)
        .index_batch_size(indexing_config.index_batch_size)
        .max_retries(indexing_config.max_retries)
        .poll_interval(indexing_config.poll_interval)
        .rpc_timeout(indexing_config.rpc_timeout)
        .build()?;

    let quick_indexer = QuickIndexer::new(
        quick_config,
        db.clone(),
        rpc_client.clone(),
        should_terminate.clone(),
    )
    .await;

    let quick_indexer_handle = tokio::spawn(async move {
        info!("Starting quick indexer");
        if let Err(e) = quick_indexer.index().await {
            error!("[quick_index] unexpected error {}", e);
        }
        Ok(())
    });

    // Start the batch indexer
    let batch_config = BatchIndexConfig::builder()
        .should_index_txs(indexing_config.should_index_txs)
        .index_batch_size(indexing_config.index_batch_size)
        .max_retries(indexing_config.max_retries)
        .poll_interval(indexing_config.poll_interval)
        .rpc_timeout(indexing_config.rpc_timeout)
        .max_concurrent_requests(10)
        .task_timeout(300)
        .build()?;

    let batch_indexer = BatchIndexer::new(
        batch_config,
        db.clone(),
        rpc_client.clone(),
        should_terminate.clone(),
    )
    .await;

    let batch_indexer_handle = tokio::spawn(async move {
        info!("Starting batch indexer");
        if let Err(e) = batch_indexer.index().await {
            error!("[batch_index] unexpected error {}", e);
        }
        Ok(())
    });
    // Wait for termination, which will join all the handles.
    wait_for_thread_completion(vec![
        router_handle,
        quick_indexer_handle,
        batch_indexer_handle,
    ])
    .await?;

    Ok(())
}

async fn initialize_index_metadata(
    db: Arc<DbConnection>,
    rpc_client: Arc<EthereumJsonRpcClient>,
) -> Result<IndexMetadataDto> {
    if let Some(metadata) = get_index_metadata(db.clone()).await? {
        return Ok(metadata);
    }

    // Set current latest block number to the latest block number - 1 to make sure we don't miss the new blocks
    let latest_block_number = rpc_client.get_latest_finalized_blocknumber(None).await? - 1;

    set_initial_indexing_status(
        db.clone(),
        latest_block_number.value(),
        latest_block_number.value(),
        true,
    )
    .await?;

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
