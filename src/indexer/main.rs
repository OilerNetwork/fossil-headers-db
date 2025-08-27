use fossil_headers_db::errors::{BlockchainError, Result};
use fossil_headers_db::indexer::lib::{start_indexing_services, IndexingConfig};
use std::{
    env, panic,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tracing::{error, info};
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

    // Initialize tracing subscriber with human-readable format
    fmt()
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .compact()
        .init();

    // Setup panic hook to log panics before the application crashes
    setup_panic_hook();

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

    // Main indexing operation with comprehensive error logging
    match start_indexing_services(indexing_config, should_terminate).await {
        Ok(()) => {
            info!("Indexing services completed successfully");
            Ok(())
        }
        Err(e) => {
            error!("CRITICAL: Indexing services failed with error: {:?}", e);
            error!("Error chain: {}", format_error_chain(&e));
            error!("This is a fatal error that caused the indexer to stop");
            Err(e)
        }
    }
}

fn setup_ctrlc_handler(should_terminate: Arc<AtomicBool>) -> Result<()> {
    ctrlc::set_handler(move || {
        info!("Received Ctrl+C");
        info!("Waiting for current processes to finish...");
        should_terminate.store(true, Ordering::SeqCst);
    })
    .map_err(|e| BlockchainError::internal(format!("Failed to set Ctrl+C handler: {e}")))
}

#[allow(clippy::cognitive_complexity)]
fn setup_panic_hook() {
    panic::set_hook(Box::new(|panic_info| {
        let location = panic_info.location().map_or_else(
            || "unknown location".to_string(),
            |loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()),
        );

        let message = panic_info.payload().downcast_ref::<&str>().map_or_else(
            || {
                panic_info.payload().downcast_ref::<String>().map_or_else(
                    || "unknown panic message".to_string(),
                    std::clone::Clone::clone,
                )
            },
            |s| (*s).to_string(),
        );

        error!("PANIC OCCURRED - INDEXER CRASHING!");
        error!("Panic location: {}", location);
        error!("Panic message: {}", message);
        error!("This indicates a critical bug in the indexer");

        // Try to log the backtrace if available
        if let Ok(backtrace) = std::env::var("RUST_BACKTRACE") {
            if !backtrace.is_empty() && backtrace != "0" {
                error!(
                    "Backtrace logging is enabled (RUST_BACKTRACE={})",
                    backtrace
                );
            }
        } else {
            error!("Enable RUST_BACKTRACE=1 for stack traces");
        }

        // Flush logs before panic continues
        std::io::Write::flush(&mut std::io::stderr()).ok();
    }));
}

fn format_error_chain(error: &BlockchainError) -> String {
    let mut chain = Vec::new();
    let mut current_error: &dyn std::error::Error = error;

    chain.push(current_error.to_string());

    while let Some(source) = current_error.source() {
        chain.push(source.to_string());
        current_error = source;
    }

    chain.join(" -> ")
}
