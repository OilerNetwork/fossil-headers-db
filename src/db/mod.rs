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

    pub async fn check_db_connection(&self) -> Result<()> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .context("Failed to query database connection")?;
        Ok(())
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
