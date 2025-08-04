//! # HTTP Router and Health Checks
//!
//! This module provides HTTP endpoints for service health monitoring and status checking.
//! It implements a simple web server that can be used for load balancer health checks,
//! monitoring systems, and basic service diagnostics.
//!
//! ## Features
//!
//! - **Health Check Endpoint**: Simple HTTP endpoint at `/` for service health status
//! - **Database Connectivity**: Health check includes database connection verification
//! - **Graceful Shutdown**: Responds to termination signals for clean service shutdown
//! - **Configurable Binding**: Configurable host and port binding via environment variables
//!
//! ## Endpoints
//!
//! - `GET /` - Health check endpoint that returns 200 OK if service and database are healthy
//!
//! ## Configuration
//!
//! The router is configured via environment variables:
//! - `ROUTER_ENDPOINT` - Host and port to bind to (default: "0.0.0.0:3000")
//!
//! ## Usage
//!
//! ```rust,no_run
//! use fossil_headers_db::router::initialize_router;
//! use std::sync::{Arc, atomic::AtomicBool};
//!
//! # async fn example() -> eyre::Result<()> {
//! let should_terminate = Arc::new(AtomicBool::new(false));
//! initialize_router(should_terminate).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Health Check Response
//!
//! The health check endpoint performs the following checks:
//! 1. Verifies database connection is active
//! 2. Returns HTTP 200 with "OK" if healthy
//! 3. Returns HTTP 500 if database is unreachable

use std::sync::atomic::{AtomicBool, Ordering};
use std::{sync::Arc, time::Duration};

use axum::{routing::get, Router};
use eyre::Result;

use reqwest::StatusCode;
use tracing::info;

use tokio::{net::TcpListener, time::sleep};

use crate::db::check_db_connection;

mod handlers;

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
    let listener: TcpListener = TcpListener::bind(
        dotenvy::var("ROUTER_ENDPOINT")
            .map_err(|e| eyre::eyre!("ROUTER_ENDPOINT must be set: {}", e))?,
    )
    .await?;

    info!(
        "->> LISTENING on {}\n",
        listener
            .local_addr()
            .map_err(|e| eyre::eyre!("Failed to get local address: {}", e))?
    );
    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal(should_terminate.clone()))
        .await?;

    Ok(())
}

async fn shutdown_signal(should_terminate: Arc<AtomicBool>) {
    while !should_terminate.load(Ordering::SeqCst) {
        sleep(Duration::from_secs(10)).await;
    }
    info!("Shutdown signal received, shutting down router");
}
