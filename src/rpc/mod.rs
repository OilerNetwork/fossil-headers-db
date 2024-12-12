use crate::types::{
    type_utils::convert_hex_string_to_i64, BlockHeaderWithEmptyTransaction,
    BlockHeaderWithFullTransaction,
};
use eyre::{Context, Result};
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, warn};

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
        id: "0",
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
