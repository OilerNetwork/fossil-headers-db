use crate::types::{BlockHeaderWithEmptyTransaction, BlockHeaderWithFullTransaction};
use crate::utils::convert_hex_string_to_i64;
use eyre::{Context, Result};
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

static CLIENT: Lazy<Client> = Lazy::new(Client::new);
static NODE_CONNECTION_STRING: Lazy<String> = Lazy::new(|| {
    dotenvy::var("NODE_CONNECTION_STRING").expect("NODE_CONNECTION_STRING must be set")
});

#[derive(Deserialize, Debug)]
pub struct RpcResponse<T> {
    pub result: T,
}

#[derive(Serialize)]
struct RpcRequest<'a, T> {
    jsonrpc: &'a str,
    id: &'a str,
    method: &'a str,
    params: T,
}

pub async fn get_latest_finalized_blocknumber(timeout: Option<u64>) -> Result<i64> {
    // TODO: Id should be different on every request, this is how request are identified by us and by the node.
    let params = RpcRequest {
        jsonrpc: "2.0",
        id: "0",
        method: "eth_getBlockByNumber",
        params: ("finalized", false),
    };

    match make_rpc_call::<_, BlockHeaderWithEmptyTransaction>(&params, timeout)
        .await
        .context("Failed to get latest block number")
    {
        Ok(blockheader) => Ok(convert_hex_string_to_i64(&blockheader.number)),
        Err(e) => Err(e),
    }
}

pub async fn get_full_block_by_number(
    number: i64,
    timeout: Option<u64>,
) -> Result<BlockHeaderWithFullTransaction> {
    let params = RpcRequest {
        jsonrpc: "2.0",
        id: "0",
        method: "eth_getBlockByNumber",
        params: (format!("0x{:x}", number), true),
    };

    make_rpc_call::<_, BlockHeaderWithFullTransaction>(&params, timeout).await
}

async fn make_rpc_call<T: Serialize, R: for<'de> Deserialize<'de>>(
    params: &T,
    timeout: Option<u64>,
) -> Result<R> {
    let raw_response = match timeout {
        Some(seconds) => {
            CLIENT
                .post(NODE_CONNECTION_STRING.as_str())
                .timeout(Duration::from_secs(seconds))
                .json(params)
                .send()
                .await
        }
        None => {
            CLIENT
                .post(NODE_CONNECTION_STRING.as_str())
                .json(params)
                .send()
                .await
        }
    };

    let raw_response = match raw_response {
        Ok(response) => response,
        Err(e) => {
            eprintln!("HTTP request error: {:?}", e);
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
                    eprintln!(
                        "Deserialization error: {:?}\nResponse snippet: {:?}",
                        e,
                        text // Log the entire response
                    );
                    Err(e.into())
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to read response body: {:?}", e);
            Err(e.into())
        }
    }
}
