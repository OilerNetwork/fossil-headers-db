mod commands;
mod db;
mod endpoints;
mod fossil_mmr;
mod router;
mod types;

use clap::{Parser, ValueEnum};
use core::cmp::min;
use derive_more::From;
use log::info;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, From)]
pub enum Error {
    Generic(String),

    #[from]
    CommandError(commands::Error),
    #[from]
    CtrlCError(ctrlc::Error),
}

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
    #[arg(short, long, default_value_t = db::DB_MAX_CONNECTIONS)]
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
    env_logger::init();

    let cli = Cli::parse();
    let should_terminate = Arc::new(AtomicBool::new(false));
    let terminate_clone = should_terminate.clone();

    setup_ctrlc_handler(Arc::clone(&should_terminate))?;

    let res = router::initialize_router(should_terminate.clone()).await;

    let router = tokio::spawn(async move {
        let res = router::initialize_router(should_terminate.clone()).await;
        match res {
            Ok(_) => (),
            Err(e) => loop {
                info!("Router error: {e}");
                sleep(Duration::from_secs(60)).await;
            },
        }
    });

    let updater = tokio::spawn(async move {
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
                Ok(_) => (),
                Err(e) => loop {
                    info!("Updater error: {e}");
                    sleep(Duration::from_secs(60)).await;
                },
            }
        });
    

    Ok(())
}

fn setup_ctrlc_handler(should_terminate: Arc<AtomicBool>) -> Result<()> {
    match ctrlc::set_handler(move || {
        info!("Received Ctrl+C");
        info!("Waiting for current blocks to finish...");
        should_terminate.store(true, Ordering::SeqCst);
    }) {
        Ok(_) => Ok(()),
        Err(e) => Err(Error::Generic(format!("Failed to set Ctrl-C Handler: {e}"))),
    }
}
