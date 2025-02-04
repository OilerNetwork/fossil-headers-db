use std::{sync::Arc, thread};

use async_std::fs;
use axum::{routing::post, Json, Router};
use eyre::Result;
use fossil_headers_db::rpc::{BlockHeader, BlockTransaction};
use reqwest::StatusCode;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tokio::{
    net::TcpListener,
    runtime::Runtime,
    sync::{
        mpsc::{channel, Sender},
        RwLock,
    },
};

#[derive(Serialize, Deserialize, Debug)]
struct AnyJsonRpcRequest {
    jsonrpc: String,
    id: String,
    method: String,
    params: (String, bool), // hardcoded for testing, but should actually be dynamic.
}

#[derive(Serialize, Deserialize, Debug)]
struct JsonRpcError {
    code: i64,
    message: String,
    // Omitting the 'data' field here since this is for testing.
}

#[derive(Serialize, Deserialize, Debug)]
struct JsonRpcResponse<T> {
    jsonrpc: String,
    #[serde(serialize_with = "serialize_id", deserialize_with = "deserialize_id")]
    id: String,
    error: Option<JsonRpcError>,
    result: Option<T>,
}

fn serialize_id<S>(id: &str, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(id)
}

fn deserialize_id<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let id = i64::deserialize(deserializer)?;
    Ok(id.to_string())
}

async fn get_fixtures_for_tests() -> Vec<BlockHeader> {
    let mut block_fixtures = vec![];

    for i in 0..=5 {
        let json_string = fs::read_to_string(format!(
            "tests/fixtures/indexer/eth_getBlockByNumber_sepolia_{}.json",
            i
        ))
        .await
        .unwrap();

        let block = serde_json::from_str::<JsonRpcResponse<BlockHeader>>(&json_string).unwrap();
        block_fixtures.push(block.result.unwrap());
    }

    block_fixtures
}

/// The integration mock server here returns a hardcoded answer based on what was queried.
/// This is useful for our integration tests with the indexer.
pub async fn start_integration_mock_rpc_server(addr: String) -> Result<Sender<i64>> {
    let fixtures = get_fixtures_for_tests().await;
    let (tx, mut rx) = channel::<i64>(1);

    let current_finalized = Arc::new(RwLock::new(fixtures.len() - 1));

    let current_finalized_clone = current_finalized.clone();

    // Spawn a task to handle updating the finalized block number
    tokio::spawn(async move {
        while let Some(new_block) = rx.recv().await {
            let mut finalized = current_finalized_clone.write().await;
            *finalized = new_block as usize;
        }
    });

    let app = Router::new().route(
        "/",
        post(|Json(request): Json<AnyJsonRpcRequest>| async move {
            // Not going to handle checking against the jsonrpc version & id
            // only checking methods since its for testing.
            match request.method.as_str() {
                "eth_getBlockByNumber" => {
                    let data = if request.params.0 == "finalized" {
                        fixtures.last().unwrap().clone()
                    } else {
                        let block_number =
                            usize::from_str_radix(request.params.0.strip_prefix("0x").unwrap(), 16)
                                .unwrap();

                        fixtures[block_number].clone()
                    };

                    if request.params.1 {
                        return (
                            StatusCode::OK,
                            Json(JsonRpcResponse {
                                jsonrpc: "2.0".to_owned(),
                                id: request.id,
                                result: Some(data),
                                error: None,
                            }),
                        );
                    }
                    let data = BlockHeader {
                        transactions: data
                            .transactions
                            .iter()
                            .map(|tx| {
                                if let BlockTransaction::Full(tx) = tx {
                                    return BlockTransaction::Hash(tx.hash.clone());
                                }
                                tx.clone()
                            })
                            .collect(),
                        ..data
                    };

                    (
                        StatusCode::OK,
                        Json(JsonRpcResponse {
                            jsonrpc: "2.0".to_owned(),
                            id: request.id,
                            result: Some(data),
                            error: None,
                        }),
                    )
                }
                _ => (
                    StatusCode::NOT_FOUND,
                    Json(JsonRpcResponse {
                        jsonrpc: "2.0".to_owned(),
                        id: request.id,
                        error: Some(JsonRpcError {
                            code: -32601,
                            message: "Method not found".to_owned(),
                        }),
                        result: None,
                    }),
                ),
            }
        }),
    );

    // Should instead not use dotenvy for prod.
    let listener: TcpListener = TcpListener::bind(addr).await?;

    thread::spawn(move || {
        Runtime::new().unwrap().block_on(async move {
            axum::serve(listener, app.into_make_service())
                .await
                .unwrap();
        })
    });

    Ok(tx)
}
