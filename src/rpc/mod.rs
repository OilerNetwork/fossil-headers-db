use crate::utils::convert_hex_string_to_i64;
use eyre::{Context, Result};
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{future::Future, time::Duration};
use tokio::time::sleep;
use tracing::{error, warn};

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
    pub receipts_root: String,
    #[serde(rename(deserialize = "stateRoot"))]
    pub state_root: String,
    #[serde(rename(deserialize = "transactionsRoot"))]
    pub transactions_root: String,
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

async fn make_retrying_rpc_call<T: Serialize, R: for<'de> Deserialize<'de>>(
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
pub trait EthereumRpcClient {
    fn get_latest_finalized_blocknumber(
        &self,
        timeout: Option<u64>,
    ) -> impl Future<Output = Result<i64>> + Send;
    fn get_full_block_by_number(
        &self,
        number: i64,
        timeout: Option<u64>,
    ) -> impl Future<Output = Result<BlockHeaderWithFullTransaction>> + Send;
}

#[derive(Debug, Clone)]
pub struct HttpEthereumRpcClient {
    client: reqwest::Client,
    connection_string: String,
    max_retries: u32,
}

impl HttpEthereumRpcClient {
    #[allow(dead_code)]
    pub fn new(connection_string: String, max_retries: Option<u32>) -> Self {
        Self {
            client: reqwest::Client::new(),
            connection_string,
            max_retries: max_retries.unwrap_or(5), // default to 5 max retries.
        }
    }

    async fn make_rpc_call<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        params: &T,
        timeout: Option<u64>,
    ) -> Result<R> {
        let connection_string = self.connection_string.clone();

        let raw_response = match timeout {
            Some(seconds) => {
                self.client
                    .post(connection_string)
                    .timeout(Duration::from_secs(seconds))
                    .json(params)
                    .send()
                    .await
            }
            None => {
                self.client
                    .post(connection_string)
                    .json(params)
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

    async fn make_retrying_rpc_call<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        params: &T,
        timeout: Option<u64>,
    ) -> Result<R> {
        let mut attempts = 0;
        loop {
            match self.make_rpc_call(params, timeout).await {
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

impl EthereumRpcClient for HttpEthereumRpcClient {
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
        timeout: Option<u64>,
    ) -> Result<BlockHeaderWithFullTransaction> {
        let params = RpcRequest {
            jsonrpc: "2.0",
            id: "0".to_string(),
            method: "eth_getBlockByNumber",
            params: (format!("0x{:x}", number), true),
        };

        self.make_retrying_rpc_call::<_, BlockHeaderWithFullTransaction>(&params, timeout)
            .await
    }
}

#[cfg(test)]
mod tests {
    use async_std::fs;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    use super::*;
    use std::{
        sync::mpsc::{sync_channel, SyncSender},
        thread,
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
        let client = HttpEthereumRpcClient::new("http://127.0.0.1:8091".to_owned(), Some(2));
        let block = client.get_full_block_by_number(21598014, None);

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

        let client = HttpEthereumRpcClient::new("http://127.0.0.1:8092".to_owned(), None);
        let block = client.get_full_block_by_number(21598014, None);

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

        let client = HttpEthereumRpcClient::new("http://127.0.0.1:8093".to_owned(), None);
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

        let client = HttpEthereumRpcClient::new("http://127.0.0.1:8094".to_owned(), None);
        let block = client.get_full_block_by_number(21598014, None);

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

        let client = HttpEthereumRpcClient::new("http://127.0.0.1:8095".to_owned(), None);
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
