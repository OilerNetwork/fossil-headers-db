use std::{
    sync::{atomic::AtomicBool, Arc},
    thread,
    time::Duration,
};

use fossil_headers_db::{
    db::DbConnection,
    indexer::lib::{start_indexing_services, IndexingConfig, IndexingConfigBuilder},
    repositories::{
        block_header::{BlockHeaderDto, TransactionDto},
        index_metadata::IndexMetadataDto,
    },
};
use test_utils::start_integration_mock_rpc_server;
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};
use tokio::{runtime::Runtime, time::sleep};

#[cfg(test)]
mod test_utils;

#[tokio::test]
#[serial_test::serial]
async fn should_index_with_normal_rpc_without_tx() {
    let postgres_instance = Postgres::default().start().await.unwrap();
    let db_url = format!(
        "postgres://postgres:postgres@{}:{}/postgres",
        postgres_instance.get_host().await.unwrap(),
        postgres_instance.get_host_port_ipv4(5432).await.unwrap()
    );

    start_integration_mock_rpc_server("127.0.0.1:35371".to_owned())
        .await
        .unwrap();

    // Setting timeouts and retries to minimum for faster tests
    let indexing_config = IndexingConfigBuilder::testing()
        .db_conn_string(db_url.clone())
        .node_conn_string("http://127.0.0.1:35371")
        .should_index_txs(false)
        .rpc_timeout(10)
        .index_batch_size(100)
        .build()
        .unwrap();

    thread::spawn(move || {
        Runtime::new().unwrap().block_on(async move {
            start_indexing_services(indexing_config, Arc::new(AtomicBool::new(false)))
                .await
                .unwrap();
        });
    });

    // Wait for the indexer to finish
    sleep(Duration::from_secs(5)).await;

    // Check if indexing is done.
    let db = DbConnection::new(db_url).await.unwrap();
    let result: IndexMetadataDto = sqlx::query_as("SELECT * FROM index_metadata")
        .fetch_one(&db.pool)
        .await
        .unwrap();

    assert_eq!(result.current_latest_block_number, 5);
    assert_eq!(result.indexing_starting_block_number, 0);
    assert!(!result.is_backfilling);
    assert_eq!(result.backfilling_block_number, Some(0));

    // Check if headers are indexed correctly.
    let headers: Vec<BlockHeaderDto> =
        sqlx::query_as("SELECT * FROM blockheaders ORDER BY number ASC")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert_eq!(headers.len(), 5); // 0 - 4 (batch indexer)
    assert_eq!(headers[4].number, 4);
    // Hash taken from the block fixtures at tests/fixtures/indexer/eth_getBlockByNumber_sepolia_4.json
    assert_eq!(
        headers[4].block_hash,
        "0x3736e6e39d90d95fc157b01f36f40c4b598f754f8912c57fa515f6051186c921"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn should_index_with_normal_rpc_with_tx() {
    let postgres_instance = Postgres::default().start().await.unwrap();
    let db_url = format!(
        "postgres://postgres:postgres@{}:{}/postgres",
        postgres_instance.get_host().await.unwrap(),
        postgres_instance.get_host_port_ipv4(5432).await.unwrap()
    );

    start_integration_mock_rpc_server("127.0.0.1:35372".to_owned())
        .await
        .unwrap();

    // Setting timeouts and retries to minimum for faster tests
    let indexing_config = IndexingConfigBuilder::testing()
        .db_conn_string(db_url.clone())
        .node_conn_string("http://127.0.0.1:35372")
        .should_index_txs(true)
        .rpc_timeout(10)
        .index_batch_size(100)
        .build()
        .unwrap();

    thread::spawn(move || {
        Runtime::new().unwrap().block_on(async move {
            start_indexing_services(indexing_config, Arc::new(AtomicBool::new(false)))
                .await
                .unwrap();
        });
    });

    // Wait for the indexer to finish
    sleep(Duration::from_secs(5)).await;

    // Check if indexing is done.
    let db = DbConnection::new(db_url).await.unwrap();
    let result: IndexMetadataDto = sqlx::query_as("SELECT * FROM index_metadata")
        .fetch_one(&db.pool)
        .await
        .unwrap();

    assert_eq!(result.current_latest_block_number, 5);
    assert_eq!(result.indexing_starting_block_number, 0);
    assert!(!result.is_backfilling);
    assert_eq!(result.backfilling_block_number, Some(0));

    // Check if headers were indexed correctly.
    let headers: Vec<BlockHeaderDto> =
        sqlx::query_as("SELECT * FROM blockheaders ORDER BY number ASC")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert_eq!(headers.len(), 5); // 0 - 4 (batch indexer)
    assert_eq!(headers[4].number, 4);
    // Hash taken from the block fixtures at tests/fixtures/indexer/eth_getBlockByNumber_sepolia_4.json
    assert_eq!(
        headers[4].block_hash,
        "0x3736e6e39d90d95fc157b01f36f40c4b598f754f8912c57fa515f6051186c921"
    );

    // Check if txs were indexed correctly
    let transactions: Vec<TransactionDto> = sqlx::query_as("SELECT * FROM transactions")
        .fetch_all(&db.pool)
        .await
        .unwrap();
    // Since only blocks 0-4 are indexed and they have no transactions, expect 0
    assert_eq!(transactions.len(), 0);
}

#[tokio::test]
#[serial_test::serial]
async fn should_automatically_migrate_on_indexer_start() {
    let postgres_instance = Postgres::default().start().await.unwrap();
    let db_url = format!(
        "postgres://postgres:postgres@{}:{}/postgres",
        postgres_instance.get_host().await.unwrap(),
        postgres_instance.get_host_port_ipv4(5432).await.unwrap()
    );

    start_integration_mock_rpc_server("127.0.0.1:35373".to_owned())
        .await
        .unwrap();

    // Setting timeouts and retries to minimum for faster tests
    let indexing_config = IndexingConfigBuilder::testing()
        .db_conn_string(db_url.clone())
        .node_conn_string("http://127.0.0.1:35373")
        .should_index_txs(false)
        .index_batch_size(100)
        .build()
        .unwrap();

    thread::spawn(move || {
        Runtime::new().unwrap().block_on(async move {
            start_indexing_services(indexing_config, Arc::new(AtomicBool::new(false)))
                .await
                .unwrap();
        });
    });

    // Wait for the indexer to start
    sleep(Duration::from_secs(1)).await;

    // Check if migration is applied.
    let db = DbConnection::new(db_url).await.unwrap();
    let existing_table: Vec<(i32, )> = sqlx::query_as("select 1 as flag from information_schema.tables where table_name  in ('transactions', 'blockheaders', 'index_metadata')").fetch_all(&db.pool).await.unwrap();

    assert_eq!(existing_table.len(), 3);
}

#[tokio::test]
#[serial_test::serial]
async fn should_fail_to_index_without_rpc_available() {
    let postgres_instance = Postgres::default().start().await.unwrap();
    let db_url = format!(
        "postgres://postgres:postgres@{}:{}/postgres",
        postgres_instance.get_host().await.unwrap(),
        postgres_instance.get_host_port_ipv4(5432).await.unwrap()
    );

    // Setting timeouts and retries to minimum for faster tests
    let indexing_config = IndexingConfig::builder()
        .db_conn_string(db_url)
        .node_conn_string("")
        .should_index_txs(false)
        .max_retries(1)
        .poll_interval(1)
        .rpc_timeout(1)
        .rpc_max_retries(1)
        .index_batch_size(100)
        .build()
        .unwrap();

    // Empty rpc should cause the services to fail to index
    let result = start_indexing_services(indexing_config, Arc::new(AtomicBool::new(false))).await;

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().to_string(),
        "RPC connection failed: Failed to get latest block number: Network error: Request error: builder error"
    );
}

// TODO: add more tests for different cases.
// 1. intermittent shutdowns
// 2. more rpc behaviors
