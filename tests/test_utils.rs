use std::collections::VecDeque;

use eyre::{anyhow, Result};
use tokio::sync::Mutex;

use fossil_headers_db::rpc::{BlockHeaderWithFullTransaction, EthereumRpcProvider};

pub struct IntegrationRpcProvider {
    pub latest_finalized_blocknumber_vec: Mutex<VecDeque<i64>>,
    pub full_block_vec: Mutex<VecDeque<BlockHeaderWithFullTransaction>>,
}

impl Default for IntegrationRpcProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl IntegrationRpcProvider {
    pub fn new() -> Self {
        Self {
            latest_finalized_blocknumber_vec: Mutex::new(VecDeque::new()),
            full_block_vec: Mutex::new(VecDeque::new()),
        }
    }
}

impl EthereumRpcProvider for IntegrationRpcProvider {
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
