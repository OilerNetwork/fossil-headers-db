//! # Fossil Headers DB
//!
//! A Rust-based blockchain indexer for Ethereum that efficiently fetches, stores, and manages
//! block headers and transaction data. This library provides both legacy CLI tools and modern
//! indexing services for different blockchain networks.
//!
//! ## Architecture Overview
//!
//! The library is organized into several key modules:
//!
//! - [`commands`] - Legacy CLI indexing operations (update/fix modes)
//! - [`db`] - Database connection management and core data operations  
//! - [`errors`] - Domain-specific error types for blockchain operations
//! - [`indexer`] - Modern indexing services (batch and quick indexing)
//! - [`repositories`] - Database query abstractions and data access layer
//! - [`router`] - HTTP health check endpoints and routing
//! - [`rpc`] - Ethereum RPC client for fetching blockchain data
//! - [`types`] - Type-safe domain models (BlockNumber, BlockHash, etc.)
//! - [`utils`] - Common utility functions for hex conversion and validation
//!
//! ## Usage Examples
//!
//! ### Using the Modern Indexer Services
//! ```rust,no_run
//! use fossil_headers_db::indexer::lib::{start_indexing_services, IndexingConfig};
//! use std::sync::{Arc, atomic::AtomicBool};
//!
//! # async fn example() -> eyre::Result<()> {
//! let should_terminate = Arc::new(AtomicBool::new(false));
//! let config = IndexingConfig {
//!     db_conn_string: "postgresql://user:pass@localhost/db".to_string(),
//!     node_conn_string: "http://localhost:8545".to_string(),
//!     should_index_txs: false,
//!     max_retries: 3,
//!     poll_interval: 12,
//!     rpc_timeout: 30,
//!     rpc_max_retries: 5,
//!     index_batch_size: 1000,
//! };
//!
//! start_indexing_services(config, should_terminate).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ### Legacy CLI Operations  
//! ```rust,no_run
//! use fossil_headers_db::commands::{update_from, fill_gaps};
//! use fossil_headers_db::types::BlockNumber;
//! use std::sync::{Arc, atomic::AtomicBool};
//!
//! # async fn example() -> eyre::Result<()> {
//! let should_terminate = Arc::new(AtomicBool::new(false));
//!
//! // Update from latest stored block to current finalized block
//! update_from(None, None, 100, should_terminate.clone()).await?;
//!
//! // Fill gaps in stored block data
//! let start = BlockNumber::from_trusted(1000000);
//! let end = BlockNumber::from_trusted(2000000);
//! fill_gaps(Some(start), Some(end), should_terminate).await?;
//! # Ok(())
//! # }
//! ```

pub mod commands;
pub mod db;
pub mod errors;
pub mod indexer;
pub mod repositories;
pub mod router;
pub mod rpc;
pub mod types;
pub mod utils;
