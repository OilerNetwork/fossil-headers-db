use crate::utils::convert_hex_string_to_i64;
use eyre::{eyre, Context, Result};
use serde::{Deserialize, Serialize};
use std::{future::Future, time::Duration};
use tokio::time::sleep;
use tracing::{error, warn};

#[derive(Serialize, Deserialize, Debug)]
pub struct RpcResponse<T> {
    pub result: T,
}

#[derive(Serialize, Deserialize, Debug)]
struct RpcRequest<'a, T> {
    jsonrpc: &'a str,
    id: String,
    method: &'a str,
    params: T,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Transaction {
    pub hash: String,
    #[serde(rename = "blockNumber")]
    pub block_number: String,
    #[serde(rename = "transactionIndex")]
    pub transaction_index: String,
    pub value: String,
    #[serde(rename = "gasPrice")]
    pub gas_price: Option<String>,
    pub gas: String,
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(rename = "maxPriorityFeePerGas")]
    pub max_priority_fee_per_gas: Option<String>,
    #[serde(rename = "maxFeePerGas")]
    pub max_fee_per_gas: Option<String>,
    #[serde(rename = "chainId")]
    pub chain_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum BlockTransaction {
    Full(Box<Transaction>),
    Hash(String),
}

impl TryFrom<BlockTransaction> for Transaction {
    type Error = eyre::Error;

    fn try_from(value: BlockTransaction) -> std::result::Result<Self, Self::Error> {
        match value {
            BlockTransaction::Full(tx) => Ok(*tx),
            BlockTransaction::Hash(_) => Err(eyre!("Cannot convert hash into Transaction")),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BlockHeader {
    #[serde(rename = "gasLimit")]
    pub gas_limit: String,
    #[serde(rename = "gasUsed")]
    pub gas_used: String,
    #[serde(rename(deserialize = "baseFeePerGas"))]
    pub base_fee_per_gas: Option<String>,
    pub hash: String,
    pub nonce: Option<String>,
    pub number: String,
    #[serde(rename = "receiptsRoot")]
    pub receipts_root: Option<String>,
    #[serde(rename = "stateRoot")]
    pub state_root: Option<String>,
    #[serde(rename = "transactionsRoot")]
    pub transactions_root: Option<String>,
    pub transactions: Vec<BlockTransaction>,
    #[serde(rename = "parentHash")]
    pub parent_hash: Option<String>,
    pub miner: Option<String>,
    #[serde(rename = "logsBloom")]
    pub logs_bloom: Option<String>,
    #[serde(rename = "difficulty")]
    pub difficulty: Option<String>,
    #[serde(rename = "totalDifficulty")]
    pub total_difficulty: Option<String>,
    #[serde(rename = "sha3Uncles")]
    pub sha3_uncles: Option<String>,
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    #[serde(rename = "extraData")]
    pub extra_data: Option<String>,
    #[serde(rename = "mixHash")]
    pub mix_hash: Option<String>,
    #[serde(rename = "withdrawalsRoot")]
    pub withdrawals_root: Option<String>,
    #[serde(rename = "blobGasUsed")]
    pub blob_gas_used: Option<String>,
    #[serde(rename = "excessBlobGas")]
    pub excess_blob_gas: Option<String>,
    #[serde(rename = "parentBeaconBlockRoot")]
    pub parent_beacon_block_root: Option<String>,
    #[serde(rename = "requestsHash")]
    pub requests_hash: Option<String>,
}

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
    ) -> impl Future<Output = Result<BlockHeader>> + Send;
}

#[derive(Debug, Clone)]
pub struct EthereumJsonRpcClient {
    client: reqwest::Client,
    connection_string: String,
    max_retries: u32,
}

impl EthereumJsonRpcClient {
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
            .make_retrying_rpc_call::<_, BlockHeader>(&params, timeout)
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
    ) -> Result<BlockHeader> {
        let params = RpcRequest {
            jsonrpc: "2.0",
            id: "0".to_string(),
            method: "eth_getBlockByNumber",
            params: (format!("0x{:x}", number), include_tx),
        };

        self.make_retrying_rpc_call::<_, BlockHeader>(&params, timeout)
            .await
    }
}

// Attempts to convert a vector of BlockTransaction type into the Transaction type
pub fn try_convert_full_tx_vector(block_tx_vec: Vec<BlockTransaction>) -> Result<Vec<Transaction>> {
    let mut result_vec = vec![];

    for block_tx in block_tx_vec.into_iter() {
        let tx = block_tx.try_into()?;
        result_vec.push(tx);
    }

    Ok(result_vec)
}

// TODO: Make this work as expected
// pub async fn batch_get_full_block_by_number(
//     numbers: Vec<i64>,
//     timeout: Option<u64>,
// ) -> Result<Vec<BlockHeaderWithFullTransaction>> {
//     let mut params = Vec::new();
//     for number in numbers {
//         let num_str = number.to_string();
//         params.push(RpcRequest {
//             jsonrpc: "2.0",
//             id: num_str,
//             method: "eth_getBlockByNumber",
//             params: (format!("0x{:x}", number), true),
//         });
//     }
//     make_rpc_call::<_, Vec<BlockHeaderWithFullTransaction>>(&params, timeout).await
// }

/// BIG TODO:
/// Handle rpc errors correctly!
/// Currently error cases are not handled as well.
#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;
    use tokio::{
        fs,
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    async fn get_fixtures_for_tests() -> BlockHeader {
        let block_21598014_string =
            fs::read_to_string("tests/fixtures/eth_getBlockByNumber_21598014.json")
                .await
                .unwrap();

        let block_21598014_response =
            serde_json::from_str::<RpcResponse<BlockHeader>>(&block_21598014_string).unwrap();

        block_21598014_response.result
    }

    async fn get_fixture_for_pectra_upgrade_tests() -> (BlockHeader, BlockHeader) {
        let block_7836330 =
            fs::read_to_string("tests/fixtures/eth_getBlockByNumber_sepolia_7836330.json")
                .await
                .unwrap();

        let block_7836330_response =
            serde_json::from_str::<RpcResponse<BlockHeader>>(&block_7836330).unwrap();

        let block_7836331_string =
            fs::read_to_string("tests/fixtures/eth_getBlockByNumber_sepolia_7836331.json")
                .await
                .unwrap();

        let block_7836331_response =
            serde_json::from_str::<RpcResponse<BlockHeader>>(&block_7836331_string).unwrap();

        (block_7836330_response.result, block_7836331_response.result)
    }

    // Helper function to start a TCP server that returns predefined JSON-RPC responses
    // Taken from katana codebase.
    pub fn start_mock_rpc_server(addr: String, responses: Vec<Option<BlockHeader>>) {
        use tokio::runtime::Builder;

        thread::spawn(move || {
            // Using the current thread runtime to allow for a more linear test
            Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async move {
                    let listener = TcpListener::bind(addr).await.unwrap();

                    let mut counter = 0;

                    loop {
                        let (mut socket, _) = listener.accept().await.unwrap();

                        // Read the request
                        let mut buffer = [0; 512];
                        let _ = socket.read(&mut buffer).await.unwrap();

                        let buffer: Vec<u8> = buffer.into_iter().take_while(|&x| x != 0).collect();

                        let request_str = String::from_utf8_lossy(&buffer);

                        // Find the JSON body after the empty line in HTTP request
                        let body = if let Some(idx) = request_str.find("\r\n\r\n") {
                            &request_str[idx + 4..]
                        } else {
                            ""
                        };

                        // Get the response from the pre-determined list, and if the list is
                        // exhausted, return the last response.
                        let selected_response = if counter < responses.len() {
                            responses[counter].clone()
                        } else {
                            responses.last().unwrap().clone()
                        };

                        // Parse the JSON body and check the value
                        let response = if let Ok(json) =
                            serde_json::from_str::<RpcRequest<(String, bool)>>(body)
                        {
                            match selected_response {
                                Some(data) => {
                                    // Based on the params return full or hashed txs.
                                    // Here assuming the provided responses are always with full txs.
                                    let final_response = if json.params.1 {
                                        data
                                    } else {
                                        // If set to false, then remove the full txs and replace with the hashed ones.
                                        BlockHeader {
                                            transactions: data
                                                .transactions
                                                .iter()
                                                .map(|tx| match tx {
                                                    BlockTransaction::Full(full_tx) => {
                                                        BlockTransaction::Hash(full_tx.hash.clone())
                                                    }
                                                    other => other.clone(),
                                                })
                                                .collect(),
                                            ..data
                                        }
                                    };

                                    let final_response =
                                        serde_json::to_string(&final_response).unwrap();
                                    format!(
                                        r#"{{ "jsonrpc": "2.0", "id": 1, "result": {} }}"#,
                                        final_response
                                    )
                                }
                                None => "{}".to_string(),
                            }
                        } else {
                            // TODO: handle this and add tests.

                            // Return an error response if JSON parsing fails
                            r#"{"error": "Invalid JSON format"}"#.to_string()
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
    }

    #[tokio::test]
    async fn test_try_from_for_full_tx() {
        // Since this is a full tx, then try from should work
        let header = get_fixtures_for_tests().await;

        // Get one of the tx to test.
        let tx = header.transactions[0].clone();

        let expected_tx = tx.clone();

        let expected_tx = if let BlockTransaction::Full(full_tx) = expected_tx {
            full_tx
        } else {
            panic!("unexpected error due to tx type, should not happen.")
        };

        let actual_tx: Result<Transaction> = Transaction::try_from(tx.clone());

        let actual_tx = actual_tx.unwrap();

        assert_eq!(expected_tx.block_number, actual_tx.block_number);
        assert_eq!(expected_tx.chain_id, actual_tx.chain_id);
        assert_eq!(expected_tx.from, actual_tx.from);
        assert_eq!(expected_tx.to, actual_tx.to);
        assert_eq!(expected_tx.transaction_index, actual_tx.transaction_index);
        assert_eq!(expected_tx.gas, actual_tx.gas);
        assert_eq!(expected_tx.gas_price, actual_tx.gas_price);
        assert_eq!(expected_tx.max_fee_per_gas, actual_tx.max_fee_per_gas);
        assert_eq!(
            expected_tx.max_priority_fee_per_gas,
            actual_tx.max_priority_fee_per_gas
        );
        assert_eq!(expected_tx.value, actual_tx.value);
        assert_eq!(expected_tx.hash, actual_tx.hash);
    }

    #[tokio::test]
    async fn test_try_into_for_full_tx() {
        // Since this is a full tx, then try from should work
        let header = get_fixtures_for_tests().await;

        // Get one of the tx to test.
        let tx = header.transactions[0].clone();

        let expected_tx = if let BlockTransaction::Full(full_tx) = tx.clone() {
            full_tx
        } else {
            panic!("unexpected error due to tx type, should not happen.")
        };

        let actual_tx: Result<Transaction> = tx.try_into();
        assert!(actual_tx.is_ok());

        let actual_tx = actual_tx.unwrap();

        assert_eq!(expected_tx.block_number, actual_tx.block_number);
        assert_eq!(expected_tx.chain_id, actual_tx.chain_id);
        assert_eq!(expected_tx.from, actual_tx.from);
        assert_eq!(expected_tx.to, actual_tx.to);
        assert_eq!(expected_tx.transaction_index, actual_tx.transaction_index);
        assert_eq!(expected_tx.gas, actual_tx.gas);
        assert_eq!(expected_tx.gas_price, actual_tx.gas_price);
        assert_eq!(expected_tx.max_fee_per_gas, actual_tx.max_fee_per_gas);
        assert_eq!(
            expected_tx.max_priority_fee_per_gas,
            actual_tx.max_priority_fee_per_gas
        );
        assert_eq!(expected_tx.value, actual_tx.value);
        assert_eq!(expected_tx.hash, actual_tx.hash);
    }

    #[tokio::test]
    async fn test_try_convert_full_tx_vector() {
        let header = get_fixtures_for_tests().await;

        // Get one of the tx to test.
        let expected_tx = header.transactions[0].clone();
        let converted_tx = try_convert_full_tx_vector(vec![expected_tx.clone()]);

        let expected_tx: Transaction = expected_tx.try_into().unwrap();

        assert!(converted_tx.is_ok());
        let actual_tx = converted_tx.unwrap()[0].clone();

        assert_eq!(expected_tx.hash, actual_tx.hash);
        assert_eq!(expected_tx.block_number, actual_tx.block_number);
    }

    #[tokio::test]
    async fn test_try_convert_full_tx_vector_should_fail_for_hash_tx() {
        let txs = try_convert_full_tx_vector(vec![BlockTransaction::Hash("0x12345ab".to_string())]);
        assert!(txs.is_err());
    }

    #[tokio::test]
    async fn test_try_convert_full_tx_vector_should_be_empty_when_empty_vec_given() {
        let txs = try_convert_full_tx_vector(vec![]);
        assert!(txs.is_ok());
        assert!(txs.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_max_retries_should_affect_number_of_retries() {
        let rpc_response = get_fixtures_for_tests().await;
        start_mock_rpc_server(
            "127.0.0.1:8091".to_owned(),
            vec![None, None, None, Some(rpc_response), None], // introduce empty responses
        );

        // Set max retries to 2, which shouldn't have any success cases.
        let client = EthereumJsonRpcClient::new("http://127.0.0.1:8091".to_owned(), 2);

        let block = client.get_full_block_by_number(21598014, false, None);

        let block = block.await;
        assert!(block.is_err());
    }

    #[tokio::test]
    async fn test_get_full_block_by_number_should_retry_when_failed() {
        let rpc_response = get_fixtures_for_tests().await;
        start_mock_rpc_server(
            "127.0.0.1:8092".to_owned(),
            vec![None, Some(rpc_response.clone())], // introduce an empty response to induce failure
        );

        let client = EthereumJsonRpcClient::new("http://127.0.0.1:8092".to_owned(), 2);
        let block = client.get_full_block_by_number(21598014, false, None);

        let block = block.await.unwrap();
        assert_eq!(block.hash, rpc_response.hash);
        assert_eq!(block.number, rpc_response.number);
    }

    #[tokio::test]
    async fn test_get_latest_finalized_blocknumber_should_retry_when_failed() {
        let rpc_response = get_fixtures_for_tests().await;
        start_mock_rpc_server(
            "127.0.0.1:8093".to_owned(),
            vec![None, Some(rpc_response.clone())], // introduce an empty response to induce failure
        );

        let client = EthereumJsonRpcClient::new("http://127.0.0.1:8093".to_owned(), 2);
        let block_number = client.get_latest_finalized_blocknumber(None);

        let block_number = block_number.await.unwrap();
        assert_eq!(
            block_number,
            i64::from_str_radix(rpc_response.number.strip_prefix("0x").unwrap(), 16).unwrap()
        );
    }

    #[tokio::test]
    async fn test_get_full_block_by_number_without_tx() {
        let rpc_response = get_fixtures_for_tests().await;
        start_mock_rpc_server(
            "127.0.0.1:8094".to_owned(),
            vec![Some(rpc_response.clone())],
        );

        let client = EthereumJsonRpcClient::new("http://127.0.0.1:8094".to_owned(), 1);
        let block = client.get_full_block_by_number(21598014, false, None);

        let block = block.await.unwrap();
        assert_eq!(block.hash, rpc_response.hash);
        assert_eq!(block.number, rpc_response.number);
        for (idx, block_tx) in block.transactions.iter().enumerate() {
            if let BlockTransaction::Hash(hash) = block_tx {
                let fixture_tx: Transaction =
                    rpc_response.transactions[idx].clone().try_into().unwrap();
                assert_eq!(hash.to_owned(), fixture_tx.hash)
            } else {
                panic!("returned tx are full.")
            }
        }
    }

    #[tokio::test]
    async fn test_get_full_block_by_number_with_tx() {
        let rpc_response = get_fixtures_for_tests().await;
        start_mock_rpc_server(
            "127.0.0.1:8095".to_owned(),
            vec![Some(rpc_response.clone())],
        );

        let client = EthereumJsonRpcClient::new("http://127.0.0.1:8095".to_owned(), 1);
        let block = client.get_full_block_by_number(21598014, true, None);

        let block = block.await.unwrap();
        assert_eq!(block.hash, rpc_response.hash);
        assert_eq!(block.number, rpc_response.number);
        for (idx, block_tx) in block.transactions.iter().enumerate() {
            if let BlockTransaction::Full(tx) = block_tx {
                let fixture_tx: Transaction = rpc_response.clone().transactions[idx]
                    .clone()
                    .try_into()
                    .unwrap();

                assert_eq!(tx.hash, fixture_tx.hash);
                assert_eq!(tx.block_number, fixture_tx.block_number);
                assert_eq!(tx.from, fixture_tx.from);
                assert_eq!(tx.to, fixture_tx.to);
                assert_eq!(tx.transaction_index, fixture_tx.transaction_index);
                assert_eq!(tx.value, fixture_tx.value);
            } else {
                panic!("returned tx are hashed.")
            }
        }
    }

    #[tokio::test]
    async fn test_get_latest_finalized_blocknumber() {
        let rpc_response = get_fixtures_for_tests().await;
        start_mock_rpc_server(
            "127.0.0.1:8096".to_owned(),
            vec![Some(rpc_response.clone())],
        );

        let client = EthereumJsonRpcClient::new("http://127.0.0.1:8096".to_owned(), 1);
        let block_number = client.get_latest_finalized_blocknumber(None);

        let block_number = block_number.await.unwrap();
        assert_eq!(
            block_number,
            i64::from_str_radix(rpc_response.number.strip_prefix("0x").unwrap(), 16).unwrap()
        );
    }

    #[tokio::test]
    async fn test_get_full_block_by_number_with_pectra_upgrade() {
        let (before_pectra_block, after_pectra_block) =
            get_fixture_for_pectra_upgrade_tests().await;
        start_mock_rpc_server(
            "127.0.0.1:8097".to_owned(),
            vec![
                Some(before_pectra_block.clone()),
                Some(after_pectra_block.clone()),
            ],
        );
        let client = EthereumJsonRpcClient::new("http://127.0.0.1:8097".to_owned(), 1);

        // Check before pectra upgrade block.
        let block = client.get_full_block_by_number(7836330, true, None);
        let block = block.await.unwrap();
        assert_eq!(block.hash, before_pectra_block.hash);
        assert_eq!(block.number, before_pectra_block.number);
        assert!(block.requests_hash.is_none());

        // Check after pectra upgrade block.
        let block = client.get_full_block_by_number(7836331, true, None);

        let block = block.await.unwrap();
        assert_eq!(block.hash, after_pectra_block.hash);
        assert_eq!(block.number, after_pectra_block.number);
        assert_eq!(block.requests_hash, after_pectra_block.requests_hash);
    }
    // TODO: Add more tests for the failure cases.
}
