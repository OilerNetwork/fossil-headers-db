use eyre::{Context, Result};
use sqlx::postgres::PgConnectOptions;
use sqlx::ConnectOptions;
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use std::sync::Arc;
use std::time::Duration;

pub const DB_MAX_CONNECTIONS: u32 = 50;

#[derive(Debug)]
pub struct DbConnection {
    pub pool: Pool<Postgres>,
}

impl DbConnection {
    pub async fn new(db_conn_string: Option<String>) -> Result<Arc<Self>> {
        let mut conn_options: PgConnectOptions = match db_conn_string {
            Some(conn_string) => conn_string.parse()?,
            None => dotenvy::var("DB_CONNECTION_STRING")
                .context("DB_CONNECTION_STRING must be set")?
                .parse()?,
        };

        conn_options = conn_options
            .log_slow_statements(tracing::log::LevelFilter::Debug, Duration::new(120, 0));

        let pool = PgPoolOptions::new()
            .max_connections(DB_MAX_CONNECTIONS)
            .connect_with(conn_options)
            .await?;

        Ok(Arc::new(Self { pool }))
    }
}
