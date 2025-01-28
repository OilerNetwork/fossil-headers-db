use std::{
    env,
    sync::{atomic::AtomicBool, Arc},
    thread::{self, JoinHandle},
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
    rpc::{self, EthereumJsonRpcClient},
};
use eyre::{anyhow, Context, Result};
use tracing::{error, info};
use tracing_subscriber::fmt;

pub async fn start_indexing_services(should_terminate: Arc<AtomicBool>) -> Result<()> {
    let db_conn_string =
        env::var("DB_CONNECTION_STRING").context("DB_CONNECTION_STRING must be set")?;
    let connection_string =
        env::var("NODE_CONNECTION_STRING").context("NODE_CONNECTION_STRING not set")?;

    let should_index_txs = env::var("INDEX_TRANSACTIONS")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .context("INDEX_TRANSACTIONS must be set")?;

    // Initialize tracing subscriber
    fmt().init();

    // Setup database connection
    info!("Connecting to DB");
    let db = DbConnection::new(db_conn_string).await?;

    let rpc_client = Arc::new(EthereumJsonRpcClient::new(connection_string, 5));

    info!("Starting Indexer");
    // Start by checking and updating the current status in the db.
    initialize_index_metadata(db.clone()).await?;
    let router_terminator = Arc::clone(&should_terminate);

    // Setup the router which allows us to query health status and operations
    let router_handle = thread::Builder::new()
        .name("[router]".to_owned())
        .spawn(move || {
            let rt = tokio::runtime::Runtime::new()?;

            info!("Starting router");
            if let Err(e) = rt.block_on(router::initialize_router(router_terminator.clone())) {
                error!("[router] unexpected error {}", e);
            }

            info!("[router] shutting down");
            Ok(())
        })?;

    // Start the quick indexer
    let quick_indexer = QuickIndexer::new(
        QuickIndexConfig {
            should_index_txs,
            index_batch_size: 100, // larger size since we are not indexing txs
            ..Default::default()
        },
        db.clone(),
        rpc_client.clone(),
        should_terminate.clone(),
    )
    .await;

    let quick_indexer_handle = thread::Builder::new()
        .name("[quick_index]".to_owned())
        .spawn(move || {
            let rt = tokio::runtime::Runtime::new()?;

            info!("Starting quick indexer");
            if let Err(e) = rt.block_on(quick_indexer.index()) {
                error!("[quick_index] unexpected error {}", e);
            }
            Ok(())
        })?;

    // Start the batch indexer
    let batch_indexer = BatchIndexer::new(
        BatchIndexConfig {
            should_index_txs,
            index_batch_size: 100, // larger size since we are not indexing txs
            ..Default::default()
        },
        db.clone(),
        rpc_client.clone(),
        should_terminate.clone(),
    )
    .await;

    let batch_indexer_handle = thread::Builder::new()
        .name("[batch_index]".to_owned())
        .spawn(move || {
            let rt = tokio::runtime::Runtime::new()?;

            info!("Starting batch indexer");
            if let Err(e) = rt.block_on(batch_indexer.index()) {
                error!("[batch_index] unexpected error {}", e);
            }
            Ok(())
        })?;

    // Wait for termination, which will join all the handles.
    wait_for_thread_completion(vec![
        router_handle,
        quick_indexer_handle,
        batch_indexer_handle,
    ])?;

    Ok(())
}

async fn initialize_index_metadata(db: Arc<DbConnection>) -> Result<IndexMetadataDto> {
    if let Some(metadata) = get_index_metadata(db.clone()).await? {
        return Ok(metadata);
    }

    // Set current latest block number to the latest block number - 1 to make sure we don't miss the new blocks
    let latest_block_number = rpc::get_latest_finalized_blocknumber(None).await? - 1;

    set_initial_indexing_status(db.clone(), latest_block_number, latest_block_number, true).await?;

    if let Some(metadata) = get_index_metadata(db).await? {
        return Ok(metadata);
    }

    Err(anyhow!("Failed to get indexer metadata"))
}

fn wait_for_thread_completion(handles: Vec<JoinHandle<Result<()>>>) -> Result<()> {
    for handle in handles {
        match handle.join() {
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
