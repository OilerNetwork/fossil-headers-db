use eyre::{Context, Result};
use fossil_headers_db::indexer::lib::{start_indexing_services, IndexingConfig};
use std::{
    env,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tracing::info;
use tracing_subscriber::fmt;

#[tokio::main]
pub async fn main() -> Result<()> {
    if env::var("IS_DEV").is_ok_and(|v| v.parse().unwrap_or(false)) {
        dotenvy::dotenv()?;
    }

    let db_conn_string =
        env::var("DB_CONNECTION_STRING").context("DB_CONNECTION_STRING must be set")?;
    let node_conn_string =
        env::var("NODE_CONNECTION_STRING").context("NODE_CONNECTION_STRING not set")?;

    let should_index_txs = env::var("INDEX_TRANSACTIONS")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .context("INDEX_TRANSACTIONS must be set")?;

    // Initialize tracing subscriber
    fmt().init();

    let should_terminate = Arc::new(AtomicBool::new(false));

    setup_ctrlc_handler(Arc::clone(&should_terminate))?;

    let indexing_config = IndexingConfig {
        db_conn_string,
        node_conn_string,
        should_index_txs,
        max_retries: 10,
        poll_interval: 10,
        rpc_timeout: 300,
        rpc_max_retries: 5,
        index_batch_size: 100, // larger size if we are indexing headers only
    };

    start_indexing_services(indexing_config, should_terminate).await?;

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
