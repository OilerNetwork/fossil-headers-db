use derive_more::{Display, From};
use futures_util::future::join_all;
use log::{error, info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::{thread, time};
use tokio::task;

use crate::{db, endpoints, fossil_mmr, types::type_utils};

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, Display, From)]
pub enum Error {
    Generic(String),

    #[from]
    SqlxError(sqlx::Error),
    #[from]
    DBError(db::Error),
    #[from]
    EndpointError(endpoints::Error),
    #[from]
    MMRError(fossil_mmr::Error),
}

const MAX_RETRIES: u32 = 10;

// Seconds
const SLEEP_INTERVAL: u64 = 60;
const TIMEOUT: u64 = 300;

pub async fn fill_gaps(
    start: Option<i64>,
    end: Option<i64>,
    should_terminate: Arc<AtomicBool>,
) -> Result<()> {
    db::create_tables().await?;

    let range_start_pointer = start.unwrap_or(0).max(0);
    let range_end = get_range_end(end).await?;

    if range_end < 0 || range_start_pointer == range_end {
        info!("Empty database");
        return Ok(());
    }

    fill_missing_blocks(range_start_pointer, range_end, &should_terminate).await
}

async fn fill_missing_blocks(
    mut range_start_pointer: i64,
    search_end: i64,
    should_terminate: &AtomicBool,
) -> Result<()> {
    let mut range_end_pointer: i64 = search_end;
    while !should_terminate.load(Ordering::Relaxed) && range_start_pointer <= range_end_pointer {
        range_end_pointer = search_end.min(range_start_pointer + 100_000);
        match db::find_first_gap(range_start_pointer, range_end_pointer).await? {
            Some(block_number) => {
                info!("[fill_gaps] Found missing block number: {}", block_number);
                if process_missing_block(block_number, &mut range_start_pointer).await? {
                    continue;
                }
            }
            None => {
                info!(
                    "[fill_gaps] No missing values found from {} to {}",
                    range_start_pointer, range_end_pointer
                );
            }
        }
    }
    Ok(())
}

async fn process_missing_block(block_number: i64, range_start_pointer: &mut i64) -> Result<bool> {
    for i in 0..MAX_RETRIES {
        match endpoints::get_full_block_by_number(block_number, Some(TIMEOUT)).await {
            Ok(block) => {
                db::write_blockheader(block).await?;
                *range_start_pointer = block_number + 1;
                info!("[fill_gaps] Successfully wrote block {block_number} after {i} retries");
                return Ok(true);
            }
            Err(e) => warn!("[fill_gaps] Error retrieving block {block_number}: {e}"),
        }
    }
    error!("[fill_gaps] Error with block number {}", block_number);
    Ok(false)
}

async fn get_range_end(end: Option<i64>) -> Result<i64> {
    Ok(match end {
        Some(s) => s,
        None => db::get_last_stored_blocknumber().await?,
    })
}

pub async fn update_from(
    start: Option<i64>,
    end: Option<i64>,
    size: u32,
    should_terminate: Arc<AtomicBool>,
) -> Result<()> {
    db::create_tables().await?;

    let range_start = get_first_missing_block(start).await?;
    info!("Range start: {}", range_start);

    let last_block = get_last_block(end).await?;
    info!("Range end: {}", last_block);

    match end {
        Some(_) => update_blocks(range_start, last_block, size, &should_terminate).await,
        None => chain_update_blocks(range_start, last_block, size, &should_terminate).await,
    }
}

async fn chain_update_blocks(
    mut range_start: i64,
    mut last_block: i64,
    size: u32,
    should_terminate: &AtomicBool,
) -> Result<()> {
    loop {
        if should_terminate.load(Ordering::Relaxed) {
            info!("Termination requested. Stopping update process.");
            break;
        }

        update_blocks(range_start, range_start + 1, size, should_terminate).await?;
        fossil_mmr::update_mmr(should_terminate).await?;

        if should_terminate.load(Ordering::Relaxed) {
            break;
        }

        range_start = last_block + 1;

        loop {
            if should_terminate.load(Ordering::Relaxed) {
                break;
            }

            let new_latest_block = range_start + 1;
            if new_latest_block > last_block {
                last_block = new_latest_block;
                info!(
                    "New latest_block: {}, last block inserted: {}.",
                    new_latest_block, last_block
                );
                break;
            } else {
                info!(
                    "No new block finalized. Latest: {}. Sleeping for {}s...",
                    new_latest_block, SLEEP_INTERVAL
                );
                thread::sleep(time::Duration::from_secs(SLEEP_INTERVAL));
            }
        }
    }

    Ok(())
}

async fn update_blocks(
    range_start: i64,
    last_block: i64,
    size: u32,
    should_terminate: &AtomicBool,
) -> Result<()> {
    if range_start <= last_block {
        for n in (range_start..=(last_block - size as i64).max(range_start)).step_by(size as usize)
        {
            if should_terminate.load(Ordering::Relaxed) {
                info!("Termination requested. Stopping update process.");
                break;
            }

            let range_end = (last_block + 1).min(n + size as i64);

            let tasks: Vec<_> = (n..range_end)
                .map(|block_number| task::spawn(process_block(block_number)))
                .collect();

            let all_res = join_all(tasks).await;
            let has_err = all_res.iter().any(|join_res| {
                join_res.is_err() || join_res.as_ref().is_ok_and(|res| res.is_err())
            });

            if has_err {
                error!("Rerun from block: {}", n);
                break;
            }
            info!(
                "Written blocks {} - {}. Next block: {}",
                n,
                range_end - 1,
                range_end
            );
        }
    }

    Ok(())
}

async fn process_block(block_number: i64) -> Result<()> {
    for i in 0..MAX_RETRIES {
        match endpoints::get_full_block_by_number(block_number, Some(TIMEOUT)).await {
            Ok(block) => match db::write_blockheader(block).await {
                Ok(_) => {
                    if i > 0 {
                        info!(
                            "[update_from] Successfully wrote block {block_number} after {i} retries"
                        );
                    }
                    return Ok(());
                }
                Err(e) => warn!("[update_from] Error writing block {block_number}: {e}"),
            },
            Err(e) => warn!(
                "[update_from] Error retrieving block {}: {}",
                block_number, e
            ),
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
    error!("[update_from] Error with block number {}", block_number);
    Err(Error::Generic(format!(
        "Failed to process block {block_number}"
    )))
}

async fn get_first_missing_block(start: Option<i64>) -> Result<i64> {
    let last_inserted_block = db::get_last_stored_blocknumber().await? + 1;

    Ok(match start {
        Some(s) => s,
        None => last_inserted_block,
    })
}

async fn get_last_block(end: Option<i64>) -> Result<i64> {
    let latest_block_hex = endpoints::get_latest_blocknumber(Some(TIMEOUT)).await?;
    let latest_block = type_utils::convert_hex_string_to_i64(&latest_block_hex);

    Ok(match end {
        Some(s) => s.min(latest_block),
        None => latest_block,
    })
}
