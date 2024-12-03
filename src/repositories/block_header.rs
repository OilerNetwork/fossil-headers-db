use eyre::Result;
use sqlx::{Pool, Postgres, QueryBuilder};
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::{rpc::BlockHeaderWithFullTransaction, utils::convert_hex_string_to_i64};

use super::repository::RepositoryError;

pub trait BlockHeaderRepositoryTrait {
    async fn write_blockheader(
        &self,
        block_header: BlockHeaderWithFullTransaction,
    ) -> Result<(), RepositoryError>;
}

// Model is used to interact with the database
pub struct BlockHeaderRepository(Arc<Pool<Postgres>>);

impl BlockHeaderRepository {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        BlockHeaderRepository(pool)
    }
}

impl BlockHeaderRepositoryTrait for BlockHeaderRepository {
    async fn write_blockheader(
        &self,
        block_header: BlockHeaderWithFullTransaction,
    ) -> Result<(), RepositoryError> {
        let mut tx = self.0.begin().await?;

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
        .bind(convert_hex_string_to_i64(&block_header.number))
        .bind(convert_hex_string_to_i64(&block_header.gas_limit))
        .bind(convert_hex_string_to_i64(&block_header.gas_used))
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
        .bind(convert_hex_string_to_i64(&block_header.timestamp))
        .bind(&block_header.extra_data)
        .bind(&block_header.mix_hash)
        .bind(&block_header.withdrawals_root)
        .bind(&block_header.blob_gas_used)
        .bind(&block_header.excess_blob_gas)
        .bind(&block_header.parent_beacon_block_root)
        .execute(&mut *tx) // Changed this line
        .await;

        let result = match result {
            Ok(result) => result,
            Err(e) => {
                error!(
                    "Failed to insert block header for block number: {}.",
                    block_header.number
                );
                error!("Detailed error: {}", e);
                return Err(RepositoryError::InsertError(format!(
                    "Failed to insert block header for block number: {}",
                    block_header.number
                )));
            }
        };

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
            // TODO: probably need a on conflict clause here too.
            let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new(
                "INSERT INTO transactions (
                    block_number, transaction_hash, transaction_index,
                    from_addr, to_addr, value, gas_price,
                    max_priority_fee_per_gas, max_fee_per_gas, gas, chain_id
                ) ",
            );

            query_builder.push_values(block_header.transactions.iter(), |mut b, tx| {
                b.push_bind(convert_hex_string_to_i64(&tx.block_number))
                    .push_bind(&tx.hash)
                    .push_bind(convert_hex_string_to_i64(&tx.transaction_index))
                    .push_bind(&tx.from)
                    .push_bind(&tx.to)
                    .push_bind(&tx.value)
                    // Use "0" as the default value if gas_price is None
                    .push_bind(tx.gas_price.as_deref().unwrap_or("0"))
                    .push_bind(tx.max_priority_fee_per_gas.as_deref().unwrap_or("0"))
                    .push_bind(tx.max_fee_per_gas.as_deref().unwrap_or("0"))
                    .push_bind(&tx.gas)
                    .push_bind(&tx.chain_id);
            });

            query_builder.push(" ON CONFLICT (transaction_hash) DO UPDATE");

            let query = query_builder.build();
            let result = match query.execute(&mut *tx).await {
                Ok(result) => result,
                Err(e) => {
                    error!("Failed to insert transactions");
                    error!("Detailed error: {}", e);
                    return Err(RepositoryError::InsertError(
                        "Failed to insert transactions".to_string(),
                    ));
                }
            };

            info!(
                "Inserted {} transactions for block {}",
                result.rows_affected(),
                block_header.number
            );
        }

        match tx.commit().await {
            Ok(_) => (),
            Err(e) => {
                error!("Failed to commit transaction: {}", e);
                return Err(RepositoryError::UpdateError(
                    "Failed to commit transaction".to_string(),
                ));
            }
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // TODO: add tests here with db
}
