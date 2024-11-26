use eyre::{Context, Result};
use futures_util::future::join_all;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{self, Duration};
use tokio::task;
use tracing::{error, info, warn};

use crate::db;
use crate::rpc;

const MAX_RETRIES: u64 = 10;

// Seconds
const POLL_INTERVAL: u64 = 60;
const TIMEOUT: u64 = 300;

pub async fn fill_gaps(
    start: Option<i64>,
    end: Option<i64>,
    should_terminate: Arc<AtomicBool>,
) -> Result<()> {
    let range_start_pointer = start.unwrap_or(0).max(0);
    let range_end = get_range_end(end).await?;

    if range_end < 0 || range_start_pointer == range_end {
        info!("Empty database");
        return Ok(());
    }

    fill_missing_blocks_in_range(range_start_pointer, range_end, &should_terminate).await?;
    fill_null_rows(range_start_pointer, range_end, &should_terminate).await
}

async fn fill_missing_blocks_in_range(
    mut range_start_pointer: i64,
    search_end: i64,
    should_terminate: &AtomicBool,
) -> Result<()> {
    let mut range_end_pointer: i64;
    for _ in 0..MAX_RETRIES {
        while !should_terminate.load(Ordering::Relaxed) && range_start_pointer <= search_end {
            range_end_pointer = search_end.min(range_start_pointer + 100_000 - 1);
            // Find gaps in block number
            match db::find_first_gap(range_start_pointer, range_end_pointer).await? {
                Some(block_number) => {
                    info!("[fill_gaps] Found missing block number: {}", block_number);
                    if process_missing_block(block_number, &mut range_start_pointer).await? {
                        range_start_pointer = block_number + 1;
                    }
                }
                None => {
                    info!(
                        "[fill_gaps] No missing values found from {} to {}",
                        range_start_pointer, range_end_pointer
                    );
                    range_start_pointer = range_end_pointer + 1
                }
            }
        }
    }
    Ok(())
}

async fn fill_null_rows(
    search_start: i64,
    search_end: i64,
    should_terminate: &AtomicBool,
) -> Result<()> {
    let mut range_start_pointer: i64 = search_start;
    let mut range_end_pointer: i64;

    while !should_terminate.load(Ordering::Relaxed) && range_start_pointer <= search_end {
        println!("or this?");

        range_end_pointer = search_end.min(range_start_pointer + 100_000 - 1);

        // Find null data in the database
        let null_data_vec = db::find_null_data(range_start_pointer, range_end_pointer).await?;
        for null_data_block_number in null_data_vec {
            info!(
                "[fill_gaps] Found null values for block number: {}",
                null_data_block_number
            );

            // Logic from process_missing_block
            for i in 0..MAX_RETRIES {
                match rpc::get_full_block_by_number(null_data_block_number, Some(TIMEOUT)).await {
                    Ok(block) => {
                        db::write_blockheader(block).await?;
                        info!("[fill_gaps] Successfully wrote block {null_data_block_number} after {i} retries");
                        range_start_pointer = null_data_block_number + 1;
                    }
                    Err(e) => {
                        warn!("[fill_gaps] Error retrieving block {null_data_block_number}: {e}")
                    }
                }
                let backoff: u64 = (i).pow(2) * 5;
                tokio::time::sleep(Duration::from_secs(backoff)).await;
            }
        }
    }
    Ok(())
}

async fn process_missing_block(block_number: i64, range_start_pointer: &mut i64) -> Result<bool> {
    for i in 0..MAX_RETRIES {
        match rpc::get_full_block_by_number(block_number, Some(TIMEOUT)).await {
            Ok(block) => {
                db::write_blockheader(block).await?;
                *range_start_pointer = block_number + 1;
                info!("[fill_gaps] Successfully wrote block {block_number} after {i} retries");
                return Ok(true);
            }
            Err(e) => warn!("[fill_gaps] Error retrieving block {block_number}: {e}"),
        }
        let backoff: u64 = (i).pow(2) * 5;
        tokio::time::sleep(Duration::from_secs(backoff)).await;
    }
    error!("[fill_gaps] Error with block number {}", block_number);
    Ok(false)
}

async fn get_range_end(end: Option<i64>) -> Result<i64> {
    Ok(match end {
        Some(s) => s,
        None => db::get_last_stored_blocknumber()
            .await
            .context("[fill_gaps] Error retrieving last_recorded_block")?,
    })
}

pub async fn update_from(
    start: Option<i64>,
    end: Option<i64>,
    size: u32,
    should_terminate: Arc<AtomicBool>,
) -> Result<()> {
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

        update_blocks(range_start, last_block, size, should_terminate).await?;

        loop {
            if should_terminate.load(Ordering::Relaxed) {
                break;
            }

            let new_latest_block = rpc::get_latest_finalized_blocknumber(Some(TIMEOUT)).await?;
            if new_latest_block > last_block {
                range_start = last_block + 1;
                last_block = new_latest_block;
                break;
            } else {
                info!(
                    "No new block finalized. Latest: {}. Sleeping for {}s...",
                    new_latest_block, POLL_INTERVAL
                );
                async_std::task::sleep(time::Duration::from_secs(POLL_INTERVAL)).await;
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
        for n in (range_start..=last_block.max(range_start)).step_by(size as usize) {
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
        match rpc::get_full_block_by_number(block_number, Some(TIMEOUT)).await {
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
        let backoff: u64 = (i).pow(2) * 5;
        tokio::time::sleep(Duration::from_secs(backoff)).await;
    }
    error!("[update_from] Error with block number {}", block_number);
    Err(eyre::anyhow!("Failed to process block {}", block_number))
}

async fn get_first_missing_block(start: Option<i64>) -> Result<i64> {
    Ok(match start {
        Some(s) => s,
        None => {
            db::get_last_stored_blocknumber()
                .await
                .context("[update_from] Error retrieving first_recorded_block")?
                + 1
        }
    })
}

async fn get_last_block(end: Option<i64>) -> Result<i64> {
    let latest_block: i64 = rpc::get_latest_finalized_blocknumber(Some(TIMEOUT))
        .await
        .context("Failed to get latest block number")?;

    Ok(match end {
        Some(s) => s.min(latest_block),
        None => latest_block,
    })
}
