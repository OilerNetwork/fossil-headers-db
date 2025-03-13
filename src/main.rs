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

    // Get config values from env vars
    // TODO: Determine if a config file is better for this
    let db_conn_string =
        env::var("DB_CONNECTION_STRING").context("DB_CONNECTION_STRING must be set")?;
    let node_conn_string =
        env::var("NODE_CONNECTION_STRING").context("NODE_CONNECTION_STRING not set")?;

    let index_batch_size = env::var("INDEX_BATCH_SIZE")
        .unwrap_or_else(|_| "100".to_string())
        .parse::<u32>()
        .context("INDEX_BATCH_SIZE must be a positive integer")?;

    let should_index_txs = env::var("INDEX_TRANSACTIONS")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .context("INDEX_TRANSACTIONS must be set to true or false")?;

    let max_retries = env::var("MAX_RETRIES")
        .unwrap_or_else(|_| "10".to_string())
        .parse::<u8>()
        .context("MAX_RETRIES must be a positive integer")?;

    let poll_interval = env::var("POLL_INTERVAL")
        .unwrap_or_else(|_| "10".to_string())
        .parse::<u32>()
        .context("POLL_INTERVAL must be a positive integer")?;

    let rpc_timeout = env::var("RPC_TIMEOUT")
        .unwrap_or_else(|_| "300".to_string())
        .parse::<u32>()
        .context("RPC_TIMEOUT must be a positive integer")?;

    let rpc_max_retries = env::var("RPC_MAX_RETRIES")
        .unwrap_or_else(|_| "5".to_string())
        .parse::<u32>()
        .context("RPC_MAX_RETRIES must be a positive integer")?;

    // Initialize tracing subscriber
    fmt().init();

    let should_terminate = Arc::new(AtomicBool::new(false));

    setup_ctrlc_handler(Arc::clone(&should_terminate))?;

    let indexing_config = IndexingConfig {
        db_conn_string,
        node_conn_string,
        index_batch_size, // larger size if we are indexing headers only
        should_index_txs,
        max_retries,
        poll_interval,
        rpc_timeout,
        rpc_max_retries,
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
