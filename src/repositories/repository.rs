use core::error;
use std::{fmt::Display, time::Duration};

use eyre::Error;
use tokio::time::sleep;
use tracing::warn;

#[derive(Debug)]
pub enum RepositoryError {
    DatabaseError(sqlx::Error),
    UpdateError(String),
    UnexpectedError,
}

impl Display for RepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DatabaseError(err) => err.fmt(f),
            Self::UpdateError(err_str) => err_str.fmt(f),
            Self::UnexpectedError => write!(f, "Unexpected model error"),
        }
    }
}

impl error::Error for RepositoryError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            RepositoryError::DatabaseError(err) => Some(err),
            RepositoryError::UpdateError(_) => None,
            RepositoryError::UnexpectedError => None,
        }
    }
}

impl From<sqlx::Error> for RepositoryError {
    fn from(err: sqlx::Error) -> Self {
        RepositoryError::DatabaseError(err)
    }
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

fn is_transient_error(e: &Error) -> bool {
    // Check for database connection errors
    if let Some(db_err) = e.downcast_ref::<sqlx::Error>() {
        match db_err {
            sqlx::Error::Io(_) => true,
            sqlx::Error::PoolTimedOut => true,
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
