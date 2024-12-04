use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use eyre::{Context, Result};
use fossil_headers_db::{
    db::get_db_pool,
    indexer::{
        service_batch::{self, BatchIndexConfig},
        service_quick::{self, QuickIndexConfig},
    },
    rpc,
};
use tokio::try_join;
use tracing::{debug, info};
use tracing_subscriber::{fmt, EnvFilter};

use fossil_headers_db::models::{
    indexer_metadata::{IndexMetadata, IndexMetadataModel, IndexMetadataModelTrait},
    model::ModelError,
};

pub async fn get_indexer_metadata() -> Result<IndexMetadata> {
    let pool = get_db_pool().await?;
    let model = IndexMetadataModel::new(pool);

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

    Err(ModelError::UnexpectedError.into())
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

    let quick_index_config = QuickIndexConfig {
        starting_block: indexing_metadata.indexing_starting_block_number,
    };
    let batch_index_config = BatchIndexConfig {
        starting_block: indexing_metadata.indexing_starting_block_number,
    };

    let quick_index_terminator = Arc::clone(&should_terminate);
    let batch_index_terminator = Arc::clone(&should_terminate);

    // Start the quick indexer and the batch indexer
    let quick_indexer_handle = tokio::spawn(async move {
        info!("Starting quick service");
        service_quick::quick_index(quick_index_config, quick_index_terminator).await
    });
    let batch_indexer_handle = tokio::spawn(async move {
        info!("Starting batch service");
        service_batch::batch_index(batch_index_config, batch_index_terminator).await
    });

    let handle_result = try_join!(quick_indexer_handle, batch_indexer_handle)?;
    debug!(
        "Indexer stopping. Spawn complete result: {:?}",
        handle_result
    );

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
