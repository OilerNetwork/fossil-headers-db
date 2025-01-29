use std::sync::{atomic::AtomicBool, Arc};

use fossil_headers_db::indexer::lib::{start_indexing_services, IndexingConfig};
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

mod test_utils;

#[tokio::test]
async fn should_index_with_normal_rpc() {
    // let block_fixtures = get_fixtures_for_tests().await;
    let postgres_instance = Postgres::default().start().await.unwrap();
    let url = format!(
        "postgres://postgres:postgres@{}:{}/postgres",
        postgres_instance.get_host().await.unwrap(),
        postgres_instance.get_host_port_ipv4(5432).await.unwrap()
    );
}

#[tokio::test]
async fn should_index_with_rpc_starting_from_zero() {}

#[tokio::test]
async fn should_index_correctly_with_intermittent_shutdowns() {}

#[tokio::test]
async fn should_index_correctly_with_far_ahead_rpc() {}

#[tokio::test]
async fn should_automatically_migrate_on_indexer_start() {}

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
}
