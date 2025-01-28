use crate::rpc::BlockHeaderWithFullTransaction;
use crate::utils::convert_hex_string_to_i64;
use eyre::{Context, Error, Result};
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

pub async fn get_db_pool() -> Result<Arc<Pool<Postgres>>> {
    if let Some(pool) = DB_POOL.get() {
        Ok(pool.clone())
    } else {
        let mut conn_options: PgConnectOptions = dotenvy::var("DB_CONNECTION_STRING")
            .context("DB_CONNECTION_STRING must be set")?
            .parse()?;
        conn_options = conn_options
            .log_slow_statements(tracing::log::LevelFilter::Debug, Duration::new(120, 0));

        let pool = PgPoolOptions::new()
            .max_connections(DB_MAX_CONNECTIONS)
            .connect_with(conn_options)
            .await?;
        let arc_pool = Arc::new(pool);
        match DB_POOL.set(arc_pool.clone()) {
            Ok(_) => Ok(arc_pool),
            Err(_) => DB_POOL
                .get()
                .ok_or_else(|| eyre::eyre!("Failed to get database pool after initialization"))
                .map(Clone::clone),
        }
    }
}

pub async fn check_db_connection() -> Result<()> {
    let pool = get_db_pool().await.context("Failed to get database pool")?;
    sqlx::query("SELECT 1")
        .execute(&*pool)
        .await
        .context("Failed to check database connection")?;
    Ok(())
}

/**
 * Retrieves the blocknumber of the latest stored blockheader
 *
 * @Returns blocknumber, else -1 if table is empty
 */
pub async fn get_last_stored_blocknumber() -> Result<i64> {
    const MAX_RETRIES: u32 = 3;

    retry_async(
        || {
            async {
                let pool = get_db_pool().await.context("Failed to get database pool")?;
                let result: (i64,) =
                    sqlx::query_as("SELECT COALESCE(MAX(number), -1) FROM blockheaders")
                        .fetch_one(&*pool)
                        .await
                        .context("Failed to get last stored block number")?;

                Ok(result.0)
            }
            .boxed()
        },
        MAX_RETRIES,
    )
    .await
}

/**
 * Returns the first missing blocknumber in between provided numbers (inclusive)
 */
pub async fn find_first_gap(start: i64, end: i64) -> Result<Option<i64>> {
    let pool = get_db_pool().await.context("Failed to get database pool")?;
    let result: Option<(i64,)> = sqlx::query_as(
        r#"
        WITH RECURSIVE number_series(n) AS (
            SELECT $1
            UNION ALL
            SELECT n + 1 FROM number_series WHERE n < $2
        )
        SELECT n FROM number_series
        WHERE n NOT IN (SELECT number FROM blockheaders WHERE number BETWEEN $1 AND $2)
        LIMIT 1
        "#,
    )
    .bind(start)
    .bind(end)
    .fetch_optional(&*pool)
    .await
    .context("Failed to find first gap")?;

    Ok(result.map(|r| r.0))
}

/**
 * This finds the null data by querying the blockheaders table.
 * Included fields is based on the reth primitives defined in https://reth.rs/docs/reth_primitives/struct.Header.html
 */
pub async fn find_null_data(start: i64, end: i64) -> Result<Vec<i64>> {
    let pool = get_db_pool().await.context("Failed to get database pool")?;
    let result: Vec<(i64,)> = sqlx::query_as(
        r#"
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
        "#,
    )
    .bind(start)
    .bind(end)
    .fetch_all(&*pool)
    .await
    .context("Failed to find any null data")?;

    Ok(result.iter().map(|row| row.0).collect())
}

pub async fn write_blockheader(block_header: BlockHeaderWithFullTransaction) -> Result<()> {
    let pool = get_db_pool().await?;
    let mut tx = pool.begin().await?;

    // // Print block_header details
    // info!("Block Header Details: {:?}", block_header);

    // Insert block header
    let result = sqlx::query(
        r#"
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
        "#,
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
    .execute(&mut *tx) // Changed this line
    .await
    .with_context(|| format!("Failed to insert block header for block number: {}", block_header.number))
    .map_err(|e| {
        error!("Detailed error: {:?}", e);
        e
    })?;

    if result.rows_affected() == 0 {
        warn!(
            "Block already exists: -- block number: {}, block hash: {}",
            block_header.number, block_header.hash
        );
        return Ok(());
    } else {
        info!(
            "Inserted block number: {}, block hash: {}",
            block_header.number, block_header.hash
        );
    }

    // Insert transactions
    if !block_header.transactions.is_empty() {
        let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new(
            "INSERT INTO transactions (
                block_number, transaction_hash, transaction_index,
                from_addr, to_addr, value, gas_price,
                max_priority_fee_per_gas, max_fee_per_gas, gas, chain_id
            ) ",
        );

        query_builder.push_values(
            block_header.transactions.iter(),
            |mut b: Separated<'_, '_, Postgres, &'static str>, tx| {
                // Convert values and unwrap_or_default() to handle errors
                let tx_block_number =
                    convert_hex_string_to_i64(&tx.block_number).unwrap_or_default();
                let tx_index = convert_hex_string_to_i64(&tx.transaction_index).unwrap_or_default();

                b.push_bind(tx_block_number)
                    .push_bind(&tx.hash)
                    .push_bind(tx_index)
                    .push_bind(&tx.from)
                    .push_bind(&tx.to)
                    .push_bind(&tx.value)
                    .push_bind(tx.gas_price.as_deref().unwrap_or("0"))
                    .push_bind(tx.max_priority_fee_per_gas.as_deref().unwrap_or("0"))
                    .push_bind(tx.max_fee_per_gas.as_deref().unwrap_or("0"))
                    .push_bind(&tx.gas)
                    .push_bind(&tx.chain_id);
            },
        );

        query_builder.push(" ON CONFLICT (transaction_hash) DO NOTHING");

        let query = query_builder.build();
        let result = query
            .execute(&mut *tx)
            .await
            .context("Failed to insert transactions")?;

        info!(
            "Inserted {} transactions for block {}",
            result.rows_affected(),
            block_header.number
        );
    }

    tx.commit().await.context("Failed to commit transaction")?;
    Ok(())
}

async fn retry_async<F, T>(mut operation: F, max_retries: u32) -> Result<T, Error>
where
    F: FnMut() -> futures::future::BoxFuture<'static, Result<T, Error>>,
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
fn is_transient_error(e: &Error) -> bool {
    // Check for database connection errors
    if let Some(db_err) = e.downcast_ref::<sqlx::Error>() {
        match db_err {
            sqlx::Error::Io(_) | sqlx::Error::PoolTimedOut => true,
            sqlx::Error::Database(db_err) => {
                // Check database-specific error codes if needed
                db_err.code().map_or(false, |code| {
                    // Add database-specific error codes that are transient
                    code == "57P01" // admin_shutdown, as an example
                })
            }
            _ => false,
        }
    } else {
        false
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
mod tests {
    use std::env;

    use super::*;

    fn get_test_db_connection() -> String {
        env::var("DATABASE_URL").unwrap()
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
