//! # Database Repository Layer
//!
//! This module provides a data access layer that abstracts database operations into
//! domain-specific repository patterns. It contains specialized repositories for
//! different types of blockchain data and metadata management.
//!
//! ## Architecture
//!
//! The repository pattern is used to separate business logic from data access logic,
//! providing a clean interface for database operations while maintaining flexibility
//! for testing and different storage backends.
//!
//! ## Repositories
//!
//! - [`block_header`] - Repository for block header and transaction data operations
//! - [`index_metadata`] - Repository for indexing progress and system metadata
//!
//! ## Usage Examples
//!
//! ### Working with Block Headers
//! ```rust,no_run
//! use fossil_headers_db::repositories::block_header::insert_block_header_query;
//! use fossil_headers_db::db::DbConnection;
//!
//! # async fn example() -> eyre::Result<()> {
//! let db_conn_string = "postgresql://user:pass@localhost/db".to_string();
//! let db = DbConnection::new(db_conn_string).await?;
//!
//! // Insert block header data
//! // insert_block_header_query(db, block_data).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ### Managing Index Metadata
//! ```rust,no_run
//! use fossil_headers_db::repositories::index_metadata::get_index_metadata;
//! use fossil_headers_db::db::DbConnection;
//!
//! # async fn example() -> eyre::Result<()> {
//! let db_conn_string = "postgresql://user:pass@localhost/db".to_string();
//! let db = DbConnection::new(db_conn_string).await?;
//!
//! // Check current indexing status
//! let metadata = get_index_metadata(db).await?;
//! if let Some(meta) = metadata {
//!     println!("Backfilling status: {}", meta.is_backfilling);
//! }
//! # Ok(())
//! # }
//! ```

pub mod block_header;
pub mod index_metadata;
