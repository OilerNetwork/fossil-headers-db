use std::sync::atomic::{AtomicBool, Ordering};
use std::{sync::Arc, time::Duration};

use axum::{routing::get, Router};
use eyre::Result;

use reqwest::StatusCode;
use tracing::info;

use tokio::{net::TcpListener, time::sleep};

use crate::db::check_db_connection;

mod handlers;

/// Initializes and starts the HTTP router for the API server.
/// 
/// # Arguments
/// * `should_terminate` - Flag to signal when the server should shut down
/// 
/// # Returns
/// * `Result<()>` - Ok if the server starts and runs successfully
/// 
/// This function:
/// 1. Creates the router with defined routes
/// 2. Binds to the configured address
/// 3. Serves requests until termination is signaled
pub async fn initialize_router(should_terminate: Arc<AtomicBool>) -> Result<()> {
    let app = Router::new().route(
        "/",
        get(|| async {
            // Check db connection here in health check.
            let db_check = check_db_connection().await;
            if db_check.is_err() {
                return (StatusCode::INTERNAL_SERVER_ERROR, "Db connection failed");
            }
            (StatusCode::OK, "Healthy")
        }),
    );

    // Should instead not use dotenvy for prod.
    let listener: TcpListener =
        TcpListener::bind(dotenvy::var("ROUTER_ENDPOINT").expect("ROUTER_ENDPOINT must be set"))
            .await?;

    info!("->> LISTENING on {}\n", listener.local_addr().unwrap());
    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal(should_terminate.clone()))
        .await?;

    Ok(())
}

/// Waits for the shutdown signal and then shuts down the router.
/// 
/// # Arguments
/// * `should_terminate` - Flag to signal when the server should shut down
async fn shutdown_signal(should_terminate: Arc<AtomicBool>) {
    while !should_terminate.load(Ordering::SeqCst) {
        sleep(Duration::from_secs(10)).await;
    }
    info!("Shutdown signal received, shutting down router");
}
