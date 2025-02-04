use std::{
    sync::{atomic::AtomicBool, Arc},
    thread,
    time::Duration,
};

use fossil_headers_db::{
    db::DbConnection,
    indexer::lib::{start_indexing_services, IndexingConfig},
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
async fn should_index_with_normal_rpc_without_tx() {
    let postgres_instance = Postgres::default().start().await.unwrap();
    let db_url = format!(
        "postgres://postgres:postgres@{}:{}/postgres",
        postgres_instance.get_host().await.unwrap(),
        postgres_instance.get_host_port_ipv4(5432).await.unwrap()
    );

    start_integration_mock_rpc_server("127.0.0.1:35351".to_owned())
        .await
        .unwrap();

    // Setting timeouts and retries to minimum for faster tests
    let indexing_config = IndexingConfig {
        db_conn_string: db_url.clone(),
        node_conn_string: "http://127.0.0.1:35351".to_owned(),
        should_index_txs: false,
        max_retries: 1,
        poll_interval: 1,
        rpc_timeout: 10,
        rpc_max_retries: 1,
        index_batch_size: 100, // larger size if we are indexing headers only
    };

    thread::spawn(move || {
        Runtime::new().unwrap().block_on(async move {
            start_indexing_services(indexing_config, Arc::new(AtomicBool::new(false)))
                .await
                .unwrap();
        });
    });

    // Wait for the indexer to finish
    sleep(Duration::from_secs(1)).await;

    // Check if indexing is done.
    let db = DbConnection::new(db_url).await.unwrap();
    let result: IndexMetadataDto = sqlx::query_as("SELECT * FROM index_metadata")
        .fetch_one(&db.pool)
        .await
        .unwrap();

    assert_eq!(result.current_latest_block_number, 5);
    assert_eq!(result.indexing_starting_block_number, 4);
    assert!(!result.is_backfilling);
    assert_eq!(result.backfilling_block_number, Some(0));

    // Check if headers are indexed correctly.
    let headers: Vec<BlockHeaderDto> =
        sqlx::query_as("SELECT * FROM blockheaders ORDER BY number ASC")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert_eq!(headers.len(), 6); // 0 - 5
    assert_eq!(headers[5].number, 5);
    // Hash taken from the block fixtures at tests/fixtures/indexer/eth_getBlockByNumber_sepolia_5.json
    assert_eq!(
        headers[5].block_hash,
        "0x290f89df59305c3d677c61be77279a942010e5687c7ab3bcda82954a96f1ceea"
    );
}

#[tokio::test]
async fn should_index_with_normal_rpc_with_tx() {
    let postgres_instance = Postgres::default().start().await.unwrap();
    let db_url = format!(
        "postgres://postgres:postgres@{}:{}/postgres",
        postgres_instance.get_host().await.unwrap(),
        postgres_instance.get_host_port_ipv4(5432).await.unwrap()
    );

    start_integration_mock_rpc_server("127.0.0.1:35352".to_owned())
        .await
        .unwrap();

    // Setting timeouts and retries to minimum for faster tests
    let indexing_config = IndexingConfig {
        db_conn_string: db_url.clone(),
        node_conn_string: "http://127.0.0.1:35352".to_owned(),
        should_index_txs: true,
        max_retries: 1,
        poll_interval: 1,
        rpc_timeout: 10,
        rpc_max_retries: 1,
        index_batch_size: 100, // larger size if we are indexing headers only
    };

    thread::spawn(move || {
        Runtime::new().unwrap().block_on(async move {
            start_indexing_services(indexing_config, Arc::new(AtomicBool::new(false)))
                .await
                .unwrap();
        });
    });

    // Wait for the indexer to finish
    sleep(Duration::from_secs(1)).await;

    // Check if indexing is done.
    let db = DbConnection::new(db_url).await.unwrap();
    let result: IndexMetadataDto = sqlx::query_as("SELECT * FROM index_metadata")
        .fetch_one(&db.pool)
        .await
        .unwrap();

    assert_eq!(result.current_latest_block_number, 5);
    assert_eq!(result.indexing_starting_block_number, 4);
    assert!(!result.is_backfilling);
    assert_eq!(result.backfilling_block_number, Some(0));

    // Check if headers were indexed correctly.
    let headers: Vec<BlockHeaderDto> =
        sqlx::query_as("SELECT * FROM blockheaders ORDER BY number ASC")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert_eq!(headers.len(), 6); // 0 - 5
    assert_eq!(headers[5].number, 5);
    // Hash taken from the block fixtures at tests/fixtures/indexer/eth_getBlockByNumber_sepolia_5.json
    assert_eq!(
        headers[5].block_hash,
        "0x290f89df59305c3d677c61be77279a942010e5687c7ab3bcda82954a96f1ceea"
    );

    // Check if txs were indexed correctly
    let transactions: Vec<TransactionDto> = sqlx::query_as("SELECT * FROM transactions")
        .fetch_all(&db.pool)
        .await
        .unwrap();
    assert_eq!(transactions.len(), 3);
    assert_eq!(transactions[2].block_number, 5);
    // Hash taken from the block fixtures at tests/fixtures/indexer/eth_getBlockByNumber_sepolia_5.json
    assert_eq!(
        transactions[2].transaction_hash,
        "0x71f743f444f577f1f952b16ef43aa5f3657007569b79355efda6729a27406a90"
    );
}

#[tokio::test]
async fn should_automatically_migrate_on_indexer_start() {
    let postgres_instance = Postgres::default().start().await.unwrap();
    let db_url = format!(
        "postgres://postgres:postgres@{}:{}/postgres",
        postgres_instance.get_host().await.unwrap(),
        postgres_instance.get_host_port_ipv4(5432).await.unwrap()
    );

    start_integration_mock_rpc_server("127.0.0.1:35355".to_owned())
        .await
        .unwrap();

    // Setting timeouts and retries to minimum for faster tests
    let indexing_config = IndexingConfig {
        db_conn_string: db_url.clone(),
        node_conn_string: "http://127.0.0.1:35355".to_owned(),
        should_index_txs: false,
        max_retries: 1,
        poll_interval: 1,
        rpc_timeout: 1,
        rpc_max_retries: 1,
        index_batch_size: 100, // larger size if we are indexing headers only
    };

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
async fn should_fail_to_index_without_rpc_available() {
    let postgres_instance = Postgres::default().start().await.unwrap();
    let db_url = format!(
        "postgres://postgres:postgres@{}:{}/postgres",
        postgres_instance.get_host().await.unwrap(),
        postgres_instance.get_host_port_ipv4(5432).await.unwrap()
    );

    // Setting timeouts and retries to minimum for faster tests
    let indexing_config = IndexingConfig {
        db_conn_string: db_url,
        node_conn_string: "".to_owned(),
        should_index_txs: false,
        max_retries: 0,
        poll_interval: 1,
        rpc_timeout: 1,
        rpc_max_retries: 0,
        index_batch_size: 100, // larger size if we are indexing headers only
    };

    // Empty rpc should cause the services to fail to index
    let result = start_indexing_services(indexing_config, Arc::new(AtomicBool::new(false))).await;

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().to_string(),
        "Failed to get latest block number"
    );
}

// TODO: add more tests for different cases.
// 1. intermittent shutdowns
// 2. more rpc behaviors
