use std::collections::VecDeque;

use eyre::{anyhow, Result};
use tokio::sync::Mutex;

use crate::rpc::{BlockHeaderWithFullTransaction, EthereumRpcProvider};

pub struct MockRpcProvider {
    pub latest_finalized_blocknumber_vec: Mutex<VecDeque<i64>>,
    pub full_block_vec: Mutex<VecDeque<BlockHeaderWithFullTransaction>>,
}

impl Default for MockRpcProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockRpcProvider {
    pub fn new() -> Self {
        Self {
            latest_finalized_blocknumber_vec: Mutex::new(VecDeque::new()),
            full_block_vec: Mutex::new(VecDeque::new()),
        }
    }

    pub fn new_with_data(
        latest_finalized_blocknumber_vec: VecDeque<i64>,
        full_block_vec: VecDeque<BlockHeaderWithFullTransaction>,
    ) -> Self {
        Self {
            latest_finalized_blocknumber_vec: Mutex::new(latest_finalized_blocknumber_vec),
            full_block_vec: Mutex::new(full_block_vec),
        }
    }
}

impl EthereumRpcProvider for MockRpcProvider {
    async fn get_latest_finalized_blocknumber(&self, _timeout: Option<u64>) -> Result<i64> {
        if let Some(res) = self
            .latest_finalized_blocknumber_vec
            .lock()
            .await
            .pop_front()
        {
            return Ok(res);
        }
        Err(anyhow!("Failed to get latest finalized block number"))
    }

    async fn get_full_block_by_number(
        &self,
        _number: i64,
        _include_tx: bool,
        _timeout: Option<u64>,
    ) -> Result<BlockHeaderWithFullTransaction> {
        if let Some(res) = self.full_block_vec.lock().await.pop_front() {
            return Ok(res);
        }
        Err(anyhow!("Failed to get full block by number"))
    }
}
