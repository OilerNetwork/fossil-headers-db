//! # Fossil Headers DB
//!
//! A Rust-based blockchain indexer for Ethereum that efficiently fetches, stores, and manages
//! block headers and transaction data. This library provides both legacy CLI tools and modern
//! indexing services for different blockchain networks.
//!
//! ## Architecture Overview
//!
//! The library is organized into several key modules with clear boundaries:
//!
//! ### Public API Modules
//! - [`commands`] - Legacy CLI indexing operations (update/fix modes)
//! - [`errors`] - Domain-specific error types for blockchain operations
//! - [`indexer`] - Modern indexing services (batch and quick indexing)
//! - [`types`] - Type-safe domain models (`BlockNumber`, `BlockHash`, etc.)
//!
//! ### Facade Modules (Simplified Interfaces)
//! - [`database`] - Database operations with hidden connection management
//! - [`blockchain`] - RPC operations with abstracted client complexity
//! - [`health`] - Health check endpoints and monitoring
//!
//! ### Internal Modules (Implementation Details)
//! - `db` - Database connection management and core data operations  
//! - `repositories` - Database query abstractions and data access layer
//! - `router` - HTTP health check endpoints and routing
//! - `rpc` - Ethereum RPC client for fetching blockchain data
//! - `utils` - Common utility functions for hex conversion and validation
//!
//! ## Module Interaction Patterns
//!
//! The library follows a layered architecture:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                   Public API Layer                         │
//! │  commands, indexer, types, errors                          │
//! └─────────────────────────────────────────────────────────────┘
//!                              ↓
//! ┌─────────────────────────────────────────────────────────────┐
//! │                   Facade Layer                              │
//! │  database, blockchain, health                               │
//! └─────────────────────────────────────────────────────────────┘
//!                              ↓
//! ┌─────────────────────────────────────────────────────────────┐
//! │                Implementation Layer                         │
//! │  db, repositories, rpc, router, utils                      │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! - **No circular dependencies**: Lower layers never import from higher layers
//! - **Clear boundaries**: Public API only uses facade modules
//! - **Hidden complexity**: Implementation details are not exposed
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
//!     start_block_offset: None,
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

// Core public modules
pub mod commands;
pub mod errors;
pub mod indexer;
pub mod types;

// Internal modules (not part of public API)
pub mod db;
pub mod repositories;
pub mod router;
pub mod rpc;
mod utils;

// Test-only modules
#[cfg(test)]
mod error_tests;
#[cfg(test)]
mod mocks;
#[cfg(test)]
mod property_tests;
#[cfg(test)]
mod test_utils;

// Public re-exports for simplified API
pub use errors::{BlockchainError, Result};
pub use types::{Address, BlockHash, BlockNumber, TransactionHash};

// Facade modules for complex subsystems
pub mod database {
    //! Database operations facade
    //!
    //! This module provides a simplified interface to database operations,
    //! hiding the internal complexity of connection management and repositories.

    pub use crate::db::{
        check_db_connection, find_first_gap, find_null_data, get_db_pool,
        get_last_stored_blocknumber, write_blockheader, DB_MAX_CONNECTIONS,
    };
}

pub mod blockchain {
    //! Blockchain RPC operations facade
    //!
    //! This module provides a simplified interface to Ethereum RPC operations,
    //! abstracting the underlying RPC client complexity.

    pub use crate::rpc::{BlockHeader, BlockHeaderWithFullTransaction, EthereumRpcProvider};
}

pub mod health {
    //! Health check and monitoring facade
    //!
    //! This module provides health check endpoints and system monitoring capabilities.

    pub use crate::router::initialize_router;
}

// Internal utilities (hidden from public API)
#[doc(hidden)]
pub mod internal {
    pub use crate::utils::*;
}
