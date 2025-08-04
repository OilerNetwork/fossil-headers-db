use fossil_headers_db::errors::{BlockchainError, Result};
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
        dotenvy::dotenv().map_err(|e| {
            BlockchainError::configuration("dotenv", format!("Failed to load .env file: {e}"))
        })?;
    }

    let db_conn_string = env::var("DB_CONNECTION_STRING").map_err(|_| {
        BlockchainError::configuration("DB_CONNECTION_STRING", "Environment variable must be set")
    })?;
    let node_conn_string = env::var("NODE_CONNECTION_STRING").map_err(|_| {
        BlockchainError::configuration("NODE_CONNECTION_STRING", "Environment variable not set")
    })?;

    let should_index_txs = env::var("INDEX_TRANSACTIONS")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .map_err(|e| {
            BlockchainError::configuration(
                "INDEX_TRANSACTIONS",
                format!("Invalid boolean value: {e}"),
            )
        })?;

    let start_block_offset = env::var("START_BLOCK_OFFSET")
        .ok()
        .and_then(|s| s.parse::<u64>().ok());

    // Initialize tracing subscriber
    fmt().init();

    let should_terminate = Arc::new(AtomicBool::new(false));

    setup_ctrlc_handler(Arc::clone(&should_terminate))?;

    let mut indexing_config_builder = IndexingConfig::builder()
        .db_conn_string(db_conn_string)
        .node_conn_string(node_conn_string)
        .should_index_txs(should_index_txs);

    if let Some(offset) = start_block_offset {
        indexing_config_builder = indexing_config_builder.start_block_offset(offset);
    }

    let indexing_config = indexing_config_builder.build()?;

    start_indexing_services(indexing_config, should_terminate).await?;

    Ok(())
}

fn setup_ctrlc_handler(should_terminate: Arc<AtomicBool>) -> Result<()> {
    ctrlc::set_handler(move || {
        info!("Received Ctrl+C");
        info!("Waiting for current processes to finish...");
        should_terminate.store(true, Ordering::SeqCst);
    })
    .map_err(|e| BlockchainError::internal(format!("Failed to set Ctrl+C handler: {e}")))
}
