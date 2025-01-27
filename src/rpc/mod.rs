use crate::utils::convert_hex_string_to_i64;
use eyre::{Context, Result};
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{future::Future, time::Duration};
use tokio::time::sleep;
use tracing::{error, warn};

#[cfg(test)]
use std::sync::mpsc::SyncSender;

// TODO: instead of keeping this static, make it passable as a dependency.
// This should allow us to test this module.
static CLIENT: Lazy<Client> = Lazy::new(Client::new);
static NODE_CONNECTION_STRING: Lazy<Option<String>> = Lazy::new(|| {
    dotenvy::var("NODE_CONNECTION_STRING")
        .map_err(|e| error!("Failed to get NODE_CONNECTION_STRING: {}", e))
        .ok()
});

// Arbitrarily set, can be set somewhere else.
const MAX_RETRIES: u8 = 5;

#[derive(Deserialize, Debug)]
pub struct RpcResponse<T> {
    pub result: T,
}

#[derive(Serialize)]
struct RpcRequest<'a, T> {
    jsonrpc: &'a str,
    id: String,
    method: &'a str,
    params: T,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Transaction {
    pub hash: String,
    #[serde(rename(deserialize = "blockNumber"))]
    pub block_number: String,
    #[serde(rename(deserialize = "transactionIndex"))]
    pub transaction_index: String,
    pub value: String,
    #[serde(rename(deserialize = "gasPrice"))]
    pub gas_price: Option<String>,
    pub gas: String,
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(rename(deserialize = "maxPriorityFeePerGas"))]
    pub max_priority_fee_per_gas: Option<String>,
    #[serde(rename(deserialize = "maxFeePerGas"))]
    pub max_fee_per_gas: Option<String>,
    #[serde(rename(deserialize = "chainId"))]
    pub chain_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct BlockHeaderWithEmptyTransaction {
    #[serde(rename(deserialize = "gasLimit"))]
    pub gas_limit: String,
    #[serde(rename(deserialize = "gasUsed"))]
    pub gas_used: String,
    #[serde(rename(deserialize = "baseFeePerGas"))]
    pub base_fee_per_gas: Option<String>,
    pub hash: String,
    pub nonce: Option<String>,
    pub number: String,
    #[serde(rename(deserialize = "receiptsRoot"))]
    pub receipts_root: Option<String>,
    #[serde(rename(deserialize = "stateRoot"))]
    pub state_root: Option<String>,
    #[serde(rename(deserialize = "transactionsRoot"))]
    pub transactions_root: Option<String>,
    #[serde(rename(deserialize = "parentHash"))]
    pub parent_hash: Option<String>,
    #[serde(rename(deserialize = "miner"))]
    pub miner: Option<String>,
    #[serde(rename(deserialize = "logsBloom"))]
    pub logs_bloom: Option<String>,
    #[serde(rename(deserialize = "difficulty"))]
    pub difficulty: Option<String>,
    #[serde(rename(deserialize = "totalDifficulty"))]
    pub total_difficulty: Option<String>,
    #[serde(rename(deserialize = "sha3Uncles"))]
    pub sha3_uncles: Option<String>,
    #[serde(rename(deserialize = "timestamp"))]
    pub timestamp: String,
    #[serde(rename(deserialize = "extraData"))]
    pub extra_data: Option<String>,
    #[serde(rename(deserialize = "mixHash"))]
    pub mix_hash: Option<String>,
    #[serde(rename(deserialize = "withdrawalsRoot"))]
    pub withdrawals_root: Option<String>,
    #[serde(rename(deserialize = "blobGasUsed"))]
    pub blob_gas_used: Option<String>,
    #[serde(rename(deserialize = "excessBlobGas"))]
    pub excess_blob_gas: Option<String>,
    #[serde(rename(deserialize = "parentBeaconBlockRoot"))]
    pub parent_beacon_block_root: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BlockHeaderWithFullTransaction {
    #[serde(rename(deserialize = "gasLimit"))]
    pub gas_limit: String,
    #[serde(rename(deserialize = "gasUsed"))]
    pub gas_used: String,
    #[serde(rename(deserialize = "baseFeePerGas"))]
    pub base_fee_per_gas: Option<String>,
    pub hash: String,
    pub nonce: Option<String>,
    pub number: String,
    #[serde(rename(deserialize = "receiptsRoot"))]
    pub receipts_root: Option<String>,
    #[serde(rename(deserialize = "stateRoot"))]
    pub state_root: Option<String>,
    #[serde(rename(deserialize = "transactionsRoot"))]
    pub transactions_root: Option<String>,
    pub transactions: Vec<Transaction>,
    #[serde(rename(deserialize = "parentHash"))]
    pub parent_hash: Option<String>,
    pub miner: Option<String>,
    #[serde(rename(deserialize = "logsBloom"))]
    pub logs_bloom: Option<String>,
    #[serde(rename(deserialize = "difficulty"))]
    pub difficulty: Option<String>,
    #[serde(rename(deserialize = "totalDifficulty"))]
    pub total_difficulty: Option<String>,
    #[serde(rename(deserialize = "sha3Uncles"))]
    pub sha3_uncles: Option<String>,
    #[serde(rename(deserialize = "timestamp"))]
    pub timestamp: String,
    #[serde(rename(deserialize = "extraData"))]
    pub extra_data: Option<String>,
    #[serde(rename(deserialize = "mixHash"))]
    pub mix_hash: Option<String>,
    #[serde(rename(deserialize = "withdrawalsRoot"))]
    pub withdrawals_root: Option<String>,
    #[serde(rename(deserialize = "blobGasUsed"))]
    pub blob_gas_used: Option<String>,
    #[serde(rename(deserialize = "excessBlobGas"))]
    pub excess_blob_gas: Option<String>,
    #[serde(rename(deserialize = "parentBeaconBlockRoot"))]
    pub parent_beacon_block_root: Option<String>,
}

impl From<BlockHeaderWithFullTransaction> for BlockHeaderWithEmptyTransaction {
    fn from(val: BlockHeaderWithFullTransaction) -> Self {
        BlockHeaderWithEmptyTransaction {
            gas_limit: val.gas_limit,
            gas_used: val.gas_used,
            base_fee_per_gas: val.base_fee_per_gas,
            hash: val.hash,
            nonce: val.nonce,
            number: val.number,
            receipts_root: val.receipts_root,
            state_root: val.state_root,
            transactions_root: val.transactions_root,
            miner: val.miner,
            logs_bloom: val.logs_bloom,
            difficulty: val.difficulty,
            total_difficulty: val.total_difficulty,
            sha3_uncles: val.sha3_uncles,
            extra_data: val.extra_data,
            mix_hash: val.mix_hash,
            parent_hash: val.parent_hash,
            timestamp: val.timestamp,
            withdrawals_root: val.withdrawals_root,
            blob_gas_used: val.blob_gas_used,
            excess_blob_gas: val.excess_blob_gas,
            parent_beacon_block_root: val.parent_beacon_block_root,
        }
    }
}

pub async fn get_latest_finalized_blocknumber(timeout: Option<u64>) -> Result<i64> {
    // TODO: Id should be different on every request, this is how request are identified by us and by the node.
    let params = RpcRequest {
        jsonrpc: "2.0",
        id: "0".to_string(),
        method: "eth_getBlockByNumber",
        params: ("finalized", false),
    };

    match make_retrying_rpc_call::<_, BlockHeaderWithEmptyTransaction>(
        &params,
        timeout,
        MAX_RETRIES.into(),
    )
    .await
    .context("Failed to get latest block number")
    {
        Ok(blockheader) => Ok(convert_hex_string_to_i64(&blockheader.number)?),
        Err(e) => Err(e),
    }
}

pub async fn get_full_block_by_number(
    number: i64,
    timeout: Option<u64>,
) -> Result<BlockHeaderWithFullTransaction> {
    let params = RpcRequest {
        jsonrpc: "2.0",
        id: "0".to_string(),
        method: "eth_getBlockByNumber",
        params: (format!("0x{:x}", number), true),
    };

    make_retrying_rpc_call::<_, BlockHeaderWithFullTransaction>(
        &params,
        timeout,
        MAX_RETRIES.into(),
    )
    .await
}

// TODO: Make this work as expected
#[allow(dead_code)]
pub async fn batch_get_full_block_by_number(
    numbers: Vec<i64>,
    timeout: Option<u64>,
) -> Result<Vec<BlockHeaderWithFullTransaction>> {
    let mut params = Vec::new();
    for number in numbers {
        let num_str = number.to_string();
        params.push(RpcRequest {
            jsonrpc: "2.0",
            id: num_str,
            method: "eth_getBlockByNumber",
            params: (format!("0x{:x}", number), true),
        });
    }
    make_rpc_call::<_, Vec<BlockHeaderWithFullTransaction>>(&params, timeout).await
}

async fn make_rpc_call<T: Serialize, R: for<'de> Deserialize<'de>>(
    params: &T,
    timeout: Option<u64>,
) -> Result<R> {
    let connection_string = (*NODE_CONNECTION_STRING)
        .as_ref()
        .ok_or_else(|| eyre::eyre!("NODE_CONNECTION_STRING not set"))?;

    let raw_response = match timeout {
        Some(seconds) => {
            CLIENT
                .post(connection_string)
                .timeout(Duration::from_secs(seconds))
                .json(params)
                .send()
                .await
        }
        None => CLIENT.post(connection_string).json(params).send().await,
    };

    let raw_response = match raw_response {
        Ok(response) => response,
        Err(e) => {
            error!("HTTP request error: {:?}", e);
            return Err(e.into());
        }
    };

    // Attempt to extract JSON from the response
    let json_response = raw_response.text().await;
    match json_response {
        Ok(text) => {
            // Try to deserialize the response, logging if it fails
            match serde_json::from_str::<RpcResponse<R>>(&text) {
                Ok(parsed) => Ok(parsed.result),
                Err(e) => {
                    error!(
                        "Deserialization error: {:?}\nResponse snippet: {:?}",
                        e,
                        text // Log the entire response
                    );
                    Err(e.into())
                }
            }
        }
        Err(e) => {
            error!("Failed to read response body: {:?}", e);
            Err(e.into())
        }
    }
}

async fn make_retrying_rpc_call<T: Serialize, R: for<'de> Deserialize<'de> + Send>(
    params: &T,
    timeout: Option<u64>,
    max_retries: u32,
) -> Result<R> {
    let mut attempts = 0;
    loop {
        match make_rpc_call(params, timeout).await {
            Ok(result) => return Ok(result),
            Err(e) => {
                attempts += 1;
                if attempts > max_retries {
                    warn!("Operation failed with error: {:?}. Max retries reached", e);
                    return Err(e);
                }
                let backoff = Duration::from_secs(2_u64.pow(attempts));
                warn!(
                    "Operation failed with error: {:?}. Retrying in {:?} (Attempt {}/{})",
                    e, backoff, attempts, max_retries
                );
                sleep(backoff).await;
            }
        }
    }
}

#[allow(dead_code)]
pub trait EthereumRpcProvider {
    fn get_latest_finalized_blocknumber(
        &self,
        timeout: Option<u64>,
    ) -> impl Future<Output = Result<i64>> + Send;
    fn get_full_block_by_number(
        &self,
        number: i64,
        include_tx: bool,
        timeout: Option<u64>,
    ) -> impl Future<Output = Result<BlockHeaderWithFullTransaction>> + Send;
}

#[derive(Debug, Clone)]
pub struct EthereumJsonRpcClient {
    client: reqwest::Client,
    connection_string: String,
    max_retries: u32,
}

impl EthereumJsonRpcClient {
    #[allow(dead_code)]
    pub fn new(connection_string: String, max_retries: u32) -> Self {
        Self {
            client: reqwest::Client::new(),
            connection_string,
            max_retries,
        }
    }
}

impl EthereumJsonRpcClient {
    async fn make_rpc_call<T: Serialize + Send + Sync, R: for<'de> Deserialize<'de> + Send>(
        &self,
        params: T,
        timeout: Option<u64>,
    ) -> Result<R> {
        let connection_string = self.connection_string.clone();

        let raw_response = match timeout {
            Some(seconds) => {
                self.client
                    .post(connection_string)
                    .timeout(Duration::from_secs(seconds))
                    .json(&params)
                    .send()
                    .await
            }
            None => {
                self.client
                    .post(connection_string)
                    .json(&params)
                    .send()
                    .await
            }
        };

        let raw_response = match raw_response {
            Ok(response) => response,
            Err(e) => {
                error!("HTTP request error: {:?}", e);
                return Err(e.into());
            }
        };

        // Attempt to extract JSON from the response
        let json_response = raw_response.text().await;
        match json_response {
            Ok(text) => {
                // Try to deserialize the response, logging if it fails
                match serde_json::from_str::<RpcResponse<R>>(&text) {
                    Ok(parsed) => Ok(parsed.result),
                    Err(e) => {
                        error!(
                            "Deserialization error: {:?}\nResponse snippet: {:?}",
                            e,
                            text // Log the entire response
                        );
                        Err(e.into())
                    }
                }
            }
            Err(e) => {
                error!("Failed to read response body: {:?}", e);
                Err(e.into())
            }
        }
    }

    async fn make_retrying_rpc_call<
        T: Serialize + Send + Sync + Clone,
        R: for<'de> Deserialize<'de> + Send,
    >(
        &self,
        params: T,
        timeout: Option<u64>,
    ) -> Result<R> {
        let mut attempts = 0;
        loop {
            match self.make_rpc_call(params.clone(), timeout).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    attempts += 1;
                    if attempts > self.max_retries {
                        warn!("Operation failed with error: {:?}. Max retries reached", e);
                        return Err(e);
                    }
                    let backoff = Duration::from_secs(2_u64.pow(attempts));
                    warn!(
                        "Operation failed with error: {:?}. Retrying in {:?} (Attempt {}/{})",
                        e, backoff, attempts, self.max_retries
                    );
                    sleep(backoff).await;
                }
            }
        }
    }
}

impl EthereumRpcProvider for EthereumJsonRpcClient {
    async fn get_latest_finalized_blocknumber(&self, timeout: Option<u64>) -> Result<i64> {
        let params = RpcRequest {
            jsonrpc: "2.0",
            id: "0".to_string(),
            method: "eth_getBlockByNumber",
            params: ("finalized", false),
        };

        match self
            .make_retrying_rpc_call::<_, BlockHeaderWithEmptyTransaction>(&params, timeout)
            .await
            .context("Failed to get latest block number")
        {
            Ok(blockheader) => Ok(convert_hex_string_to_i64(&blockheader.number)?),
            Err(e) => Err(e),
        }
    }

    async fn get_full_block_by_number(
        &self,
        number: i64,
        include_tx: bool,
        timeout: Option<u64>,
    ) -> Result<BlockHeaderWithFullTransaction> {
        let params = RpcRequest {
            jsonrpc: "2.0",
            id: "0".to_string(),
            method: "eth_getBlockByNumber",
            params: (format!("0x{:x}", number), include_tx),
        };

        self.make_retrying_rpc_call::<_, BlockHeaderWithFullTransaction>(&params, timeout)
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc::sync_channel, thread};

    use super::*;
    use tokio::{
        fs,
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    async fn get_fixtures_for_tests() -> (String, BlockHeaderWithFullTransaction) {
        let block_21598014_string =
            fs::read_to_string("tests/fixtures/eth_getBlockByNumber_21598014.json")
                .await
                .unwrap();

        let block_21598014_response = serde_json::from_str::<
            RpcResponse<BlockHeaderWithFullTransaction>,
        >(&block_21598014_string)
        .unwrap();

        (block_21598014_string, block_21598014_response.result)
    }

    // Helper function to start a TCP server that returns predefined JSON-RPC responses
    // Taken from katana codebase.
    pub fn start_mock_rpc_server(addr: String, responses: Vec<String>) -> SyncSender<()> {
        use tokio::runtime::Builder;
        let (tx, rx) = sync_channel::<()>(1);

        thread::spawn(move || {
            Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async move {
                    let listener = TcpListener::bind(addr).await.unwrap();

                    let mut counter = 0;

                    // Wait for a signal to return the response.
                    rx.recv().unwrap();

                    loop {
                        let (mut socket, _) = listener.accept().await.unwrap();

                        // Read the request, so hyper would not close the connection
                        let mut buffer = [0; 1024];
                        let _ = socket.read(&mut buffer).await.unwrap();

                        // Get the response from the pre-determined list, and if the list is
                        // exhausted, return the last response.
                        let response = if counter < responses.len() {
                            responses[counter].clone()
                        } else {
                            responses.last().unwrap().clone()
                        };

                        // After reading, we send the pre-determined response
                        let http_response = format!(
                            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\ncontent-type: \
                             application/json\r\n\r\n{}",
                            response.len(),
                            response
                        );

                        socket.write_all(http_response.as_bytes()).await.unwrap();
                        socket.flush().await.unwrap();
                        counter += 1;
                    }
                });
        });

        // Returning the sender to allow controlling the response timing.
        tx
    }

    #[test]
    fn test_into_should_convert_empty_tx_from_full_tx() {
        let full_tx = BlockHeaderWithFullTransaction {
            gas_limit: "0x0".to_string(),
            gas_used: "0x0".to_string(),
            base_fee_per_gas: Some("0x0".to_string()),
            hash: "0x0".to_string(),
            nonce: Some("0x0".to_string()),
            number: "0x0".to_string(),
            receipts_root: Some("0x0".to_string()),
            state_root: Some("0x0".to_string()),
            transactions_root: Some("0x0".to_string()),
            miner: Some("0x0".to_string()),
            logs_bloom: Some("0x0".to_string()),
            difficulty: Some("0x0".to_string()),
            total_difficulty: Some("0x0".to_string()),
            sha3_uncles: Some("0x0".to_string()),
            timestamp: "0x0".to_string(),
            extra_data: Some("0x0".to_string()),
            mix_hash: Some("0x0".to_string()),
            parent_hash: Some("0x0".to_string()),
            withdrawals_root: Some("0x0".to_string()),
            blob_gas_used: Some("0x0".to_string()),
            excess_blob_gas: Some("0x0".to_string()),
            parent_beacon_block_root: Some("0x0".to_string()),
            transactions: vec![Transaction {
                hash: "0x0".to_string(),
                block_number: "0x0".to_string(),
                transaction_index: "0x0".to_string(),
                value: "0x0".to_string(),
                gas_price: Some("0x0".to_string()),
                gas: "0x0".to_string(),
                from: Some("0x0".to_string()),
                to: Some("0x0".to_string()),
                max_priority_fee_per_gas: Some("0x0".to_string()),
                max_fee_per_gas: Some("0x0".to_string()),
                chain_id: Some("0x0".to_string()),
            }],
        };

        let empty_tx: BlockHeaderWithEmptyTransaction = full_tx.clone().into();
        assert_eq!(empty_tx.number, full_tx.number);
        assert_eq!(empty_tx.hash, full_tx.hash);
        assert_eq!(empty_tx.nonce, full_tx.nonce);
        assert_eq!(empty_tx.parent_hash, full_tx.parent_hash);
        assert_eq!(empty_tx.gas_limit, full_tx.gas_limit);
        assert_eq!(empty_tx.gas_used, full_tx.gas_used);
        assert_eq!(empty_tx.base_fee_per_gas, full_tx.base_fee_per_gas);
        assert_eq!(empty_tx.receipts_root, full_tx.receipts_root);
        assert_eq!(empty_tx.state_root, full_tx.state_root);
        assert_eq!(empty_tx.transactions_root, full_tx.transactions_root);
        assert_eq!(empty_tx.miner, full_tx.miner);
        assert_eq!(empty_tx.logs_bloom, full_tx.logs_bloom);
        assert_eq!(empty_tx.difficulty, full_tx.difficulty);
        assert_eq!(empty_tx.total_difficulty, full_tx.total_difficulty);
        assert_eq!(empty_tx.sha3_uncles, full_tx.sha3_uncles);
        assert_eq!(empty_tx.timestamp, full_tx.timestamp);
        assert_eq!(empty_tx.extra_data, full_tx.extra_data);
        assert_eq!(empty_tx.mix_hash, full_tx.mix_hash);
        assert_eq!(empty_tx.withdrawals_root, full_tx.withdrawals_root);
        assert_eq!(empty_tx.blob_gas_used, full_tx.blob_gas_used);
        assert_eq!(empty_tx.excess_blob_gas, full_tx.excess_blob_gas);
        assert_eq!(
            empty_tx.parent_beacon_block_root,
            full_tx.parent_beacon_block_root
        );
    }

    #[test]
    fn test_from_should_convert_empty_tx_from_full_tx() {
        let full_tx = BlockHeaderWithFullTransaction {
            gas_limit: "0x0".to_string(),
            gas_used: "0x0".to_string(),
            base_fee_per_gas: Some("0x0".to_string()),
            hash: "0x0".to_string(),
            nonce: Some("0x0".to_string()),
            number: "0x0".to_string(),
            receipts_root: Some("0x0".to_string()),
            state_root: Some("0x0".to_string()),
            transactions_root: Some("0x0".to_string()),
            miner: Some("0x0".to_string()),
            logs_bloom: Some("0x0".to_string()),
            difficulty: Some("0x0".to_string()),
            total_difficulty: Some("0x0".to_string()),
            sha3_uncles: Some("0x0".to_string()),
            timestamp: "0x0".to_string(),
            extra_data: Some("0x0".to_string()),
            mix_hash: Some("0x0".to_string()),
            parent_hash: Some("0x0".to_string()),
            withdrawals_root: Some("0x0".to_string()),
            blob_gas_used: Some("0x0".to_string()),
            excess_blob_gas: Some("0x0".to_string()),
            parent_beacon_block_root: Some("0x0".to_string()),
            transactions: vec![Transaction {
                hash: "0x0".to_string(),
                block_number: "0x0".to_string(),
                transaction_index: "0x0".to_string(),
                value: "0x0".to_string(),
                gas_price: Some("0x0".to_string()),
                gas: "0x0".to_string(),
                from: Some("0x0".to_string()),
                to: Some("0x0".to_string()),
                max_priority_fee_per_gas: Some("0x0".to_string()),
                max_fee_per_gas: Some("0x0".to_string()),
                chain_id: Some("0x0".to_string()),
            }],
        };
        let empty_tx = BlockHeaderWithEmptyTransaction::from(full_tx.clone());
        assert_eq!(empty_tx.number, full_tx.number);
        assert_eq!(empty_tx.hash, full_tx.hash);
        assert_eq!(empty_tx.nonce, full_tx.nonce);
        assert_eq!(empty_tx.parent_hash, full_tx.parent_hash);
        assert_eq!(empty_tx.gas_limit, full_tx.gas_limit);
        assert_eq!(empty_tx.gas_used, full_tx.gas_used);
        assert_eq!(empty_tx.base_fee_per_gas, full_tx.base_fee_per_gas);
        assert_eq!(empty_tx.receipts_root, full_tx.receipts_root);
        assert_eq!(empty_tx.state_root, full_tx.state_root);
        assert_eq!(empty_tx.transactions_root, full_tx.transactions_root);
        assert_eq!(empty_tx.miner, full_tx.miner);
        assert_eq!(empty_tx.logs_bloom, full_tx.logs_bloom);
        assert_eq!(empty_tx.difficulty, full_tx.difficulty);
        assert_eq!(empty_tx.total_difficulty, full_tx.total_difficulty);
        assert_eq!(empty_tx.sha3_uncles, full_tx.sha3_uncles);
        assert_eq!(empty_tx.timestamp, full_tx.timestamp);
        assert_eq!(empty_tx.extra_data, full_tx.extra_data);
        assert_eq!(empty_tx.mix_hash, full_tx.mix_hash);
        assert_eq!(empty_tx.withdrawals_root, full_tx.withdrawals_root);
        assert_eq!(empty_tx.blob_gas_used, full_tx.blob_gas_used);
        assert_eq!(empty_tx.excess_blob_gas, full_tx.excess_blob_gas);
        assert_eq!(
            empty_tx.parent_beacon_block_root,
            full_tx.parent_beacon_block_root
        );
    }

    #[tokio::test]
    async fn test_max_retries_should_affect_number_of_retries() {
        let (rpc_response_string, _) = get_fixtures_for_tests().await;
        let sender = start_mock_rpc_server(
            "127.0.0.1:8091".to_owned(),
            vec![
                "{}".to_string(),
                "{}".to_string(),
                "{}".to_string(),
                rpc_response_string,
                "{}".to_string(),
            ], // introduce empty responses
        );

        // Set max retries to 2, which shouldn't have any success cases.
        let client = EthereumJsonRpcClient::new("http://127.0.0.1:8091".to_owned(), 2);

        let block = client.get_full_block_by_number(21598014, false, None);

        // Trigger the response
        sender.send(()).unwrap();

        let block = block.await;
        assert!(block.is_err());
    }

    #[tokio::test]
    async fn test_get_full_block_by_number_should_retry_when_failed() {
        let (rpc_response_string, rpc_response_struct) = get_fixtures_for_tests().await;
        let sender = start_mock_rpc_server(
            "127.0.0.1:8092".to_owned(),
            vec!["{}".to_string(), rpc_response_string], // introduce an empty response to induce failure
        );

        let client = EthereumJsonRpcClient::new("http://127.0.0.1:8092".to_owned(), 5);
        let block = client.get_full_block_by_number(21598014, false, None);

        // Trigger the response
        sender.send(()).unwrap();

        let block = block.await.unwrap();
        assert_eq!(block.hash, rpc_response_struct.hash);
        assert_eq!(block.number, rpc_response_struct.number);
    }

    #[tokio::test]
    async fn test_get_latest_finalized_blocknumber_should_retry_when_failed() {
        let (rpc_response_string, rpc_response_struct) = get_fixtures_for_tests().await;
        let sender = start_mock_rpc_server(
            "127.0.0.1:8093".to_owned(),
            vec!["{}".to_string(), rpc_response_string], // introduce an empty response to induce failure
        );

        let client = EthereumJsonRpcClient::new("http://127.0.0.1:8093".to_owned(), 5);
        let block_number = client.get_latest_finalized_blocknumber(None);

        // Trigger the response
        sender.send(()).unwrap();

        let block_number = block_number.await.unwrap();
        assert_eq!(
            block_number,
            i64::from_str_radix(rpc_response_struct.number.strip_prefix("0x").unwrap(), 16)
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_get_full_block_by_number() {
        let (rpc_response_string, rpc_response_struct) = get_fixtures_for_tests().await;
        let sender = start_mock_rpc_server("127.0.0.1:8094".to_owned(), vec![rpc_response_string]);

        let client = EthereumJsonRpcClient::new("http://127.0.0.1:8094".to_owned(), 5);
        let block = client.get_full_block_by_number(21598014, false, None);

        // Trigger the response
        sender.send(()).unwrap();

        let block = block.await.unwrap();
        assert_eq!(block.hash, rpc_response_struct.hash);
        assert_eq!(block.number, rpc_response_struct.number);
    }

    #[tokio::test]
    async fn test_get_latest_finalized_blocknumber() {
        let (rpc_response_string, rpc_response_struct) = get_fixtures_for_tests().await;
        let sender = start_mock_rpc_server("127.0.0.1:8095".to_owned(), vec![rpc_response_string]);

        let client = EthereumJsonRpcClient::new("http://127.0.0.1:8095".to_owned(), 5);
        let block_number = client.get_latest_finalized_blocknumber(None);

        // Trigger the response
        sender.send(()).unwrap();

        let block_number = block_number.await.unwrap();
        assert_eq!(
            block_number,
            i64::from_str_radix(rpc_response_struct.number.strip_prefix("0x").unwrap(), 16)
                .unwrap()
        );
    }

    // TODO: Add more tests for the failure cases.
}
