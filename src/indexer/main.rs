use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
};

use eyre::{anyhow, Context, Result};
use fossil_headers_db::{
    db::DbConnection,
    indexer::{
        batch_service::{BatchIndexConfig, BatchIndexer},
        quick_service::{QuickIndexConfig, QuickIndexer},
    },
    repositories::index_metadata::{
        get_index_metadata, set_initial_indexing_status, IndexMetadata,
    },
    router, rpc,
};
use tracing::{error, info};
use tracing_subscriber::fmt;

pub async fn get_base_index_metadata(db: Arc<DbConnection>) -> Result<IndexMetadata> {
    if let Some(metadata) = get_index_metadata(db.clone()).await? {
        return Ok(metadata);
    }

    let latest_block_number = rpc::get_latest_finalized_blocknumber(None).await?;

    set_initial_indexing_status(db.clone(), latest_block_number, latest_block_number, true).await?;

    if let Some(metadata) = get_index_metadata(db).await? {
        return Ok(metadata);
    }

    Err(anyhow!("Failed to get indexer metadata"))
}

#[tokio::main]
pub async fn main() -> Result<()> {
    // TODO: this should be set to only be turned on if we're in dev mode
    dotenvy::dotenv()?;

    // Initialize tracing subscriber
    fmt().init();

    // Setup database connection
    info!("Connecting to DB");
    let db = DbConnection::new(None).await?;

    info!("Starting Indexer");

    let should_terminate = Arc::new(AtomicBool::new(false));

    setup_ctrlc_handler(Arc::clone(&should_terminate))?;

    // Start by checking and updating the current status in the db.
    // let indexing_metadata = get_base_index_metadata(db.clone()).await?;
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
    let quick_index_config = QuickIndexConfig::default();
    let quick_indexer =
        QuickIndexer::new(quick_index_config, db.clone(), should_terminate.clone()).await;

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
    let batch_index_config = BatchIndexConfig::default();
    let batch_indexer =
        BatchIndexer::new(batch_index_config, db.clone(), should_terminate.clone()).await;

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

fn setup_ctrlc_handler(should_terminate: Arc<AtomicBool>) -> Result<()> {
    ctrlc::set_handler(move || {
        info!("Received Ctrl+C");
        info!("Waiting for current processes to finish...");
        should_terminate.store(true, Ordering::SeqCst);
    })
    .context("Failed to set Ctrl+C handler")
}
