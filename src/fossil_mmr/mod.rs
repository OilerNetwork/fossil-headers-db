use accumulators::{
    hasher::keccak::KeccakHasher,
    mmr::{element_index_to_leaf_index, elements_count_to_leaf_count, map_leaf_index_to_element_index, AppendResult, Proof, MMR},
    store::sqlite::SQLiteStore,
};
use anyhow::Result;
use chrono::{TimeZone, Utc};
use lazy_static::lazy_static;
use log::{debug, error, info, warn};
use once_cell::sync::Lazy;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::{Mutex, MutexGuard, OnceCell};

use crate::{
    db,
    types::{BlockDetails, Update},
};

const MAX_RETRIES: u64 = 10;
const DB_FILE_PATH: &str = "mmr_db";
const MMR_ID: &str = "blockheaders_mmr";
const MMR_APPEND_LOOPSIZE: i32 = 10_000; // How many (upper limit) block hashes are retrieved at for each query (limit for performance)
const MMR_APPEND_CHUNKSIZE: usize = 50;
static HASHES_MMR: OnceCell<Arc<Mutex<MMR>>> = OnceCell::const_new();
static IS_UPDATING: Lazy<Arc<AtomicBool>> = Lazy::new(|| Arc::new(AtomicBool::new(false)));

lazy_static! {
    static ref LATEST_UPDATE: Arc<Mutex<Update>> = Arc::new(Mutex::new(Update {
        latest_blocknumber: 0,
        latest_roothash: "Unset".to_string(),
        update_timestamp: Utc.with_ymd_and_hms(1700, 1, 1, 0, 0, 0).unwrap()
    }));
}

async fn get_mmr() -> Result<Arc<Mutex<MMR>>> {
    match HASHES_MMR.get() {
        Some(mmr) => Ok(mmr.clone()),
        None => {
            let store = SQLiteStore::new(DB_FILE_PATH, Some(true), Some(MMR_ID)).await?;
            let store_rc = Arc::new(store);
            let hasher = Arc::new(KeccakHasher::new());
            let mmr = MMR::new(store_rc.clone(), hasher.clone(), Some(MMR_ID.to_string()));

            let arc_mutex_mmr = Arc::new(Mutex::new(mmr));

            match HASHES_MMR.set(arc_mutex_mmr.clone()) {
                Ok(_) => Ok(arc_mutex_mmr),
                Err(_) => Ok(HASHES_MMR.get().expect("MMR was just set").clone()),
            }
        }
    }
}


pub async fn update_mmr(should_terminate: &AtomicBool) -> Result<()> {
    for _ in 0..MAX_RETRIES {
        if IS_UPDATING.load(Ordering::Relaxed) {
            error!("Currently updating MMR");
            return Ok(());
        }
        IS_UPDATING.store(true, Ordering::SeqCst);

        let update_result = perform_mmr_update(should_terminate).await;

        IS_UPDATING.store(false, Ordering::SeqCst);

        match update_result {
            Ok(_) => return Ok(()),
            Err(e) => warn!("[update_mmr] Error with updating MMR: {}", e),
        }
    }
    error!("[update_mmr] Max retries reached. Failed to update MMR.");
    Ok(())
}

async fn perform_mmr_update(should_terminate: &AtomicBool) -> Result<()> {
    let last_added_blocknumber = get_last_added_blocknumber().await?;
    info!("Last added block number: {}", last_added_blocknumber);

    let range_end = db::get_last_stored_blocknumber().await?;

    for start_block in (last_added_blocknumber..=range_end).step_by(MMR_APPEND_LOOPSIZE as usize) {
        if should_terminate.load(Ordering::Relaxed) {
            info!("Termination requested. Stopping MMR update process.");
            return Ok(());
        }

        update_mmr_chunk(start_block, should_terminate).await?;
    }

    Ok(())
}

async fn get_last_added_blocknumber() -> Result<i64> {
    // Retrieves the blocknumber for the next blockhash
    let mmr = get_mmr().await?;
    let element_count = {
        let mmr_guard = mmr.lock().await;
        mmr_guard.elements_count.get().await?
    };
    element_count_to_blocknumber(element_count)
}


async fn update_mmr_chunk(start_block: i64, should_terminate: &AtomicBool) -> Result<()> {
    for _ in 0..MAX_RETRIES {
        if should_terminate.load(Ordering::Relaxed) {
            info!("Termination requested. Stopping MMR update process.");
            return Ok(());
        }

        match db::get_blockheaders(start_block, MMR_APPEND_LOOPSIZE).await {
            Ok(hashes) => {
                info!(
                    "Successfully retrieved {} blockheaders. Adding hashes to MMR...",
                    hashes.len()
                );
                match append_to_mmr(hashes, should_terminate).await {
                    Ok(_) => return Ok(()),
                    Err(e) => warn!(
                        "[update_mmr_chunk] Error appending to MMR, blockheaders from block {}: {}",
                        start_block, e
                    ),
                }
            }
            Err(e) => {
                warn!(
                    "[update_mmr_chunk] Error getting blockheaders from block {}: {}",
                    start_block, e
                )
            }
        }
    }
    Err(anyhow::anyhow!(
        "Failed to update MMR chunk starting at block {}",
        start_block
    ))
}

async fn append_to_mmr(
    block_details: Vec<BlockDetails>,
    should_terminate: &AtomicBool,
) -> Result<()> {
    let mmr = get_mmr().await?;
    // verify next in seq
    let first_block = block_details.first();
    let mut prev_blocknumber = match first_block {
        None => return Ok(()),
        Some(first_block_details) => {
            info!("Verifing block: {}", first_block_details.number);
            verify_first_new_block_sequence(&mmr, first_block_details).await?;
            first_block_details.number
        }
    };
    info!("First block verified");

    for block_detail_chunk in block_details[1..].chunks(MMR_APPEND_CHUNKSIZE) {
        let mut mmr_guard = mmr.lock().await;
        for block_detail in block_detail_chunk {
            
            if should_terminate.load(Ordering::Relaxed) {
                info!("Termination requested. Stopping MMR update process.");
                let element_count = mmr_guard.elements_count.get().await?;
                let last_blocknumber_added: i64 = element_count_to_blocknumber(element_count)?;

                info!("Last block added to MMR: {}", last_blocknumber_added);
                return Ok(());
            }

            // Checks the next hash to be added is what is expected (ensures hashes are added in order and without gaps)
            assert_eq!(
                block_detail.number,
                prev_blocknumber + 1,
                "Blockdetails not added in order. Expected blocknumber: {}, received blocknumber: {}",
                prev_blocknumber + 1,
                block_detail.number
            );

            let append_result: AppendResult = mmr_guard
                .append(block_detail.block_hash.to_string())
                .await?;

            update_mmr_stats(block_detail.number, append_result.root_hash).await?;

            debug!("Block {} added", prev_blocknumber);
            prev_blocknumber = block_detail.number;
        }
    }

    match block_details.last() {
        Some(detail) => {
            info!("Last block added to MMR: {}", detail.number);
        }
        None => {
            warn!("Can't retrieve last block added. Error unwrapping.")
        }
    }
    Ok(())
}

/**
 * Verifies that the first hash to be added is the next one that is missing from the MMR
 */
async fn verify_first_new_block_sequence(
    mmr: &Arc<Mutex<MMR>>,
    first_block_details: &BlockDetails,
) -> Result<()> {
    let mut mmr_guard = mmr.lock().await;

    let mut draft = mmr_guard.start_draft().await?;
    let append_result = draft
        .mmr
        .append(first_block_details.block_hash.to_string())
        .await?;

    let expected_number: i64 =
        element_index_to_leaf_index(append_result.element_index)?.try_into()?;

    assert_eq!(
        first_block_details.number, expected_number,
        "Blockdetail not added in order. Expected blocknumber: {}, received blocknumber: {}",
        expected_number, first_block_details.number
    );

    draft.commit().await?;
    update_mmr_stats(first_block_details.number, append_result.root_hash).await?;

    Ok(())
}

pub async fn get_proof(blocknumber: i64) -> Result<Proof> {
    let leaf_index: usize = (blocknumber + 1).try_into()?;
    let element_index: usize = map_leaf_index_to_element_index(leaf_index);
    let mmr: Arc<Mutex<MMR>> = get_mmr().await?;
    let mmr_guard: MutexGuard<MMR> = mmr.lock().await;
    let res: Proof = mmr_guard.get_proof(element_index, None).await?;

    Ok(res)
}

async fn update_mmr_stats(latest_blocknumber: i64, latest_roothash: String) -> Result<()> {
    let mut update_guard = LATEST_UPDATE.lock().await;
    *update_guard = Update {
        latest_blocknumber,
        latest_roothash,
        update_timestamp: Utc::now(),
    };
    Ok(())
}

pub async fn get_mmr_stats() -> Result<Update> {
    let update_guard = LATEST_UPDATE.lock().await;
    Ok(update_guard.clone())
}

fn element_count_to_blocknumber(element_count: usize) -> Result<i64> {
    let leaf_count: i64 = elements_count_to_leaf_count(element_count)?.try_into()?;
    Ok(leaf_count - 1)
}
