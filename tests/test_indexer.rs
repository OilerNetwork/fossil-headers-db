use testcontainers;
use testcontainers_modules::postgres::Postgres;

#[tokio::test]
async fn should_index_with_normal_rpc() {}

#[tokio::test]
async fn should_index_with_rpc_starting_from_zero() {}
#[tokio::test]

async fn should_index_correctly_with_intermittent_shutdowns() {}
#[tokio::test]

async fn should_index_correctly_with_far_ahead_rpc() {}
#[tokio::test]
async fn should_fail_to_index_without_rpc_available() {}

#[tokio::test]
async fn should_fail_to_index_if_db_not_migrated() {}
