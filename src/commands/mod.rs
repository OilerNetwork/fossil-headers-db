//! # Legacy CLI Commands
//!
//! This module implements the legacy command-line interface operations for blockchain
//! data management. It provides two main operational modes: `update` and `fix` for
//! maintaining blockchain data integrity and completeness.
//!
//! ## Key Operations
//!
//! - **Update Mode** ([`update_from`]): Fetches new blocks from the blockchain and stores them
//! - **Fill Gaps Mode** ([`fill_gaps`]): Identifies and fills missing blocks in stored data
//!
//! ## Features
//!
//! - **Concurrent Processing**: Configurable concurrency for parallel block fetching
//! - **Resilient Operations**: Built-in retry logic with exponential backoff
//! - **Gap Detection**: Automatic identification of missing block ranges
//! - **Graceful Shutdown**: Responds to termination signals for clean shutdowns
//! - **Progress Monitoring**: Comprehensive logging of indexing progress
//!
//! ## Usage Examples
//!
//! ### Update Mode - Fetch Latest Blocks
//! ```rust,no_run
//! use fossil_headers_db::commands::update_from;
//! use fossil_headers_db::types::BlockNumber;
//! use std::sync::{Arc, atomic::AtomicBool};
//!
//! # async fn example() -> eyre::Result<()> {
//! let should_terminate = Arc::new(AtomicBool::new(false));
//!
//! // Fetch from block 19000000 to latest finalized, processing 100 blocks at a time
//! let start = Some(BlockNumber::from_trusted(19000000));
//! update_from(start, None, 100, should_terminate).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ### Fill Gaps Mode - Fix Missing Data
//! ```rust,no_run
//! use fossil_headers_db::commands::fill_gaps;
//! use fossil_headers_db::types::BlockNumber;
//! use std::sync::{Arc, atomic::AtomicBool};
//!
//! # async fn example() -> eyre::Result<()> {
//! let should_terminate = Arc::new(AtomicBool::new(false));
//!
//! // Fill gaps between blocks 1000000 and 2000000
//! let start = Some(BlockNumber::from_trusted(1000000));
//! let end = Some(BlockNumber::from_trusted(2000000));
//! fill_gaps(start, end, should_terminate).await?;
//! # Ok(())
//! # }
//! ```

use crate::errors::{BlockchainError, Result};
use crate::types::BlockNumber;
use futures_util::future::join_all;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::task;
use tracing::{error, info, warn};

use crate::db;
use crate::rpc;

pub const MAX_RETRIES: u64 = 10;

// Seconds
pub const POLL_INTERVAL: u64 = 60;
pub const TIMEOUT: u64 = 300;

// Concurrency limits
pub const MAX_CONCURRENT_TASKS: usize = 10;
pub const TASK_TIMEOUT: u64 = 300;

pub async fn fill_gaps(
    start: Option<BlockNumber>,
    end: Option<BlockNumber>,
    should_terminate: Arc<AtomicBool>,
) -> Result<()> {
    let range_start_pointer = start.unwrap_or_else(|| BlockNumber::from_trusted(0));
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
                range_start_pointer = BlockNumber::from_trusted(range_end_pointer.value() + 1);
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

    while !should_terminate.load(Ordering::Relaxed) && range_start_pointer <= search_end {
        let range_end_pointer = calculate_range_end(search_end, range_start_pointer);

        let null_data_vec = db::find_null_data(range_start_pointer, range_end_pointer).await?;

        range_start_pointer = process_null_blocks(null_data_vec, range_start_pointer).await?;
    }
    Ok(())
}

async fn process_null_blocks(
    null_data_vec: Vec<BlockNumber>,
    mut range_start_pointer: BlockNumber,
) -> Result<BlockNumber> {
    for null_data_block_number in null_data_vec {
        info!(
            "[fill_gaps] Found null values for block number: {}",
            null_data_block_number
        );

        retry_block_retrieval(null_data_block_number).await?;
        range_start_pointer = null_data_block_number + 1;
    }
    Ok(range_start_pointer)
}

async fn retry_block_retrieval(block_number: BlockNumber) -> Result<()> {
    for i in 0..MAX_RETRIES {
        match rpc::get_full_block_by_number(block_number, Some(TIMEOUT)).await {
            Ok(block) => {
                db::write_blockheader(block).await?;
                info!(
                    "[fill_gaps] Successfully wrote block {} after {i} retries",
                    block_number.value()
                );
                return Ok(());
            }
            Err(e) => {
                warn!(
                    "[fill_gaps] Error retrieving block {block_number} (attempt {}/{}): {e}",
                    i + 1,
                    MAX_RETRIES
                );

                if i == MAX_RETRIES - 1 {
                    return Err(BlockchainError::block_not_found(format!(
                        "Failed to retrieve block {} after {MAX_RETRIES} attempts during gap filling",
                        block_number.value()
                    )));
                }
            }
        }

        apply_exponential_backoff(i).await;
    }

    Err(BlockchainError::block_not_found(format!(
        "Failed to retrieve block {} after {MAX_RETRIES} attempts",
        block_number.value()
    )))
}

fn calculate_range_end(search_end: BlockNumber, range_start_pointer: BlockNumber) -> BlockNumber {
    BlockNumber::from_trusted(
        search_end
            .value()
            .min(range_start_pointer.value() + 100_000 - 1),
    )
}

async fn apply_exponential_backoff(attempt: u64) {
    let backoff: u64 = (attempt + 1).pow(2) * 5;
    tokio::time::sleep(Duration::from_secs(backoff)).await;
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
    last_error.map_or_else(
        || {
            Err(BlockchainError::block_not_found(format!(
                "Failed to retrieve block {} after {MAX_RETRIES} attempts (no error captured)",
                block_number.value()
            )))
        },
        |_e| {
            Err(BlockchainError::block_not_found(format!(
                "Failed to retrieve block {} after {MAX_RETRIES} attempts",
                block_number.value()
            )))
        },
    )
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
            }
            info!(
                "No new block finalized. Latest: {}. Sleeping for {}s...",
                new_latest_block.value(),
                POLL_INTERVAL
            );
            tokio::time::sleep(Duration::from_secs(POLL_INTERVAL)).await;
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
        // Create semaphore to limit concurrent tasks
        let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_TASKS));

        for n in (range_start.value()..=last_block.value().max(range_start.value()))
            .step_by(size as usize)
        {
            if should_terminate.load(Ordering::Relaxed) {
                info!("Termination requested. Stopping update process.");
                break;
            }

            let range_end = (last_block.value() + 1).min(n + i64::from(size));

            let (successful_count, failed_blocks) =
                process_block_range(n, range_end, &semaphore).await?;

            let should_continue =
                apply_backpressure_if_needed(&failed_blocks, successful_count, n).await?;

            if !should_continue {
                break;
            }

            let should_continue =
                handle_batch_completion(&failed_blocks, successful_count, n, range_end);

            if !should_continue {
                break;
            }
        }
    }

    Ok(())
}

async fn process_block(block_number: BlockNumber) -> Result<()> {
    for i in 0..MAX_RETRIES {
        if let Some(result) = attempt_block_processing(block_number, i).await? {
            return result;
        }

        apply_exponential_backoff(i).await;
    }

    error!("[update_from] Error with block number {}", block_number);
    Err(BlockchainError::internal(format!(
        "Failed to process block {block_number}"
    )))
}

async fn attempt_block_processing(
    block_number: BlockNumber,
    attempt: u64,
) -> Result<Option<Result<()>>> {
    let block = match rpc::get_full_block_by_number(block_number, Some(TIMEOUT)).await {
        Ok(block) => block,
        Err(e) => {
            warn!(
                "[update_from] Error retrieving block {}: {}",
                block_number, e
            );
            return Ok(None);
        }
    };

    match db::write_blockheader(block).await {
        Ok(()) => {
            log_success_if_retry(block_number, attempt);
            Ok(Some(Ok(())))
        }
        Err(e) => {
            warn!("[update_from] Error writing block {block_number}: {e}");
            Ok(None)
        }
    }
}

fn log_success_if_retry(block_number: BlockNumber, attempt: u64) {
    if attempt > 0 {
        info!(
            "[update_from] Successfully wrote block {} after {attempt} retries",
            block_number.value()
        );
    }
}

async fn get_first_missing_block(start: Option<BlockNumber>) -> Result<BlockNumber> {
    Ok(match start {
        Some(s) => s,
        None => BlockNumber::from_trusted(db::get_last_stored_blocknumber().await?.value() + 1),
    })
}

async fn get_last_block(end: Option<BlockNumber>) -> Result<BlockNumber> {
    let latest_block: BlockNumber = rpc::get_latest_finalized_blocknumber(Some(TIMEOUT)).await?;

    Ok(end.map_or(latest_block, |s| {
        if s <= latest_block {
            s
        } else {
            latest_block
        }
    }))
}

async fn process_block_range(
    start_block: i64,
    end_block: i64,
    semaphore: &Arc<Semaphore>,
) -> Result<(usize, Vec<i64>)> {
    let tasks: Vec<_> = (start_block..end_block)
        .map(|block_number| {
            let permit = semaphore.clone();
            task::spawn(async move {
                let _permit = permit
                    .acquire()
                    .await
                    .map_err(|_| BlockchainError::internal("Semaphore closed".to_string()))?;

                tokio::time::timeout(
                    Duration::from_secs(TASK_TIMEOUT),
                    process_block(BlockNumber::from_trusted(block_number)),
                )
                .await
                .map_err(|_| {
                    BlockchainError::internal(format!(
                        "Task timeout processing block {block_number}"
                    ))
                })?
            })
        })
        .collect();

    let all_res = join_all(tasks).await;

    let mut failed_blocks = Vec::new();
    let mut successful_count = 0;

    for (idx, join_res) in all_res.iter().enumerate() {
        let block_num = start_block + i64::try_from(idx).unwrap_or(0);
        match join_res {
            Ok(Ok(())) => {
                successful_count += 1;
            }
            Ok(Err(e)) => {
                warn!("Block {} failed with error: {}", block_num, e);
                failed_blocks.push(block_num);
            }
            Err(e) => {
                warn!("Task for block {} panicked: {}", block_num, e);
                failed_blocks.push(block_num);
            }
        }
    }

    Ok((successful_count, failed_blocks))
}

async fn apply_backpressure_if_needed(
    failed_blocks: &[i64],
    successful_count: usize,
    start_block: i64,
) -> Result<bool> {
    let total_blocks = failed_blocks.len() + successful_count;
    if total_blocks == 0 {
        return Ok(true);
    }

    #[allow(clippy::cast_precision_loss, clippy::float_arithmetic)]
    let failure_ratio = failed_blocks.len() as f64 / total_blocks as f64;

    if failure_ratio > 0.3 {
        warn!(
            "High failure rate ({:.1}%), applying backpressure. Failed blocks: {:?}",
            failure_ratio * 100.0,
            failed_blocks
        );
        tokio::time::sleep(Duration::from_secs(5)).await;

        if failure_ratio > 0.7 {
            error!(
                "Failure rate too high ({:.1}%), stopping batch. Rerun from block: {}",
                failure_ratio * 100.0,
                start_block
            );
            return Ok(false);
        }
    }

    Ok(true)
}

fn handle_batch_completion(
    failed_blocks: &[i64],
    successful_count: usize,
    start_block: i64,
    end_block: i64,
) -> bool {
    if !failed_blocks.is_empty() {
        error!(
            "Some blocks failed. Failed: {:?}. Rerun from block: {}",
            failed_blocks, start_block
        );
        return false;
    }

    info!(
        "Successfully written {} blocks ({} - {}). Next block: {}",
        successful_count,
        start_block,
        end_block - 1,
        end_block
    );

    true
}

#[cfg(test)]
mod tests;
