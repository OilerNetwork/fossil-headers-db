// Legacy CLI binary - uses library interface only
use clap::{Parser, ValueEnum};
use core::cmp::min;
use fossil_headers_db::{
    commands, database::DB_MAX_CONNECTIONS, health::initialize_router, BlockNumber,
    BlockchainError, Result,
};
use futures::future::join;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{info, warn};
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// What mode to run the program in
    #[arg(value_enum)]
    mode: Mode,

    /// Start block number
    #[arg(short, long)]
    start: Option<i64>,

    /// End block number
    #[arg(short, long)]
    end: Option<i64>,

    /// Number of threads (Max = 1000)
    #[arg(short, long, default_value_t = DB_MAX_CONNECTIONS)]
    loopsize: u32,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum Mode {
    Fix,
    Update,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    // Initialize tracing subscriber
    fmt().with_env_filter(EnvFilter::from_default_env()).init();

    info!("Starting Indexer");

    let cli = Cli::parse();
    let should_terminate = Arc::new(AtomicBool::new(false));
    let terminate_clone = should_terminate.clone();

    setup_ctrlc_handler(Arc::clone(&should_terminate))?;

    let router = async {
        let res = initialize_router(should_terminate.clone()).await;
        match res {
            Ok(()) => info!("Router task completed"),
            Err(e) => warn!("Router task failed: {:?}", e),
        }
    };

    let updater = async {
        let res = match cli.mode {
            Mode::Fix => {
                let start = cli.start.map(BlockNumber::from_trusted);
                let end = cli.end.map(BlockNumber::from_trusted);
                commands::fill_gaps(start, end, Arc::clone(&terminate_clone)).await
            }
            Mode::Update => {
                let start = cli.start.map(BlockNumber::from_trusted);
                let end = cli.end.map(BlockNumber::from_trusted);
                commands::update_from(
                    start,
                    end,
                    min(cli.loopsize, DB_MAX_CONNECTIONS),
                    Arc::clone(&terminate_clone),
                )
                .await
            }
        };

        match res {
            Ok(()) => info!("Updater task completed"),
            Err(e) => warn!("Updater task failed: {:?}", e),
        }
    };

    let _ = join(router, updater).await;

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
