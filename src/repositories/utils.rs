use std::time::Duration;

use eyre::Error;
use tokio::time::sleep;
use tracing::warn;

// TODO: revisit this to ensure that these 2 functions are still useful in current setup

#[allow(dead_code)]
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

#[allow(dead_code)]
fn is_transient_error(e: &Error) -> bool {
    // Check for database connection errors
    if let Some(db_err) = e.downcast_ref::<sqlx::Error>() {
        match db_err {
            sqlx::Error::Io(_) | sqlx::Error::PoolTimedOut => true,
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
