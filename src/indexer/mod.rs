//! # Modern Indexing Services
//!
//! This module provides the modern, service-oriented approach to blockchain indexing.
//! It implements two complementary indexing strategies: batch processing for historical
//! data and quick indexing for real-time updates.
//!
//! ## Architecture
//!
//! The indexing system is designed around two core services:
//!
//! - **Batch Indexer** ([`batch_service`]): Efficiently processes large ranges of historical blocks
//! - **Quick Indexer** ([`quick_service`]): Provides real-time indexing of new blocks as they're finalized
//!
//! ## Key Features
//!
//! - **Dual-Mode Operation**: Simultaneous batch and real-time indexing
//! - **Graceful Coordination**: Services coordinate to avoid conflicts and gaps
//! - **Resource Management**: Configurable concurrency and batch sizes
//! - **Progress Tracking**: Persistent metadata tracking indexing progress
//! - **Fault Tolerance**: Automatic recovery from failures and network issues
//!
//! ## Service Coordination
//!
//! The services work together through shared metadata:
//! - Batch indexer processes historical blocks up to a target
//! - Quick indexer handles new blocks from the network tip
//! - Metadata tracks progress and prevents overlap
//! - Automatic handoff when batch indexing reaches real-time
//!
//! ## Usage
//!
//! The main entry point is [`lib::start_indexing_services`] which coordinates both services:
//!
//! ```rust,no_run
//! use fossil_headers_db::indexer::lib::{start_indexing_services, IndexingConfig};
//! use std::sync::{Arc, atomic::AtomicBool};
//!
//! # async fn example() -> eyre::Result<()> {
//! let should_terminate = Arc::new(AtomicBool::new(false));
//! let config = IndexingConfig {
//!     db_conn_string: "postgresql://user:pass@localhost/db".to_string(),
//!     node_conn_string: "http://localhost:8545".to_string(),
//!     should_index_txs: false, // Only index block headers
//!     max_retries: 3,
//!     poll_interval: 12,       // Poll every 12 seconds for new blocks
//!     rpc_timeout: 30,
//!     rpc_max_retries: 5,
//!     index_batch_size: 1000,  // Process 1000 blocks per batch
//!     start_block_offset: None,
//! };
//!
//! start_indexing_services(config, should_terminate).await?;
//! # Ok(())
//! # }
//! ```

pub mod batch_service;
pub mod lib;
pub mod quick_service;

#[cfg(test)]
pub mod test_utils;
