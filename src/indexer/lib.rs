use std::{
    sync::{atomic::AtomicBool, Arc},
    // thread::{self, JoinHandle},
};

use crate::{
    db::DbConnection,
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
use eyre::{eyre, Result};
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
    sqlx::migrate!().run(&db.pool).await?;

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
    let quick_indexer = QuickIndexer::new(
        QuickIndexConfig {
            should_index_txs: indexing_config.should_index_txs,
            index_batch_size: indexing_config.index_batch_size,
            max_retries: indexing_config.max_retries,
            poll_interval: indexing_config.poll_interval,
            rpc_timeout: indexing_config.rpc_timeout,
        },
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
    let batch_indexer = BatchIndexer::new(
        BatchIndexConfig {
            should_index_txs: indexing_config.should_index_txs,
            index_batch_size: indexing_config.index_batch_size,
            max_retries: indexing_config.max_retries,
            poll_interval: indexing_config.poll_interval,
            rpc_timeout: indexing_config.rpc_timeout,
            max_concurrent_requests: 10,
            task_timeout: 300,
        },
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

    Err(eyre!("Failed to get indexer metadata"))
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
