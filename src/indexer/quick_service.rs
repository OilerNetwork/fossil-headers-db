use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use eyre::{Context, Result};
use tracing::{error, info, warn};

use crate::{
    db::db::DbConnection,
    repositories::{block_header::write_blockheader, indexer_metadata::get_index_metadata},
    rpc,
};

#[derive(Debug)]
pub struct QuickIndexConfig {
    pub starting_block: i64,
    pub max_retries: u8,
    pub poll_interval: u32,
    pub rpc_timeout: u32,
}

impl Default for QuickIndexConfig {
    fn default() -> Self {
        Self {
            starting_block: 0,
            max_retries: 10,
            poll_interval: 10,
            rpc_timeout: 300,
        }
    }
}

pub struct QuickIndexer {
    config: QuickIndexConfig,
    db: Arc<DbConnection>,
    should_terminate: Arc<AtomicBool>,
}

impl QuickIndexer {
    pub async fn new(
        config: QuickIndexConfig,
        db: Arc<DbConnection>,
        should_terminate: Arc<AtomicBool>,
    ) -> QuickIndexer {
        Self {
            db,
            config,
            should_terminate,
        }
    }

    pub async fn index(&self) -> Result<()> {
        // Quick indexer loop, does the following until terminated:
        // 1. check current latest block
        // 2. check if the block is already indexed
        // 3. if not, index the block
        // 4. if yes, sleep for a period of time and do nothing
        while !self.should_terminate.load(Ordering::Relaxed) {
            let last_block_number = match get_index_metadata(self.db.clone()).await {
                Ok(metadata) => match metadata {
                    Some(metadata) => metadata.current_latest_block_number,
                    None => {
                        error!("[quick_index] Error getting index metadata");
                        return Err(eyre::anyhow!(
                            "Error getting index metadata: metadata not found."
                        ));
                    }
                },
                Err(e) => {
                    error!("[quick_index] Error getting index metadata: {}", e);
                    return Err(e.into());
                }
            };

            let new_latest_block =
                rpc::get_latest_finalized_blocknumber(Some(self.config.rpc_timeout.into())).await?;

            if new_latest_block > last_block_number {
                // Do something here.
                break;
            } else {
                info!(
                    "No new block finalized. Latest: {}. Sleeping for {}s...",
                    new_latest_block, self.config.poll_interval
                );
                tokio::time::sleep(Duration::from_secs(self.config.poll_interval.into())).await;
            }
        }

        info!("[quick_index] Process terminating.");
        Ok(())
    }

    async fn process_block(&self, config: QuickIndexConfig, block_number: i64) -> Result<()> {
        for i in 0..config.max_retries {
            match rpc::get_full_block_by_number(block_number, Some(config.rpc_timeout.into())).await
            {
                Ok(block) => match write_blockheader(self.db.clone(), block).await {
                    Ok(_) => {
                        if i > 0 {
                            info!(
                                "[quick_index] Successfully wrote block {block_number} after {i} retries"
                            );
                        }
                        return Ok(());
                    }
                    Err(e) => warn!("[quick_index] Error writing block {block_number}: {e}"),
                },
                Err(e) => warn!(
                    "[quick_index] Error retrieving block {}: {}",
                    block_number, e
                ),
            }
            // Exponential backoff
            let backoff = (i as u64).pow(2) * 5;
            tokio::time::sleep(Duration::from_secs(backoff)).await;
        }
        error!("[quick_index] Error with block number {}", block_number);
        Err(eyre::anyhow!("Failed to process block {}", block_number))
    }
}
