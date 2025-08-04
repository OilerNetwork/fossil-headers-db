use crate::errors::{BlockchainError, Result};
use crate::types::BlockNumber;
use futures_util::future::join_all;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::task;
use tracing::{error, info, warn};

use crate::db;
use crate::rpc;

const MAX_RETRIES: u64 = 10;

// Seconds
const POLL_INTERVAL: u64 = 60;
const TIMEOUT: u64 = 300;

pub async fn fill_gaps(
    start: Option<BlockNumber>,
    end: Option<BlockNumber>,
    should_terminate: Arc<AtomicBool>,
) -> Result<()> {
    let range_start_pointer = start.unwrap_or(BlockNumber::from_trusted(0));
    let range_end = get_range_end(end).await?;

    if range_end.value() < 0 || range_start_pointer == range_end {
        info!("Empty database");
        return Ok(());
    }

    fill_missing_blocks_in_range(range_start_pointer, range_end, &should_terminate).await?;
    fill_null_rows(range_start_pointer, range_end, &should_terminate).await
}

async fn fill_missing_blocks_in_range(
    mut range_start_pointer: BlockNumber,
    search_end: BlockNumber,
    should_terminate: &AtomicBool,
) -> Result<()> {
    let mut range_end_pointer: BlockNumber;
    for _ in 0..MAX_RETRIES {
        while !should_terminate.load(Ordering::Relaxed) && range_start_pointer <= search_end {
            range_end_pointer = BlockNumber::from_trusted(
                search_end
                    .value()
                    .min(range_start_pointer.value() + 100_000 - 1),
            );
            // Find gaps in block number
            if let Some(block_number) =
                db::find_first_gap(range_start_pointer, range_end_pointer).await?
            {
                info!(
                    "[fill_gaps] Found missing block number: {}",
                    block_number.value()
                );
                if process_missing_block(block_number, &mut range_start_pointer).await? {
                    range_start_pointer = BlockNumber::from_trusted(block_number.value() + 1);
                }
            } else {
                info!(
                    "[fill_gaps] No missing values found from {} to {}",
                    range_start_pointer, range_end_pointer
                );
                range_start_pointer = BlockNumber::from_trusted(range_end_pointer.value() + 1)
            }
        }
    }
    Ok(())
}

async fn fill_null_rows(
    search_start: BlockNumber,
    search_end: BlockNumber,
    should_terminate: &AtomicBool,
) -> Result<()> {
    let mut range_start_pointer: BlockNumber = search_start;
    let mut range_end_pointer: BlockNumber;

    while !should_terminate.load(Ordering::Relaxed) && range_start_pointer <= search_end {
        range_end_pointer = BlockNumber::from_trusted(
            search_end
                .value()
                .min(range_start_pointer.value() + 100_000 - 1),
        );

        // Find null data in the database
        let null_data_vec = db::find_null_data(range_start_pointer, range_end_pointer).await?;
        for null_data_block_number in null_data_vec {
            info!(
                "[fill_gaps] Found null values for block number: {}",
                null_data_block_number
            );

            // Logic from process_missing_block
            let mut block_retrieved = false;
            for i in 0..MAX_RETRIES {
                match rpc::get_full_block_by_number(null_data_block_number, Some(TIMEOUT)).await {
                    Ok(block) => {
                        db::write_blockheader(block).await?;
                        info!(
                            "[fill_gaps] Successfully wrote block {} after {i} retries",
                            null_data_block_number.value()
                        );
                        range_start_pointer = null_data_block_number + 1;
                        block_retrieved = true;
                        break;
                    }
                    Err(e) => {
                        warn!("[fill_gaps] Error retrieving block {null_data_block_number} (attempt {}/{}): {e}", i + 1, MAX_RETRIES);
                        if i == MAX_RETRIES - 1 {
                            // Last attempt failed - this is a critical error for gap filling
                            return Err(BlockchainError::block_not_found(format!("Failed to retrieve block {} after {MAX_RETRIES} attempts during gap filling", null_data_block_number.value())));
                        }
                    }
                }
                let backoff: u64 = (i + 1).pow(2) * 5;
                tokio::time::sleep(Duration::from_secs(backoff)).await;
            }

            if !block_retrieved {
                return Err(BlockchainError::block_not_found(format!(
                    "Failed to retrieve block {} after {MAX_RETRIES} attempts",
                    null_data_block_number.value()
                )));
            }
        }
    }
    Ok(())
}

async fn process_missing_block(
    block_number: BlockNumber,
    range_start_pointer: &mut BlockNumber,
) -> Result<bool> {
    let mut last_error = None;
    for i in 0..MAX_RETRIES {
        match rpc::get_full_block_by_number(block_number, Some(TIMEOUT)).await {
            Ok(block) => {
                db::write_blockheader(block).await?;
                *range_start_pointer = BlockNumber::from_trusted(block_number.value() + 1);
                info!(
                    "[fill_gaps] Successfully wrote block {} after {i} retries",
                    block_number.value()
                );
                return Ok(true);
            }
            Err(e) => {
                warn!(
                    "[fill_gaps] Error retrieving block {} (attempt {}/{}): {e}",
                    block_number.value(),
                    i + 1,
                    MAX_RETRIES
                );
                last_error = Some(e);
            }
        }
        let backoff: u64 = (i + 1).pow(2) * 5;
        tokio::time::sleep(Duration::from_secs(backoff)).await;
    }

    // All retries failed - return the error instead of silently continuing
    if let Some(_e) = last_error {
        Err(BlockchainError::block_not_found(format!(
            "Failed to retrieve block {} after {MAX_RETRIES} attempts",
            block_number.value()
        )))
    } else {
        Err(BlockchainError::block_not_found(format!(
            "Failed to retrieve block {} after {MAX_RETRIES} attempts (no error captured)",
            block_number.value()
        )))
    }
}

async fn get_range_end(end: Option<BlockNumber>) -> Result<BlockNumber> {
    Ok(match end {
        Some(s) => s,
        None => db::get_last_stored_blocknumber().await?,
    })
}

pub async fn update_from(
    start: Option<BlockNumber>,
    end: Option<BlockNumber>,
    size: u32,
    should_terminate: Arc<AtomicBool>,
) -> Result<()> {
    let range_start = get_first_missing_block(start).await?;
    info!("Range start: {}", range_start.value());

    let last_block = get_last_block(end).await?;
    info!("Range end: {}", last_block.value());

    match end {
        Some(_) => update_blocks(range_start, last_block, size, &should_terminate).await,
        None => chain_update_blocks(range_start, last_block, size, &should_terminate).await,
    }
}

async fn chain_update_blocks(
    mut range_start: BlockNumber,
    mut last_block: BlockNumber,
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
                    new_latest_block.value(),
                    POLL_INTERVAL
                );
                tokio::time::sleep(Duration::from_secs(POLL_INTERVAL)).await;
            }
        }
    }

    Ok(())
}

async fn update_blocks(
    range_start: BlockNumber,
    last_block: BlockNumber,
    size: u32,
    should_terminate: &AtomicBool,
) -> Result<()> {
    if range_start <= last_block {
        for n in (range_start.value()..=last_block.value().max(range_start.value()))
            .step_by(size as usize)
        {
            if should_terminate.load(Ordering::Relaxed) {
                info!("Termination requested. Stopping update process.");
                break;
            }

            let range_end = (last_block.value() + 1).min(n + size as i64);

            let tasks: Vec<_> = (n..range_end)
                .map(|block_number| {
                    task::spawn(process_block(BlockNumber::from_trusted(block_number)))
                })
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

async fn process_block(block_number: BlockNumber) -> Result<()> {
    for i in 0..MAX_RETRIES {
        match rpc::get_full_block_by_number(block_number, Some(TIMEOUT)).await {
            Ok(block) => match db::write_blockheader(block).await {
                Ok(_) => {
                    if i > 0 {
                        info!(
                            "[update_from] Successfully wrote block {} after {i} retries",
                            block_number.value()
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
    Err(BlockchainError::internal(format!(
        "Failed to process block {block_number}"
    )))
}

async fn get_first_missing_block(start: Option<BlockNumber>) -> Result<BlockNumber> {
    Ok(match start {
        Some(s) => s,
        None => BlockNumber::from_trusted(db::get_last_stored_blocknumber().await?.value() + 1),
    })
}

async fn get_last_block(end: Option<BlockNumber>) -> Result<BlockNumber> {
    let latest_block: BlockNumber = rpc::get_latest_finalized_blocknumber(Some(TIMEOUT)).await?;

    Ok(match end {
        Some(s) => {
            if s <= latest_block {
                s
            } else {
                latest_block
            }
        }
        None => latest_block,
    })
}
