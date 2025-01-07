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
        index_metadata::{get_index_metadata, update_latest_quick_index_block_number_query},
    },
    rpc::{self},
};

#[derive(Debug)]
pub struct QuickIndexConfig {
    pub starting_block: i64,
    pub max_retries: u8,
    pub poll_interval: u32,
    pub rpc_timeout: u32,
    pub index_batch_size: u32,
}

impl Default for QuickIndexConfig {
    fn default() -> Self {
        Self {
            starting_block: 0,
            max_retries: 10,
            poll_interval: 10,
            rpc_timeout: 300,
            index_batch_size: 20,
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
                Ok(metadata) => {
                    if let Some(metadata) = metadata {
                        metadata.current_latest_block_number
                    } else {
                        error!("[quick_index] Error getting index metadata");
                        return Err(anyhow!("Error getting index metadata: metadata not found."));
                    }
                }
                Err(e) => {
                    error!("[quick_index] Error getting index metadata: {}", e);
                    return Err(e);
                }
            };

            let new_latest_block =
                rpc::get_latest_finalized_blocknumber(Some(self.config.rpc_timeout.into())).await?;

            if new_latest_block > last_block_number {
                let ending_block_number: i64 =
                    if new_latest_block - last_block_number > self.config.index_batch_size.into() {
                        last_block_number + i64::from(self.config.index_batch_size)
                    } else {
                        new_latest_block
                    };

                self.index_block_range(
                    last_block_number,
                    ending_block_number,
                    &self.should_terminate,
                )
                .await?;
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

    // Indexing a block range, inclusive.
    async fn index_block_range(
        &self,
        starting_block: i64,
        ending_block: i64,
        should_terminate: &AtomicBool,
    ) -> Result<()> {
        let block_range: Vec<i64> = (starting_block..ending_block + 1).collect();

        for i in 0..self.config.max_retries {
            if should_terminate.load(Ordering::Relaxed) {
                info!("[quick_index] Termination requested. Stopping quick indexing.");
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
                            "[quick_index] Error retrieving block in range from {} to {}. error: {}",
                            starting_block, ending_block, e
                        )
                    }
                }
            }

            if !has_err {
                let mut db_tx = self.db.pool.begin().await?;

                insert_block_header_query(&mut db_tx, block_headers).await?;
                update_latest_quick_index_block_number_query(&mut db_tx, ending_block).await?;

                // Commit at the end
                db_tx.commit().await?;

                info!(
                    "[quick_index] Indexing block range from {} to {} complete.",
                    starting_block, ending_block
                );
                break;
            }

            // If there's an error during rpc, retry.
            error!("[quick_index] Rerun from block: {}", starting_block);

            // Exponential backoff
            let backoff = (i as u64).pow(2) * 5;
            tokio::time::sleep(Duration::from_secs(backoff)).await;
        }

        Ok(())
    }
}
