use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
};

use eyre::{Context, Result};
use fossil_headers_db::{
    db::get_db_pool,
    indexer::{
        service_batch::{self, BatchIndexConfig},
        service_quick::{self, QuickIndexConfig},
    },
    router, rpc,
};
use tracing::{error, info};
use tracing_subscriber::{fmt, EnvFilter};

use fossil_headers_db::repositories::{
    indexer_metadata::{IndexMetadata, IndexMetadataRepository, IndexMetadataRepositoryTrait},
    repository::RepositoryError,
};

pub async fn get_indexer_metadata() -> Result<IndexMetadata> {
    let pool = get_db_pool().await?;
    let model = IndexMetadataRepository::new(pool);

    if let Some(metadata) = model.get_index_metadata().await? {
        return Ok(metadata);
    }

    let latest_block_number = rpc::get_latest_finalized_blocknumber(None).await?;

    model
        .set_initial_indexing_status(latest_block_number, latest_block_number, true)
        .await?;

    if let Some(metadata) = model.get_index_metadata().await? {
        return Ok(metadata);
    }

    Err(RepositoryError::UnexpectedError.into())
}

#[tokio::main]
pub async fn main() -> Result<()> {
    // Initialize tracing subscriber
    fmt().with_env_filter(EnvFilter::from_default_env()).init();

    info!("Starting Indexer");

    let should_terminate = Arc::new(AtomicBool::new(false));

    setup_ctrlc_handler(Arc::clone(&should_terminate))?;

    // Start by checking and updating the current status in the db.
    let indexing_metadata = get_indexer_metadata().await?;
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
    let quick_index_config = QuickIndexConfig {
        starting_block: indexing_metadata.indexing_starting_block_number,
    };
    let quick_index_terminator = Arc::clone(&should_terminate);

    let quick_indexer_handle = thread::Builder::new()
        .name("[quick_indexer]".to_owned())
        .spawn(move || {
            let rt = tokio::runtime::Runtime::new()?;

            info!("Starting quick indexer");
            if let Err(e) = rt.block_on(service_quick::quick_index(
                quick_index_config,
                quick_index_terminator,
            )) {
                error!("[quick_indexer] unexpected error {}", e);
            }
            Ok(())
        })?;

    // Start the batch indexer
    let batch_index_config = BatchIndexConfig {
        starting_block: indexing_metadata.indexing_starting_block_number,
    };
    let batch_index_terminator = Arc::clone(&should_terminate);

    let batch_indexer_handle = thread::Builder::new()
        .name("[batch_indexer]".to_owned())
        .spawn(move || {
            let rt = tokio::runtime::Runtime::new()?;

            info!("Starting batch indexer");
            if let Err(e) = rt.block_on(service_batch::batch_index(
                batch_index_config,
                batch_index_terminator,
            )) {
                error!("[batch_indexer] unexpected error {}", e);
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
        handle.join().unwrap()?;
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
