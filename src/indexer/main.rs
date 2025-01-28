use eyre::{Context, Result};
use fossil_headers_db::indexer::lib::start_indexing_services;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tracing::info;

#[tokio::main]
pub async fn main() -> Result<()> {
    // TODO: this should be set to only be turned on if we're in dev mode
    dotenvy::dotenv()?;

    let should_terminate = Arc::new(AtomicBool::new(false));

    setup_ctrlc_handler(Arc::clone(&should_terminate))?;

    start_indexing_services(should_terminate).await?;

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
