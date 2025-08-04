//! # Database Operations
//!
//! This module provides database connectivity, connection pooling, and core data operations
//! for storing and retrieving Ethereum blockchain data. It manages the persistent storage
//! of block headers, transactions, and indexing metadata.
//!
//! ## Key Features
//!
//! - **Connection Pooling**: Managed `PostgreSQL` connection pool with configurable limits
//! - **Retry Logic**: Built-in retry mechanisms with exponential backoff for resilient operations
//! - **Gap Detection**: Algorithms to identify missing blocks in the stored data
//! - **Batch Operations**: Efficient bulk insertion of blockchain data
//! - **Migration Support**: Database schema management and versioning
//!
//! ## Database Schema
//!
//! The module works with several key tables:
//! - `blockheaders` - Stores Ethereum block header information
//! - `transactions` - Stores transaction data (optional based on configuration)
//! - `index_metadata` - Tracks indexing progress and system state
//!
//! ## Connection Management
//!
//! The database connection is managed through a singleton pattern using [`get_db_pool()`]
//! which creates and maintains a connection pool. The pool is configured with:
//! - Maximum connections: [`DB_MAX_CONNECTIONS`] (50 by default)
//! - Slow query logging: Queries taking >120 seconds are logged
//! - Automatic reconnection and error handling
//!
//! ## Usage Examples
//!
//! ### Basic Database Operations
//! ```rust,no_run
//! use fossil_headers_db::db::{get_db_pool, get_last_stored_blocknumber};
//!
//! # async fn example() -> eyre::Result<()> {
//! // Get database connection pool
//! let pool = get_db_pool().await?;
//!
//! // Get the latest stored block number
//! let latest_block = get_last_stored_blocknumber().await?;
//! println!("Latest stored block: {}", latest_block.value());
//! # Ok(())
//! # }
//! ```
//!
//! ### Gap Detection
//! ```rust,no_run
//! use fossil_headers_db::db::find_first_gap;
//! use fossil_headers_db::types::BlockNumber;
//!
//! # async fn example() -> eyre::Result<()> {
//! let start = BlockNumber::from_trusted(1000000);
//! let end = BlockNumber::from_trusted(2000000);
//!
//! if let Some(gap_block) = find_first_gap(start, end).await? {
//!     println!("Found gap at block: {}", gap_block.value());
//! }
//! # Ok(())
//! # }
//! ```

use crate::errors::{BlockchainError, Result};
use crate::rpc::BlockHeaderWithFullTransaction;
use crate::types::BlockNumber;
use crate::utils::convert_hex_string_to_i64;
use futures::FutureExt;
use sqlx::postgres::PgConnectOptions;
use sqlx::query_builder::Separated;
use sqlx::ConnectOptions;
use sqlx::QueryBuilder;
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::OnceCell;
use tokio::time::sleep;
use tracing::{error, info, warn};

static DB_POOL: OnceCell<Arc<Pool<Postgres>>> = OnceCell::const_new();
pub const DB_MAX_CONNECTIONS: u32 = 50;

/// Gets or creates the shared database connection pool.
///
/// This function implements a singleton pattern for database connection management.
/// On first call, it creates a new connection pool with the configured settings.
/// Subsequent calls return the existing pool, ensuring efficient resource usage.
///
/// # Connection Configuration
///
/// The connection pool is configured with:
/// - Maximum connections: [`DB_MAX_CONNECTIONS`] (50 by default)
/// - Slow query logging: Queries taking >120 seconds are logged at DEBUG level
/// - Connection string from `DB_CONNECTION_STRING` environment variable
///
/// # Returns
///
/// Returns a `Result<Arc<Pool<Postgres>>>` containing the shared connection pool.
///
/// # Errors
///
/// This function will return an error if:
/// - The `DB_CONNECTION_STRING` environment variable is not set or invalid
/// - The database server is unreachable
/// - Authentication fails
/// - The connection pool cannot be initialized
///
/// # Examples
///
/// ```rust,no_run
/// use fossil_headers_db::db::get_db_pool;
///
/// # async fn example() -> eyre::Result<()> {
/// let pool = get_db_pool().await?;
/// // Use the pool for database operations
/// # Ok(())
/// # }
/// ```
pub async fn get_db_pool() -> Result<Arc<Pool<Postgres>>> {
    if let Some(pool) = DB_POOL.get() {
        Ok(pool.clone())
    } else {
        let mut conn_options: PgConnectOptions = dotenvy::var("DB_CONNECTION_STRING")
            .map_err(|_| {
                BlockchainError::configuration(
                    "DB_CONNECTION_STRING",
                    "Environment variable must be set",
                )
            })?
            .parse()
            .map_err(|e| {
                BlockchainError::configuration(
                    "DB_CONNECTION_STRING",
                    format!("Invalid connection string: {e}"),
                )
            })?;
        conn_options = conn_options
            .log_slow_statements(tracing::log::LevelFilter::Debug, Duration::new(120, 0));

        let pool = PgPoolOptions::new()
            .max_connections(DB_MAX_CONNECTIONS)
            .connect_with(conn_options)
            .await
            .map_err(|e| {
                BlockchainError::database_connection(format!(
                    "Failed to create connection pool: {e}"
                ))
            })?;
        let arc_pool = Arc::new(pool);
        match DB_POOL.set(arc_pool.clone()) {
            Ok(()) => Ok(arc_pool),
            Err(_) => DB_POOL
                .get()
                .ok_or_else(|| {
                    BlockchainError::database_connection(
                        "Failed to get database pool after initialization",
                    )
                })
                .map(Clone::clone),
        }
    }
}

/// Checks if the database connection is healthy and responsive.
///
/// This function performs a simple connectivity test by executing a basic query
/// against the database. It's commonly used for health checks and monitoring.
///
/// # Returns
///
/// Returns `Result<()>` which is `Ok(())` if the connection is healthy.
///
/// # Errors
///
/// This function will return an error if:
/// - The database connection pool cannot be obtained
/// - The database server is unreachable
/// - The test query fails to execute
/// - Authentication or permissions issues occur
///
/// # Examples
///
/// ```rust,no_run
/// use fossil_headers_db::db::check_db_connection;
///
/// # async fn example() -> eyre::Result<()> {
/// match check_db_connection().await {
///     Ok(()) => println!("Database is healthy"),
///     Err(e) => println!("Database connection failed: {}", e),
/// }
/// # Ok(())
/// # }
/// ```
pub async fn check_db_connection() -> Result<()> {
    let pool = get_db_pool().await.map_err(|e| {
        BlockchainError::database_connection(format!("Failed to get database pool: {e}"))
    })?;
    sqlx::query("SELECT 1")
        .execute(&*pool)
        .await
        .map_err(|e| BlockchainError::database_query(format!("Connection check failed: {e}")))?;
    Ok(())
}

/// Retrieves the block number of the latest stored block header.
///
/// This function queries the database to find the highest block number that has been
/// successfully stored in the blockheaders table. This is commonly used to determine
/// where to resume indexing operations.
///
/// # Returns
///
/// Returns a `Result<BlockNumber>` containing:
/// - The highest block number if blocks exist in the database
/// - Block number -1 if the table is empty (no blocks stored yet)
///
/// # Errors
///
/// This function will return an error if:
/// - The database connection fails
/// - The query cannot be executed
/// - The result cannot be parsed as a valid block number
///
/// # Examples
///
/// ```rust,no_run
/// use fossil_headers_db::db::get_last_stored_blocknumber;
///
/// # async fn example() -> eyre::Result<()> {
/// let latest_block = get_last_stored_blocknumber().await?;
/// if latest_block.value() == -1 {
///     println!("No blocks stored yet");
/// } else {
///     println!("Latest stored block: {}", latest_block.value());
/// }
/// # Ok(())
/// # }
/// ```
pub async fn get_last_stored_blocknumber() -> Result<BlockNumber> {
    const MAX_RETRIES: u32 = 3;

    retry_async(
        || {
            async {
                let pool = get_db_pool().await?;
                let result: (i64,) =
                    sqlx::query_as("SELECT COALESCE(MAX(number), -1) FROM blockheaders")
                        .fetch_one(&*pool)
                        .await
                        .map_err(|e| {
                            BlockchainError::database_query(format!(
                                "Failed to get last stored block number: {e}"
                            ))
                        })?;

                Ok(BlockNumber::from_trusted(result.0))
            }
            .boxed()
        },
        MAX_RETRIES,
    )
    .await
}

/// Finds the first missing block number within a specified range.
///
/// This function performs gap detection by checking for missing block numbers in the
/// specified range (inclusive). It uses a recursive CTE to generate a series of expected
/// block numbers and identifies the first one that's missing from the database.
///
/// # Arguments
///
/// * `start` - The starting block number (inclusive)
/// * `end` - The ending block number (inclusive)
///
/// # Returns
///
/// Returns a `Result<Option<BlockNumber>>` where:
/// - `Some(BlockNumber)` contains the first missing block number if a gap exists
/// - `None` if no gaps are found in the specified range
///
/// # Errors
///
/// This function will return an error if:
/// - The database connection fails
/// - The gap detection query cannot be executed
/// - Invalid block number range is provided (start > end)
///
/// # Examples
///
/// ```rust,no_run
/// use fossil_headers_db::db::find_first_gap;
/// use fossil_headers_db::types::BlockNumber;
///
/// # async fn example() -> eyre::Result<()> {
/// let start = BlockNumber::from_trusted(1000000);
/// let end = BlockNumber::from_trusted(1001000);
///
/// match find_first_gap(start, end).await? {
///     Some(gap_block) => println!("Found gap at block: {}", gap_block.value()),
///     None => println!("No gaps found in range"),
/// }
/// # Ok(())
/// # }
/// ```
pub async fn find_first_gap(start: BlockNumber, end: BlockNumber) -> Result<Option<BlockNumber>> {
    let pool = get_db_pool().await?;
    let result: Option<(i64,)> = sqlx::query_as(
        r"
        WITH RECURSIVE number_series(n) AS (
            SELECT $1
            UNION ALL
            SELECT n + 1 FROM number_series WHERE n < $2
        )
        SELECT n FROM number_series
        WHERE n NOT IN (SELECT number FROM blockheaders WHERE number BETWEEN $1 AND $2)
        LIMIT 1
        ",
    )
    .bind(start.value())
    .bind(end.value())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| BlockchainError::database_query(format!("Failed to find first gap: {e}")))?;

    Ok(result.map(|r| BlockNumber::from_trusted(r.0)))
}

/**
 * This finds the null data by querying the blockheaders table.
 * Included fields is based on the reth primitives defined in <https://reth.rs/docs/reth_primitives/struct.Header.html>
 */
pub async fn find_null_data(start: BlockNumber, end: BlockNumber) -> Result<Vec<BlockNumber>> {
    let pool = get_db_pool().await?;
    let result: Vec<(i64,)> = sqlx::query_as(
        r"
        SELECT number FROM blockheaders
            WHERE (block_hash IS NULL
            OR gas_limit IS NULL
            OR gas_used IS NULL
            OR nonce IS NULL
            OR transaction_root IS NULL
            OR receipts_root IS NULL
            OR state_root IS NULL
            OR parent_hash IS NULL
            OR logs_bloom IS NULL
            OR difficulty IS NULL
            OR totalDifficulty IS NULL
            OR timestamp IS NULL
            OR extra_data IS NULL
            OR mix_hash IS NULL)
            AND number BETWEEN $1 AND $2
            ORDER BY number ASC
        ",
    )
    .bind(start.value())
    .bind(end.value())
    .fetch_all(&*pool)
    .await
    .map_err(|e| BlockchainError::database_query(format!("Failed to find any null data: {e}")))?;

    Ok(result
        .iter()
        .map(|row| BlockNumber::from_trusted(row.0))
        .collect())
}

async fn insert_block_header(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    block_header: &BlockHeaderWithFullTransaction,
) -> Result<bool> {
    let result = sqlx::query(
        r"
        INSERT INTO blockheaders (
            block_hash, number, gas_limit, gas_used, base_fee_per_gas,
            nonce, transaction_root, receipts_root, state_root,
            parent_hash, miner, logs_bloom, difficulty, totalDifficulty,
            sha3_uncles, timestamp, extra_data, mix_hash, withdrawals_root,
            blob_gas_used, excess_blob_gas, parent_beacon_block_root
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22)
        ON CONFLICT (number)
        DO UPDATE SET
            block_hash = EXCLUDED.block_hash,
            gas_limit = EXCLUDED.gas_limit,
            gas_used = EXCLUDED.gas_used,
            base_fee_per_gas = EXCLUDED.base_fee_per_gas,
            nonce = EXCLUDED.nonce,
            transaction_root = EXCLUDED.transaction_root,
            receipts_root = EXCLUDED.receipts_root,
            state_root = EXCLUDED.state_root,
            parent_hash = EXCLUDED.parent_hash,
            miner = EXCLUDED.miner,
            logs_bloom = EXCLUDED.logs_bloom,
            difficulty = EXCLUDED.difficulty,
            totalDifficulty = EXCLUDED.totalDifficulty,
            sha3_uncles = EXCLUDED.sha3_uncles,
            timestamp = EXCLUDED.timestamp,
            extra_data = EXCLUDED.extra_data,
            mix_hash = EXCLUDED.mix_hash,
            withdrawals_root = EXCLUDED.withdrawals_root,
            blob_gas_used = EXCLUDED.blob_gas_used,
            excess_blob_gas = EXCLUDED.excess_blob_gas,
            parent_beacon_block_root = EXCLUDED.parent_beacon_block_root;
        ",
    )
    .bind(&block_header.hash)
    .bind(convert_hex_string_to_i64(&block_header.number)?)
    .bind(convert_hex_string_to_i64(&block_header.gas_limit)?)
    .bind(convert_hex_string_to_i64(&block_header.gas_used)?)
    .bind(&block_header.base_fee_per_gas)
    .bind(&block_header.nonce)
    .bind(&block_header.transactions_root)
    .bind(&block_header.receipts_root)
    .bind(&block_header.state_root)
    .bind(&block_header.parent_hash)
    .bind(&block_header.miner)
    .bind(&block_header.logs_bloom)
    .bind(&block_header.difficulty)
    .bind(&block_header.total_difficulty)
    .bind(&block_header.sha3_uncles)
    .bind(convert_hex_string_to_i64(&block_header.timestamp)?)
    .bind(&block_header.extra_data)
    .bind(&block_header.mix_hash)
    .bind(&block_header.withdrawals_root)
    .bind(&block_header.blob_gas_used)
    .bind(&block_header.excess_blob_gas)
    .bind(&block_header.parent_beacon_block_root)
    .execute(&mut **tx)
    .await
    .map_err(|e| {
        error!("Detailed error: {:?}", e);
        BlockchainError::database_query(format!("Failed to insert block header for block number {}: {}", block_header.number, e))
    })?;

    if result.rows_affected() == 0 {
        warn!(
            "Block already exists: -- block number: {}, block hash: {}",
            block_header.number, block_header.hash
        );
        return Ok(false);
    }
    info!(
        "Inserted block number: {}, block hash: {}",
        block_header.number, block_header.hash
    );
    Ok(true)
}

async fn insert_transactions(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    block_header: &BlockHeaderWithFullTransaction,
) -> Result<()> {
    if block_header.transactions.is_empty() {
        return Ok(());
    }

    let processed_transactions: Result<Vec<_>> = block_header
        .transactions
        .iter()
        .map(|tx| {
            let tx_block_number = convert_hex_string_to_i64(&tx.block_number)
                .map_err(|_| BlockchainError::invalid_hex(&tx.block_number))?;
            let tx_index = convert_hex_string_to_i64(&tx.transaction_index)
                .map_err(|_| BlockchainError::invalid_hex(&tx.transaction_index))?;

            Ok((tx, tx_block_number, tx_index))
        })
        .collect();

    let processed_transactions = processed_transactions?;

    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new(
        "INSERT INTO transactions (
            block_number, transaction_hash, transaction_index,
            from_addr, to_addr, value, gas_price,
            max_priority_fee_per_gas, max_fee_per_gas, gas, chain_id
        ) ",
    );

    query_builder.push_values(
        processed_transactions.iter(),
        |mut b: Separated<'_, '_, Postgres, &'static str>, (tx_data, tx_block_number, tx_index)| {
            b.push_bind(*tx_block_number)
                .push_bind(&tx_data.hash)
                .push_bind(*tx_index)
                .push_bind(&tx_data.from)
                .push_bind(&tx_data.to)
                .push_bind(&tx_data.value)
                .push_bind(tx_data.gas_price.as_deref().unwrap_or("0"))
                .push_bind(tx_data.max_priority_fee_per_gas.as_deref().unwrap_or("0"))
                .push_bind(tx_data.max_fee_per_gas.as_deref().unwrap_or("0"))
                .push_bind(&tx_data.gas)
                .push_bind(&tx_data.chain_id);
        },
    );

    query_builder.push(" ON CONFLICT (transaction_hash) DO NOTHING");

    let query = query_builder.build();
    let result = query.execute(&mut **tx).await.map_err(|e| {
        BlockchainError::database_query(format!("Failed to insert transactions: {e}"))
    })?;

    info!(
        "Inserted {} transactions for block {}",
        result.rows_affected(),
        block_header.number
    );
    Ok(())
}

pub async fn write_blockheader(block_header: BlockHeaderWithFullTransaction) -> Result<()> {
    let pool = get_db_pool().await?;
    let mut tx = pool.begin().await?;

    let block_inserted = insert_block_header(&mut tx, &block_header).await?;
    if !block_inserted {
        return Ok(());
    }

    insert_transactions(&mut tx, &block_header).await?;

    tx.commit().await.map_err(|e| {
        BlockchainError::database_transaction(format!("Failed to commit transaction: {e}"))
    })?;
    Ok(())
}

async fn retry_async<F, T>(mut operation: F, max_retries: u32) -> Result<T>
where
    F: FnMut() -> futures::future::BoxFuture<'static, Result<T>>,
{
    let mut attempts = 0;
    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                attempts += 1;
                if attempts > max_retries || !is_transient_error(&e) {
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

#[allow(clippy::all)]
const fn is_transient_error(e: &BlockchainError) -> bool {
    // Check for transient error types
    match e {
        BlockchainError::DatabaseConnectionFailed { .. } => true,
        BlockchainError::RpcConnectionFailed { .. } => true,
        BlockchainError::RpcTimeout { .. } => true,
        BlockchainError::NetworkError { .. } => true,
        _ => false,
    }
}

#[derive(Debug)]
pub struct DbConnection {
    pub pool: Pool<Postgres>,
}

impl DbConnection {
    #[allow(dead_code)]
    pub async fn new(db_conn_string: String) -> Result<Arc<Self>> {
        let mut conn_options: PgConnectOptions = db_conn_string.parse()?;

        conn_options = conn_options
            .log_slow_statements(tracing::log::LevelFilter::Debug, Duration::new(120, 0));

        let pool = PgPoolOptions::new()
            .max_connections(DB_MAX_CONNECTIONS)
            .connect_with(conn_options)
            .await?;

        Ok(Arc::new(Self { pool }))
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod integration_tests {
    use std::env;

    use super::*;

    fn get_test_db_connection() -> String {
        env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@localhost:5433/fossil_test".to_string()
        })
    }

    #[tokio::test]
    async fn test_should_successfully_initialize_db() {
        let url = get_test_db_connection();
        let db = DbConnection::new(url).await.unwrap();

        assert!(db.pool.acquire().await.is_ok());
    }

    #[tokio::test]
    async fn test_should_fail_if_incorrect_db_url_provided() {
        assert!(DbConnection::new("test".to_string()).await.is_err());
    }
}
