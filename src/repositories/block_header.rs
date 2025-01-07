use eyre::{ContextCompat, Result};
use sqlx::{query_builder::Separated, Postgres, QueryBuilder};

use crate::{
    rpc::{BlockHeaderWithFullTransaction, Transaction},
    utils::convert_hex_string_to_i64,
};

// TODO: allow dead code for now. Adding tests in future PRs should allow us to remove this.
#[allow(dead_code)]
#[derive(Debug)]
pub struct TransactionFormatted {
    pub hash: String,
    pub block_number: i64,
    pub transaction_index: i64,
    pub value: String,
    pub gas_price: String,
    pub gas: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub max_priority_fee_per_gas: String,
    pub max_fee_per_gas: String,
    pub chain_id: Option<String>,
}

// TODO: allow dead code for now. Adding tests in future PRs should allow us to remove this.
#[allow(dead_code)]
#[derive(Debug)]
pub struct BlockHeaderFormatted {
    pub gas_limit: i64,
    pub gas_used: i64,
    pub base_fee_per_gas: Option<String>,
    pub hash: String,
    pub nonce: Option<String>,
    pub number: i64,
    pub receipts_root: String,
    pub state_root: String,
    pub transactions_root: String,
    pub parent_hash: Option<String>,
    pub miner: Option<String>,
    pub logs_bloom: Option<String>,
    pub difficulty: Option<String>,
    pub total_difficulty: Option<String>,
    pub sha3_uncles: Option<String>,
    pub timestamp: i64,
    pub extra_data: Option<String>,
    pub mix_hash: Option<String>,
    pub withdrawals_root: Option<String>,
    pub blob_gas_used: Option<String>,
    pub excess_blob_gas: Option<String>,
    pub parent_beacon_block_root: Option<String>,
}

// TODO: allow dead code for now. Adding tests in future PRs should allow us to remove this.
#[allow(dead_code)]
// Seems that using transaction with multi row inserts seem to be the fastest
pub async fn insert_block_header_query(
    db_tx: &mut sqlx::Transaction<'_, Postgres>,
    block_headers: Vec<BlockHeaderWithFullTransaction>,
) -> Result<()> {
    let mut formatted_block_headers: Vec<BlockHeaderFormatted> =
        Vec::with_capacity(block_headers.len());
    let mut flattened_transactions = Vec::new();

    for header in block_headers.iter() {
        // These fields should be converted successfully, and if its not converted successfully,
        // it should be considered an unintended bug,
        let block_number = convert_hex_string_to_i64(&header.number)?;
        let gas_limit = convert_hex_string_to_i64(&header.gas_limit)?;
        let gas_used = convert_hex_string_to_i64(&header.gas_used)?;
        let block_timestamp = convert_hex_string_to_i64(&header.timestamp)?;
        let receipts_root = header
            .receipts_root
            .clone()
            .context("receipt root should not be empty")?;
        let state_root = header
            .state_root
            .clone()
            .context("state root should not be empty")?;
        let transactions_root = header
            .transactions_root
            .clone()
            .context("transactions root should not be empty")?;

        formatted_block_headers.push(BlockHeaderFormatted {
            hash: header.hash.clone(),
            number: block_number,
            gas_limit,
            gas_used,
            base_fee_per_gas: header.base_fee_per_gas.clone(),
            nonce: header.nonce.clone(),

            parent_hash: header.parent_hash.clone(),
            miner: header.miner.clone(),
            logs_bloom: header.logs_bloom.clone(),
            difficulty: header.difficulty.clone(),
            total_difficulty: header.total_difficulty.clone(),
            sha3_uncles: header.sha3_uncles.clone(),
            timestamp: block_timestamp,
            extra_data: header.extra_data.clone(),
            mix_hash: header.mix_hash.clone(),
            withdrawals_root: header.withdrawals_root.clone(),
            blob_gas_used: header.blob_gas_used.clone(),
            excess_blob_gas: header.excess_blob_gas.clone(),
            parent_beacon_block_root: header.parent_beacon_block_root.clone(),
            receipts_root,
            state_root,
            transactions_root,
        });

        // Collect the transactions here and get the queries.
        flattened_transactions.extend(header.transactions.clone());
    }

    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new(
        "INSERT INTO blockheaders (
                block_hash, number, gas_limit, gas_used, base_fee_per_gas,
                nonce, transaction_root, receipts_root, state_root,
                parent_hash, miner, logs_bloom, difficulty, totalDifficulty,
                sha3_uncles, timestamp, extra_data, mix_hash, withdrawals_root,
                blob_gas_used, excess_blob_gas, parent_beacon_block_root
            )",
    );

    query_builder.push_values(
        formatted_block_headers.iter(),
        |mut b: Separated<'_, '_, Postgres, &'static str>, block_header| {
            // Convert values and unwrap_or_default() to handle errors
            b.push_bind(&block_header.hash)
                .push_bind(block_header.number)
                .push_bind(block_header.gas_limit)
                .push_bind(block_header.gas_used)
                .push_bind(&block_header.base_fee_per_gas)
                .push_bind(&block_header.nonce)
                .push_bind(&block_header.transactions_root)
                .push_bind(&block_header.receipts_root)
                .push_bind(&block_header.state_root)
                .push_bind(&block_header.parent_hash)
                .push_bind(&block_header.miner)
                .push_bind(&block_header.logs_bloom)
                .push_bind(&block_header.difficulty)
                .push_bind(&block_header.total_difficulty)
                .push_bind(&block_header.sha3_uncles)
                .push_bind(block_header.timestamp)
                .push_bind(&block_header.extra_data)
                .push_bind(&block_header.mix_hash)
                .push_bind(&block_header.withdrawals_root)
                .push_bind(&block_header.blob_gas_used)
                .push_bind(&block_header.excess_blob_gas)
                .push_bind(&block_header.parent_beacon_block_root);
        },
    );

    query_builder.push(
        r#"
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
                parent_beacon_block_root = EXCLUDED.parent_beacon_block_root;"#,
    );

    query_builder.build().execute(&mut **db_tx).await?;

    insert_block_txs_query(db_tx, flattened_transactions).await?;

    Ok(())
}

// TODO: allow dead code for now. Adding tests in future PRs should allow us to remove this.
#[allow(dead_code)]
pub async fn insert_block_txs_query(
    db_tx: &mut sqlx::Transaction<'_, Postgres>,
    transactions: Vec<Transaction>,
) -> Result<()> {
    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new(
        "INSERT INTO transactions (
                block_number, transaction_hash, transaction_index,
                from_addr, to_addr, value, gas_price,
                max_priority_fee_per_gas, max_fee_per_gas, gas, chain_id
            ) ",
    );

    // Pre-format and handle the potential error arising from the hex -> i64 conversion

    let mut formatted_transactions: Vec<TransactionFormatted> =
        Vec::with_capacity(transactions.len());
    for transaction in transactions.iter() {
        let tx_block_number = convert_hex_string_to_i64(&transaction.block_number)?;
        let tx_index = convert_hex_string_to_i64(&transaction.transaction_index)?;

        formatted_transactions.push(TransactionFormatted {
            block_number: tx_block_number,
            hash: transaction.hash.clone(),
            transaction_index: tx_index,
            from: transaction.from.clone(),
            to: transaction.to.clone(),
            value: transaction.value.clone(),
            // TODO: is this a good idea? To default to 0? Does this interfere with any calculations?
            gas_price: transaction.gas_price.clone().unwrap_or("0".to_string()),
            max_priority_fee_per_gas: transaction
                .max_priority_fee_per_gas
                .clone()
                .unwrap_or("0".to_string()),
            max_fee_per_gas: transaction
                .max_fee_per_gas
                .clone()
                .unwrap_or("0".to_string()),
            gas: transaction.gas.clone(),
            chain_id: transaction.chain_id.clone(),
        });
    }

    // Push the formatted values into the query.
    query_builder.push_values(
        formatted_transactions.iter(),
        |mut b: Separated<'_, '_, Postgres, &'static str>, tx| {
            // Convert values and unwrap_or_default() to handle errors
            b.push_bind(tx.block_number)
                .push_bind(&tx.hash)
                .push_bind(tx.transaction_index)
                .push_bind(&tx.from)
                .push_bind(&tx.to)
                .push_bind(&tx.value)
                .push_bind(&tx.gas_price)
                .push_bind(&tx.max_priority_fee_per_gas)
                .push_bind(&tx.max_fee_per_gas)
                .push_bind(&tx.gas)
                .push_bind(&tx.chain_id);
        },
    );
    query_builder.push(
        r#"
        ON CONFLICT (transaction_hash)
            DO UPDATE SET
                block_number = EXCLUDED.block_number,
                transaction_index = EXCLUDED.transaction_index,
                from_addr = EXCLUDED.from_addr,
                to_addr = EXCLUDED.to_addr,
                value = EXCLUDED.value,
                gas_price = EXCLUDED.gas_price,
                max_priority_fee_per_gas = EXCLUDED.max_priority_fee_per_gas,
                max_fee_per_gas = EXCLUDED.max_fee_per_gas,
                gas = EXCLUDED.gas,
                chain_id = EXCLUDED.chain_id;"#,
    );
    query_builder.build().execute(&mut **db_tx).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    // TODO: add tests here with db
}
