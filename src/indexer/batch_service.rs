use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use eyre::{anyhow, Result};
use futures::future::try_join_all;
use tokio::task;
use tracing::{error, info, warn};

use crate::{
    db::DbConnection,
    repositories::{
        block_header::insert_block_header_query,
        index_metadata::{get_index_metadata, update_backfilling_block_number_query},
    },
    rpc,
};

#[derive(Debug)]
pub struct BatchIndexConfig {
    // TODO: maybe reconsidering these variables? Since we share the same with quick for now
    pub max_retries: u8,
    pub poll_interval: u32,
    pub rpc_timeout: u32,
    pub index_batch_size: u32,
}

impl Default for BatchIndexConfig {
    fn default() -> Self {
        Self {
            max_retries: 10,
            poll_interval: 10,
            rpc_timeout: 300,
            index_batch_size: 50,
        }
    }
}

pub struct BatchIndexer {
    config: BatchIndexConfig,
    db: Arc<DbConnection>,
    should_terminate: Arc<AtomicBool>,
}

impl BatchIndexer {
    pub async fn new(
        config: BatchIndexConfig,
        db: Arc<DbConnection>,
        should_terminate: Arc<AtomicBool>,
    ) -> BatchIndexer {
        Self {
            db,
            config,
            should_terminate,
        }
    }

    // TODO: Since this is similar to the quick indexer with the only exception being the block logic,
    // maybe we could DRY this?
    pub async fn index(&self) -> Result<()> {
        // Batch indexer loop. does the following until terminated:
        // 1. check if finished indexing (backfilling block is 0)
        // 2. check current starting block and backfilling block
        // 3. if not fully indexed, index the block in batches (20 seems to be good for quick, but should be adjustable)
        // 4. if it is fully indexed, do nothing (maybe we could exit this too?)
        while !self.should_terminate.load(Ordering::Relaxed) {
            let current_index_metadata = match get_index_metadata(self.db.clone()).await {
                Ok(metadata) => {
                    if let Some(metadata) = metadata {
                        metadata
                    } else {
                        error!("[batch_index] Error getting index metadata");
                        return Err(anyhow!("Error getting index metadata: metadata not found."));
                    }
                }
                Err(e) => {
                    error!("[batch_index] Error getting index metadata: {}", e);
                    return Err(e);
                }
            };

            let index_start_block_number = current_index_metadata.indexing_starting_block_number;
            let current_backfilling_block_number =
                if let Some(block_number) = current_index_metadata.backfilling_block_number {
                    block_number
                } else {
                    index_start_block_number
                };

            if current_backfilling_block_number == 0 {
                info!("[batch_index] Backfilling complete, terminating backfilling process.");
                break;
            }

            if current_backfilling_block_number > 0 {
                let backfilling_target_block =
                    current_backfilling_block_number - i64::from(self.config.index_batch_size);
                let starting_block_number: i64 = if backfilling_target_block < 0 {
                    0
                } else {
                    backfilling_target_block
                };

                self.index_block_range(
                    starting_block_number,
                    current_backfilling_block_number,
                    &self.should_terminate,
                )
                .await?;
            } else {
                info!("[batch_index] Backfilling complete, terminating backfilling process.");
                break;
            }
        }
        Ok(())
    }

    // Indexing a block range, inclusive.
    pub async fn index_block_range(
        &self,
        starting_block: i64,
        ending_block: i64,
        should_terminate: &AtomicBool,
    ) -> Result<()> {
        let block_range: Vec<i64> = (starting_block..ending_block + 1).collect();

        for i in 0..self.config.max_retries {
            if should_terminate.load(Ordering::Relaxed) {
                info!("[batch_index] Termination requested. Stopping quick indexing.");
                break;
            }

            let timeout = self.config.rpc_timeout;

            let rpc_block_headers_futures: Vec<_> = block_range
                .iter()
                .map(|block_number| {
                    task::spawn(rpc::get_full_block_by_number(
                        *block_number,
                        Some(timeout.into()),
                    ))
                })
                .collect();

            let rpc_block_headers_response = try_join_all(rpc_block_headers_futures).await?;

            let mut block_headers = Vec::with_capacity(rpc_block_headers_response.len());
            let mut has_err = false;

            for header in rpc_block_headers_response.into_iter() {
                match header {
                    Ok(header) => {
                        block_headers.push(header);
                    }
                    Err(e) => {
                        has_err = true;
                        warn!(
                            "[batch_index] Error retrieving block in range from {} to {}. error: {}",
                            starting_block, ending_block, e
                        )
                    }
                }
            }

            if !has_err {
                let mut db_tx = self.db.pool.begin().await?;

                insert_block_header_query(&mut db_tx, block_headers).await?;
                update_backfilling_block_number_query(&mut db_tx, starting_block).await?;

                // Commit at the end
                db_tx.commit().await?;

                info!(
                    "[batch_index] Indexing block range from {} to {} complete.",
                    starting_block, ending_block
                );
                return Ok(());
            }

            // If there's an error during rpc, retry.
            error!("[batch_index] Error encountered during rpc, retry no. {}. Re-running from block: {}", i, starting_block);

            // Exponential backoff
            let backoff = (i as u64).pow(2) * 5;
            tokio::time::sleep(Duration::from_secs(backoff)).await;
        }

        Err(anyhow!("Max retries reached. Stopping batch indexing."))
    }
}
