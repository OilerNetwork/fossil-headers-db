use crate::errors::{BlockchainError, Result};
use sqlx::{query_builder::Separated, Postgres, QueryBuilder};

use crate::{
    rpc::{try_convert_full_tx_vector, BlockHeader, Transaction},
    utils::{convert_hex_string_to_i32, convert_hex_string_to_i64},
};

/// Internal data transfer object for transaction data.
///
/// This struct is used internally for database operations and is not part of the public API.
#[doc(hidden)]
#[allow(dead_code)]
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TransactionDto {
    pub transaction_hash: String,
    pub block_number: i64,
    pub transaction_index: i32,
    pub value: String,
    pub gas_price: String,
    pub gas: String,
    pub from_addr: Option<String>,
    pub to_addr: Option<String>,
    pub max_priority_fee_per_gas: String,
    pub max_fee_per_gas: String,
    pub chain_id: Option<String>,
}

/// Internal data transfer object for block header data.
///
/// This struct is used internally for database operations and is not part of the public API.
#[doc(hidden)]
#[allow(dead_code)]
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BlockHeaderDto {
    pub gas_limit: i64,
    pub gas_used: i64,
    pub base_fee_per_gas: Option<String>,
    pub block_hash: String,
    pub nonce: Option<String>,
    pub number: i64,
    pub receipts_root: String,
    pub state_root: String,
    pub transaction_root: String,
    pub parent_hash: Option<String>,
    pub miner: Option<String>,
    pub logs_bloom: Option<String>,
    pub difficulty: Option<String>,
    #[sqlx(rename = "totaldifficulty")]
    pub total_difficulty: Option<String>,
    pub sha3_uncles: Option<String>,
    pub timestamp: String,
    pub extra_data: Option<String>,
    pub mix_hash: Option<String>,
    pub withdrawals_root: Option<String>,
    pub blob_gas_used: Option<String>,
    pub excess_blob_gas: Option<String>,
    pub parent_beacon_block_root: Option<String>,
}

#[allow(dead_code)]
fn convert_rpc_blockheader_to_dto(block_header: BlockHeader) -> Result<BlockHeaderDto> {
    // These fields should be converted successfully, and if its not converted successfully,
    // it should be considered an unintended bug.
    let block_number = convert_hex_string_to_i64(&block_header.number)?;
    let gas_limit = convert_hex_string_to_i64(&block_header.gas_limit)?;
    let gas_used = convert_hex_string_to_i64(&block_header.gas_used)?;
    let block_timestamp = convert_hex_string_to_i64(&block_header.timestamp)?;
    let receipts_root = block_header.receipts_root.clone().ok_or_else(|| {
        BlockchainError::block_validation(
            block_header.number.clone().parse().unwrap_or(-1),
            "receipt root should not be empty",
        )
    })?;
    let state_root = block_header.state_root.clone().ok_or_else(|| {
        BlockchainError::block_validation(
            block_header.number.clone().parse().unwrap_or(-1),
            "state root should not be empty",
        )
    })?;
    let transaction_root = block_header.transactions_root.clone().ok_or_else(|| {
        BlockchainError::block_validation(
            block_header.number.clone().parse().unwrap_or(-1),
            "transactions root should not be empty",
        )
    })?;

    Ok(BlockHeaderDto {
        block_hash: block_header.hash.clone(),
        number: block_number,
        gas_limit,
        gas_used,
        base_fee_per_gas: block_header.base_fee_per_gas.clone(),
        nonce: block_header.nonce.clone(),

        parent_hash: block_header.parent_hash.clone(),
        miner: block_header.miner.clone(),
        logs_bloom: block_header.logs_bloom.clone(),
        difficulty: block_header.difficulty.clone(),
        total_difficulty: block_header.total_difficulty.clone(),
        sha3_uncles: block_header.sha3_uncles.clone(),
        timestamp: block_timestamp.to_string(),
        extra_data: block_header.extra_data.clone(),
        mix_hash: block_header.mix_hash.clone(),
        withdrawals_root: block_header.withdrawals_root.clone(),
        blob_gas_used: block_header.blob_gas_used.clone(),
        excess_blob_gas: block_header.excess_blob_gas.clone(),
        parent_beacon_block_root: block_header.parent_beacon_block_root,
        receipts_root,
        state_root,
        transaction_root,
    })
}

fn convert_rpc_transaction_to_dto(transaction: Transaction) -> Result<TransactionDto> {
    let block_number = convert_hex_string_to_i64(&transaction.block_number)?;
    let transaction_index = convert_hex_string_to_i32(&transaction.transaction_index)?;

    Ok(TransactionDto {
        transaction_hash: transaction.hash.clone(),
        block_number,
        transaction_index,
        from_addr: transaction.from.clone(),
        to_addr: transaction.to.clone(),
        value: transaction.value.clone(),
        gas_price: transaction
            .gas_price
            .clone()
            .unwrap_or_else(|| "0".to_string()),
        max_priority_fee_per_gas: transaction
            .max_priority_fee_per_gas
            .clone()
            .unwrap_or_else(|| "0".to_string()),
        max_fee_per_gas: transaction
            .max_fee_per_gas
            .clone()
            .unwrap_or_else(|| "0".to_string()),
        gas: transaction.gas.clone(),
        chain_id: transaction.chain_id,
    })
}

#[allow(dead_code)]
// Using transaction with multi row inserts seem to be the fastest
pub async fn insert_block_header_query(
    db_tx: &mut sqlx::Transaction<'_, Postgres>,
    block_headers: Vec<BlockHeader>,
) -> Result<()> {
    let mut formatted_block_headers: Vec<BlockHeaderDto> = Vec::with_capacity(block_headers.len());
    let mut flattened_transactions = Vec::new();

    for header in &block_headers {
        let dto = convert_rpc_blockheader_to_dto(header.clone())?;
        formatted_block_headers.push(dto);

        // Collect the transactions here and get the queries.
        // Here we assume that its a full transaction that is received
        let txs = try_convert_full_tx_vector(header.transactions.clone())?;
        flattened_transactions.extend(txs);
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
            b.push_bind(&block_header.block_hash)
                .push_bind(block_header.number)
                .push_bind(block_header.gas_limit)
                .push_bind(block_header.gas_used)
                .push_bind(&block_header.base_fee_per_gas)
                .push_bind(&block_header.nonce)
                .push_bind(&block_header.transaction_root)
                .push_bind(&block_header.receipts_root)
                .push_bind(&block_header.state_root)
                .push_bind(&block_header.parent_hash)
                .push_bind(&block_header.miner)
                .push_bind(&block_header.logs_bloom)
                .push_bind(&block_header.difficulty)
                .push_bind(&block_header.total_difficulty)
                .push_bind(&block_header.sha3_uncles)
                .push_bind(block_header.timestamp.to_string())
                .push_bind(&block_header.extra_data)
                .push_bind(&block_header.mix_hash)
                .push_bind(&block_header.withdrawals_root)
                .push_bind(&block_header.blob_gas_used)
                .push_bind(&block_header.excess_blob_gas)
                .push_bind(&block_header.parent_beacon_block_root);
        },
    );

    query_builder.push(
        r"
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
                parent_beacon_block_root = EXCLUDED.parent_beacon_block_root;",
    );

    query_builder.build().execute(&mut **db_tx).await?;

    if !flattened_transactions.is_empty() {
        insert_block_txs_query(db_tx, flattened_transactions).await?;
    }

    Ok(())
}

async fn insert_block_txs_query(
    db_tx: &mut sqlx::Transaction<'_, Postgres>,
    transactions: Vec<Transaction>,
) -> Result<()> {
    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new(
        "INSERT INTO transactions (
                block_number, transaction_hash, transaction_index,
                from_addr, to_addr, value, gas_price,
                max_priority_fee_per_gas, max_fee_per_gas, gas, chain_id
            )",
    );

    // Pre-format and handle the potential error arising from the hex -> i64 conversion
    let mut formatted_transactions: Vec<TransactionDto> = Vec::with_capacity(transactions.len());
    for transaction in &transactions {
        let dto = convert_rpc_transaction_to_dto(transaction.clone())?;
        formatted_transactions.push(dto);
    }

    // Push the formatted values into the query.
    query_builder.push_values(
        formatted_transactions.iter(),
        |mut b: Separated<'_, '_, Postgres, &'static str>, tx| {
            b.push_bind(tx.block_number)
                .push_bind(&tx.transaction_hash)
                .push_bind(tx.transaction_index)
                .push_bind(&tx.from_addr)
                .push_bind(&tx.to_addr)
                .push_bind(&tx.value)
                .push_bind(&tx.gas_price)
                .push_bind(&tx.max_priority_fee_per_gas)
                .push_bind(&tx.max_fee_per_gas)
                .push_bind(&tx.gas)
                .push_bind(&tx.chain_id);
        },
    );
    query_builder.push(
        r"
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
                chain_id = EXCLUDED.chain_id;",
    );
    query_builder.build().execute(&mut **db_tx).await?;

    Ok(())
}

#[allow(dead_code)]
pub async fn insert_block_header_only_query(
    db_tx: &mut sqlx::Transaction<'_, Postgres>,
    block_headers: Vec<BlockHeader>,
) -> Result<()> {
    let mut formatted_block_headers: Vec<BlockHeaderDto> = Vec::with_capacity(block_headers.len());

    for header in &block_headers {
        let dto = convert_rpc_blockheader_to_dto(header.clone())?;
        formatted_block_headers.push(dto);
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
            b.push_bind(&block_header.block_hash)
                .push_bind(block_header.number)
                .push_bind(block_header.gas_limit)
                .push_bind(block_header.gas_used)
                .push_bind(&block_header.base_fee_per_gas)
                .push_bind(&block_header.nonce)
                .push_bind(&block_header.transaction_root)
                .push_bind(&block_header.receipts_root)
                .push_bind(&block_header.state_root)
                .push_bind(&block_header.parent_hash)
                .push_bind(&block_header.miner)
                .push_bind(&block_header.logs_bloom)
                .push_bind(&block_header.difficulty)
                .push_bind(&block_header.total_difficulty)
                .push_bind(&block_header.sha3_uncles)
                .push_bind(block_header.timestamp.to_string())
                .push_bind(&block_header.extra_data)
                .push_bind(&block_header.mix_hash)
                .push_bind(&block_header.withdrawals_root)
                .push_bind(&block_header.blob_gas_used)
                .push_bind(&block_header.excess_blob_gas)
                .push_bind(&block_header.parent_beacon_block_root);
        },
    );

    query_builder.push(
        r"
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
                parent_beacon_block_root = EXCLUDED.parent_beacon_block_root;",
    );

    query_builder.build().execute(&mut **db_tx).await?;
    Ok(())
}

#[serial_test::serial]
#[cfg(test)]
mod tests {
    use std::{env, vec};

    use tokio::fs;

    use crate::{db::DbConnection, rpc::RpcResponse};

    use super::*;

    fn get_test_db_connection() -> String {
        env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@localhost:5433/fossil_test".to_string()
        })
    }

    fn assert_block_header_eq(header1: BlockHeaderDto, header2: BlockHeaderDto) {
        assert_eq!(header1.number, header2.number);
        assert_eq!(header1.block_hash, header2.block_hash);
        assert_eq!(header1.nonce, header2.nonce);
        assert_eq!(header1.parent_hash, header2.parent_hash);
        assert_eq!(header1.gas_limit, header2.gas_limit);
        assert_eq!(header1.gas_used, header2.gas_used);
        assert_eq!(header1.base_fee_per_gas, header2.base_fee_per_gas);
        assert_eq!(header1.receipts_root, header2.receipts_root);
        assert_eq!(header1.state_root, header2.state_root);
        assert_eq!(header1.transaction_root, header2.transaction_root);
        assert_eq!(header1.miner, header2.miner);
        assert_eq!(header1.logs_bloom, header2.logs_bloom);
        assert_eq!(header1.difficulty, header2.difficulty);
        assert_eq!(header1.total_difficulty, header2.total_difficulty);
        assert_eq!(header1.sha3_uncles, header2.sha3_uncles);
        assert_eq!(header1.timestamp, header2.timestamp);
        assert_eq!(header1.extra_data, header2.extra_data);
        assert_eq!(header1.mix_hash, header2.mix_hash);
        assert_eq!(header1.withdrawals_root, header2.withdrawals_root);
        assert_eq!(header1.blob_gas_used, header2.blob_gas_used);
        assert_eq!(header1.excess_blob_gas, header2.excess_blob_gas);
        assert_eq!(
            header1.parent_beacon_block_root,
            header2.parent_beacon_block_root
        );
    }

    fn assert_transactions_eq(transaction1: TransactionDto, transaction2: TransactionDto) {
        assert_eq!(transaction1.block_number, transaction2.block_number);
        assert_eq!(transaction1.transaction_hash, transaction2.transaction_hash);
        assert_eq!(
            transaction1.transaction_index,
            transaction2.transaction_index
        );
        assert_eq!(transaction1.from_addr, transaction2.from_addr);
        assert_eq!(transaction1.to_addr, transaction2.to_addr);
        assert_eq!(transaction1.value, transaction2.value);
        assert_eq!(transaction1.gas_price, transaction2.gas_price);
        assert_eq!(
            transaction1.max_priority_fee_per_gas,
            transaction2.max_priority_fee_per_gas
        );
        assert_eq!(transaction1.max_fee_per_gas, transaction2.max_fee_per_gas);
        assert_eq!(transaction1.gas, transaction2.gas);
        assert_eq!(transaction1.chain_id, transaction2.chain_id);
    }

    async fn get_fixtures_for_tests() -> Vec<BlockHeader> {
        let block_21598014 =
            fs::read_to_string("tests/fixtures/eth_getBlockByNumber_21598014.json")
                .await
                .unwrap();
        let block_21598015 =
            fs::read_to_string("tests/fixtures/eth_getBlockByNumber_21598015.json")
                .await
                .unwrap();

        let block_21598014_response =
            serde_json::from_str::<RpcResponse<BlockHeader>>(&block_21598014).unwrap();
        let block_21598015_response =
            serde_json::from_str::<RpcResponse<BlockHeader>>(&block_21598015).unwrap();

        vec![
            block_21598014_response.result,
            block_21598015_response.result,
        ]
    }

    #[tokio::test]
    async fn test_insert_block_header_success() {
        let url = get_test_db_connection();
        let db = DbConnection::new(url).await.unwrap();

        let block_headers = get_fixtures_for_tests().await;

        // Insert the block header
        let mut tx = db.pool.begin().await.unwrap();
        insert_block_header_query(&mut tx, vec![block_headers[0].clone()])
            .await
            .unwrap();

        // Check if the block header was inserted
        let result: std::result::Result<BlockHeaderDto, sqlx::Error> =
            sqlx::query_as("SELECT * FROM blockheaders WHERE number = $1")
                .bind(convert_hex_string_to_i64(&block_headers[0].number).unwrap())
                .fetch_one(&mut *tx)
                .await;
        assert!(result.is_ok());

        let block_header_to_compare =
            convert_rpc_blockheader_to_dto(block_headers[0].clone()).unwrap();
        let block_header_in_db = result.unwrap();

        assert_block_header_eq(block_header_in_db, block_header_to_compare);

        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn test_insert_multiple_block_header_success() {
        let url = get_test_db_connection();
        let db = DbConnection::new(url).await.unwrap();

        let block_headers = get_fixtures_for_tests().await;

        // Insert the block header
        let mut tx = db.pool.begin().await.unwrap();
        insert_block_header_query(&mut tx, block_headers.clone())
            .await
            .unwrap();

        // Check if the first block is inserted
        let result: std::result::Result<BlockHeaderDto, sqlx::Error> =
            sqlx::query_as("SELECT * FROM blockheaders WHERE number = $1")
                .bind(convert_hex_string_to_i64(&block_headers[0].number).unwrap())
                .fetch_one(&mut *tx)
                .await;
        assert!(result.is_ok());

        let block_header_to_compare =
            convert_rpc_blockheader_to_dto(block_headers[0].clone()).unwrap();
        let block_header_in_db = result.unwrap();
        assert_block_header_eq(block_header_in_db, block_header_to_compare);

        // Check if the second block is inserted
        let result: std::result::Result<BlockHeaderDto, sqlx::Error> =
            sqlx::query_as("SELECT * FROM blockheaders WHERE number = $1")
                .bind(convert_hex_string_to_i64(&block_headers[1].number).unwrap())
                .fetch_one(&mut *tx)
                .await;
        assert!(result.is_ok());

        let block_header_to_compare =
            convert_rpc_blockheader_to_dto(block_headers[1].clone()).unwrap();
        let block_header_in_db = result.unwrap();
        assert_block_header_eq(block_header_in_db, block_header_to_compare);

        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn test_insert_block_header_failed_hex_conversion() {
        let url = get_test_db_connection();
        let db = DbConnection::new(url).await.unwrap();

        let block_headers = get_fixtures_for_tests().await;

        // Insert the block header
        let mut tx = db.pool.begin().await.unwrap();
        let block_header = block_headers[0].clone();
        let block_header = BlockHeader {
            number: "0xg".to_string(),
            ..block_header
        };

        let result = insert_block_header_query(&mut tx, vec![block_header]).await;
        assert!(result.is_err());

        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn test_insert_block_header_with_number_conflict() {
        let url = get_test_db_connection();
        let db = DbConnection::new(url).await.unwrap();

        let block_headers = get_fixtures_for_tests().await;

        // Insert the block header
        let mut tx = db.pool.begin().await.unwrap();
        insert_block_header_query(&mut tx, vec![block_headers[0].clone()])
            .await
            .unwrap();

        // Insert the block header again
        let same_block_header_with_diff_values = block_headers[0].clone();
        let same_block_header_with_diff_values = BlockHeader {
            gas_limit: "0x1".to_string(),
            ..same_block_header_with_diff_values
        };
        let result =
            insert_block_header_query(&mut tx, vec![same_block_header_with_diff_values.clone()])
                .await;
        assert!(result.is_ok());

        // See if its properly updated
        let result: std::result::Result<BlockHeaderDto, sqlx::Error> =
            sqlx::query_as("SELECT * FROM blockheaders WHERE number = $1")
                .bind(convert_hex_string_to_i64(&block_headers[0].number).unwrap())
                .fetch_one(&mut *tx)
                .await;
        assert!(result.is_ok());
        let block_header_to_compare =
            convert_rpc_blockheader_to_dto(same_block_header_with_diff_values.clone()).unwrap();
        let block_header_in_db = result.unwrap();
        assert_block_header_eq(block_header_in_db, block_header_to_compare);

        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn test_insert_block_txs_success() {
        let url = get_test_db_connection();
        let db = DbConnection::new(url).await.unwrap();

        let block_headers = get_fixtures_for_tests().await;

        let mut tx = db.pool.begin().await.unwrap();

        // Insert the transactions
        let transaction: Transaction = block_headers[0].transactions[0].clone().try_into().unwrap();
        insert_block_txs_query(&mut tx, vec![transaction.clone()])
            .await
            .unwrap();

        // Check if the transactions are inserted
        let result: std::result::Result<TransactionDto, sqlx::Error> =
            sqlx::query_as("SELECT * FROM transactions WHERE transaction_hash = $1")
                .bind(&transaction.hash)
                .fetch_one(&mut *tx)
                .await;
        assert!(result.is_ok());

        let transaction_to_compare = convert_rpc_transaction_to_dto(transaction.clone()).unwrap();
        let transaction_in_db = result.unwrap();
        assert_transactions_eq(transaction_in_db, transaction_to_compare);

        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn test_insert_multiple_block_txs_success() {
        let url = get_test_db_connection();
        let db = DbConnection::new(url).await.unwrap();

        let block_headers = get_fixtures_for_tests().await;

        let mut tx = db.pool.begin().await.unwrap();

        // Insert the transactions
        let transaction_1: Transaction =
            block_headers[0].transactions[0].clone().try_into().unwrap();
        let transaction_2: Transaction =
            block_headers[0].transactions[1].clone().try_into().unwrap();

        insert_block_txs_query(&mut tx, vec![transaction_1.clone(), transaction_2.clone()])
            .await
            .unwrap();

        // Check if the transactions are inserted
        let result: std::result::Result<TransactionDto, sqlx::Error> =
            sqlx::query_as("SELECT * FROM transactions WHERE transaction_hash = $1")
                .bind(&transaction_1.hash)
                .fetch_one(&mut *tx)
                .await;
        assert!(result.is_ok());

        let transaction_to_compare = convert_rpc_transaction_to_dto(transaction_1.clone()).unwrap();
        let transaction_in_db = result.unwrap();
        assert_transactions_eq(transaction_in_db, transaction_to_compare);

        let result: std::result::Result<TransactionDto, sqlx::Error> =
            sqlx::query_as("SELECT * FROM transactions WHERE transaction_hash = $1")
                .bind(&transaction_2.hash)
                .fetch_one(&mut *tx)
                .await;
        assert!(result.is_ok());

        let transaction_to_compare = convert_rpc_transaction_to_dto(transaction_2.clone()).unwrap();
        let transaction_in_db = result.unwrap();
        assert_transactions_eq(transaction_in_db, transaction_to_compare);

        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn test_insert_block_txs_failed_hex_conversion() {
        let url = get_test_db_connection();
        let db = DbConnection::new(url).await.unwrap();

        let block_headers = get_fixtures_for_tests().await;

        let mut tx = db.pool.begin().await.unwrap();

        // Insert the transactions
        let transaction = block_headers[0].transactions[0].clone().try_into().unwrap();
        let transaction = Transaction {
            block_number: "0xg".to_string(),
            ..transaction
        };
        let result = insert_block_txs_query(&mut tx, vec![transaction]).await;
        assert!(result.is_err());

        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn test_insert_block_txs_with_hash_conflict() {
        let url = get_test_db_connection();
        let db = DbConnection::new(url).await.unwrap();

        let block_headers = get_fixtures_for_tests().await;

        let mut tx = db.pool.begin().await.unwrap();

        // Insert the transactions
        let transaction: Transaction = block_headers[0].transactions[0].clone().try_into().unwrap();
        insert_block_txs_query(&mut tx, vec![transaction.clone()])
            .await
            .unwrap();

        // Insert the transactions again
        let same_transaction_with_diff_values: Transaction =
            block_headers[0].transactions[0].clone().try_into().unwrap();
        let same_transaction_with_diff_values = Transaction {
            value: "0x1".to_string(),
            ..same_transaction_with_diff_values
        };
        let result =
            insert_block_txs_query(&mut tx, vec![same_transaction_with_diff_values.clone()]).await;
        assert!(result.is_ok());

        // See if its properly updated
        let result: std::result::Result<TransactionDto, sqlx::Error> =
            sqlx::query_as("SELECT * FROM transactions WHERE transaction_hash = $1")
                .bind(&transaction.hash)
                .fetch_one(&mut *tx)
                .await;
        assert!(result.is_ok());
        let transaction_to_compare =
            convert_rpc_transaction_to_dto(same_transaction_with_diff_values.clone()).unwrap();
        let transaction_in_db = result.unwrap();
        assert_transactions_eq(transaction_in_db, transaction_to_compare);

        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn test_insert_block_header_only_success() {
        let url = get_test_db_connection();
        let db = DbConnection::new(url).await.unwrap();

        let block_headers = get_fixtures_for_tests().await;

        // Insert the block header
        let mut tx = db.pool.begin().await.unwrap();
        insert_block_header_only_query(&mut tx, vec![block_headers[0].clone()])
            .await
            .unwrap();

        // Check if the block header was inserted
        let result: std::result::Result<BlockHeaderDto, sqlx::Error> =
            sqlx::query_as("SELECT * FROM blockheaders WHERE number = $1")
                .bind(convert_hex_string_to_i64(&block_headers[0].number).unwrap())
                .fetch_one(&mut *tx)
                .await;
        assert!(result.is_ok());

        let block_header_to_compare =
            convert_rpc_blockheader_to_dto(block_headers[0].clone()).unwrap();
        let block_header_in_db = result.unwrap();

        assert_block_header_eq(block_header_in_db, block_header_to_compare);

        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn test_insert_multiple_block_header_only_success() {
        let url = get_test_db_connection();
        let db = DbConnection::new(url).await.unwrap();

        let block_headers = get_fixtures_for_tests().await;

        // Insert the block header
        let mut tx = db.pool.begin().await.unwrap();
        insert_block_header_only_query(&mut tx, block_headers.clone())
            .await
            .unwrap();

        // Check if the first block is inserted
        let result: std::result::Result<BlockHeaderDto, sqlx::Error> =
            sqlx::query_as("SELECT * FROM blockheaders WHERE number = $1")
                .bind(convert_hex_string_to_i64(&block_headers[0].number).unwrap())
                .fetch_one(&mut *tx)
                .await;
        assert!(result.is_ok());

        let block_header_to_compare =
            convert_rpc_blockheader_to_dto(block_headers[0].clone()).unwrap();
        let block_header_in_db = result.unwrap();
        assert_block_header_eq(block_header_in_db, block_header_to_compare);

        // Check if the second block is inserted
        let result: std::result::Result<BlockHeaderDto, sqlx::Error> =
            sqlx::query_as("SELECT * FROM blockheaders WHERE number = $1")
                .bind(convert_hex_string_to_i64(&block_headers[1].number).unwrap())
                .fetch_one(&mut *tx)
                .await;
        assert!(result.is_ok());

        let block_header_to_compare =
            convert_rpc_blockheader_to_dto(block_headers[1].clone()).unwrap();
        let block_header_in_db = result.unwrap();
        assert_block_header_eq(block_header_in_db, block_header_to_compare);

        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn test_insert_block_header_only_failed_hex_conversion() {
        let url = get_test_db_connection();
        let db = DbConnection::new(url).await.unwrap();

        let block_headers = get_fixtures_for_tests().await;

        // Insert the block header
        let mut tx = db.pool.begin().await.unwrap();
        let block_header = block_headers[0].clone();
        let block_header = BlockHeader {
            number: "0xg".to_string(),
            ..block_header
        };

        let result = insert_block_header_only_query(&mut tx, vec![block_header]).await;
        assert!(result.is_err());

        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn test_insert_block_header_only_with_number_conflict() {
        let url = get_test_db_connection();
        let db = DbConnection::new(url).await.unwrap();

        let block_headers = get_fixtures_for_tests().await;

        // Insert the block header
        let mut tx = db.pool.begin().await.unwrap();
        insert_block_header_only_query(&mut tx, vec![block_headers[0].clone()])
            .await
            .unwrap();

        // Insert the block header again
        let same_block_header_with_diff_values = block_headers[0].clone();
        let same_block_header_with_diff_values = BlockHeader {
            gas_limit: "0x1".to_string(),
            ..same_block_header_with_diff_values
        };
        let result = insert_block_header_only_query(
            &mut tx,
            vec![same_block_header_with_diff_values.clone()],
        )
        .await;
        assert!(result.is_ok());

        // See if its properly updated
        let result: std::result::Result<BlockHeaderDto, sqlx::Error> =
            sqlx::query_as("SELECT * FROM blockheaders WHERE number = $1")
                .bind(convert_hex_string_to_i64(&block_headers[0].number).unwrap())
                .fetch_one(&mut *tx)
                .await;
        assert!(result.is_ok());
        let block_header_to_compare =
            convert_rpc_blockheader_to_dto(same_block_header_with_diff_values.clone()).unwrap();
        let block_header_in_db = result.unwrap();
        assert_block_header_eq(block_header_in_db, block_header_to_compare);

        tx.rollback().await.unwrap();
    }
}
