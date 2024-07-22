use derive_more::{Display, From};
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::types::{BlockHeaderWithEmptyTransaction, BlockHeaderWithFullTransaction};

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, Display, From)]
pub enum Error {
    Generic(String),

    #[from]
    ReqwestError(reqwest::Error),
}

static CLIENT: Lazy<Client> = Lazy::new(Client::new);
static NODE_CONNECTION_STRING: Lazy<String> = Lazy::new(|| {
    dotenvy::var("NODE_CONNECTION_STRING").expect("NODE_CONNECTION_STRING must be set")
});

#[derive(Deserialize, Debug)]
pub struct RpcResponse<T> {
    pub result: T,
    // pub id: String,
    // pub jsonrpc: String,
}

#[derive(Serialize)]
struct RpcRequest<'a, T> {
    jsonrpc: &'a str,
    id: &'a str,
    method: &'a str,
    params: T,
}

pub async fn get_latest_blocknumber(timeout: Option<u64>) -> Result<String> {
    let params = RpcRequest {
        jsonrpc: "2.0",
        id: "0",
        method: "eth_getBlockByNumber",
        params: vec!["finalized", "false"],
    };

    match make_rpc_call::<_, BlockHeaderWithEmptyTransaction>(&params, timeout).await {
        Ok(blockheader) => Ok(blockheader.number),
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
        params: vec![format!("0x{:x}", number), true.to_string()],
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
    let response = raw_response?.json::<RpcResponse<R>>().await?;

    Ok(response.result)
}
