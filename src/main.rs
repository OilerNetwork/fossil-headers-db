#![deny(unused_crate_dependencies)]
use fossil_headers_db as _;

mod commands;
mod db;
mod router;
mod rpc;
mod types;

use clap::{Parser, ValueEnum};
use core::cmp::min;
use eyre::{Context, Result};
use futures::future::join;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{info, warn};
use tracing_subscriber::{fmt, EnvFilter};

/// Command line interface for the Fossil Headers DB application.
/// Provides options for running the application in different modes
/// and configuring its behavior.
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// What mode to run the program in:
    /// - Fix: Repairs gaps in the blockchain data
    /// - Update: Updates the database with new blocks
    #[arg(value_enum)]
    mode: Mode,

    /// Start block number for processing.
    /// If not provided, will use appropriate defaults based on mode.
    #[arg(short, long)]
    start: Option<i64>,

    /// End block number for processing.
    /// If not provided, will process up to the latest block.
    #[arg(short, long)]
    end: Option<i64>,

    /// Number of concurrent processing threads (Max = 1000).
    /// Controls the parallelism of block processing.
    #[arg(short, long, default_value_t = db::DB_MAX_CONNECTIONS)]
    loopsize: u32,
}

/// Operating modes for the application.
#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum Mode {
    /// Fix mode: Identifies and repairs gaps in the blockchain data
    Fix,
    /// Update mode: Fetches and stores new blocks from the chain
    Update,
}

/// Main entry point for the Fossil Headers DB application.
/// 
/// This function:
/// 1. Initializes environment variables and logging
/// 2. Parses command line arguments
/// 3. Sets up graceful shutdown handling
/// 4. Starts the router for handling API requests
/// 5. Starts the updater task based on the specified mode
/// 
/// # Returns
/// * `Result<()>`: Ok if the application runs and terminates successfully,
///                 Err if there's an unrecoverable error
#[tokio::main]
async fn main() -> Result<()> {
    // TODO: Load environment variables if its dev mode
    dotenvy::dotenv().ok();

    // Initialize tracing subscriber
    fmt().with_env_filter(EnvFilter::from_default_env()).init();

    info!("Starting Indexer");

    let cli = Cli::parse();
    let should_terminate = Arc::new(AtomicBool::new(false));
    let terminate_clone = should_terminate.clone();

    setup_ctrlc_handler(Arc::clone(&should_terminate))?;

    let router = async {
        let res = router::initialize_router(should_terminate.clone()).await;
        match res {
            Ok(()) => info!("Router task completed"),
            Err(e) => warn!("Router task failed: {:?}", e),
        };
    };

    let updater = async {
        let res = match cli.mode {
            Mode::Fix => {
                commands::fill_gaps(cli.start, cli.end, Arc::clone(&terminate_clone)).await
            }
            Mode::Update => {
                commands::update_from(
                    cli.start,
                    cli.end,
                    min(cli.loopsize, db::DB_MAX_CONNECTIONS),
                    Arc::clone(&terminate_clone),
                )
                .await
            }
        };

        match res {
            Ok(()) => info!("Updater task completed"),
            Err(e) => warn!("Updater task failed: {:?}", e),
        };
    };

    let _ = join(router, updater).await;

    Ok(())
}

/// Sets up a handler for Ctrl+C signals to enable graceful shutdown.
/// 
/// When Ctrl+C is received, this handler:
/// 1. Logs the signal reception
/// 2. Notifies that the system is waiting for current processes
/// 3. Sets the termination flag to true
/// 
/// # Arguments
/// * `should_terminate` - Atomic boolean flag shared across threads to signal termination
/// 
/// # Returns
/// * `Result<()>` - Ok if handler was set up successfully, Err otherwise
fn setup_ctrlc_handler(should_terminate: Arc<AtomicBool>) -> Result<()> {
    ctrlc::set_handler(move || {
        info!("Received Ctrl+C");
        info!("Waiting for current processes to finish...");
        should_terminate.store(true, Ordering::SeqCst);
    })
    .context("Failed to set Ctrl+C handler")
}
