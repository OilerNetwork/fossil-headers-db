use crate::router::handlers::{get_mmr_latest, get_mmr_proof};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{sync::Arc, time::Duration};

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use derive_more::{Display, From};
use log::info;
use serde::Serialize;
use tokio::{net::TcpListener, time::sleep};

mod handlers;

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, Display, From)]
pub enum Error {
    #[from]
    StdIOError(std::io::Error),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        // How we want errors responses to be serialized
        #[derive(Serialize)]
        struct ErrorResponse {
            message: String,
        }

        let (status, message) = match self {
            Error::StdIOError(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Something went wrong".to_owned(),
            ),
        };

        let body = Json(serde_json::json!({
            "error": message,
        }));

        (status, body).into_response()
    }
}

pub async fn initialize_router(should_terminate: Arc<AtomicBool>) -> Result<()> {
    let app = Router::new()
        .route("/", get(|| async { "Healthy" }))
        .route("/mmr", get(get_mmr_latest))
        .route("/mmr/:blocknumber", get(get_mmr_proof));

    let listener: TcpListener =
        TcpListener::bind(dotenvy::var("ROUTER_ENDPOINT").expect("ROUTER_ENDPOINT must be set"))
            .await?;

    info!("->> LISTENING on {}\n", listener.local_addr().unwrap());
    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal(should_terminate.clone()))
        .await?;

    Ok(())
}

async fn shutdown_signal(should_terminate: Arc<AtomicBool>) {
    while !should_terminate.load(Ordering::SeqCst) {
        sleep(Duration::from_secs(30)).await;
    }
    info!("Shutdown signal received, shutting down router");
}
