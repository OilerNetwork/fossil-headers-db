use std::{future::Future, time::Duration};

use eyre::{Context, Result};
use tokio::time::sleep;
use tracing::warn;

pub fn convert_hex_string_to_i64(hex_string: &str) -> Result<i64> {
    i64::from_str_radix(hex_string.trim_start_matches("0x"), 16).context("Invalid hex string")
}

pub fn convert_hex_string_to_i32(hex_string: &str) -> Result<i32> {
    i32::from_str_radix(hex_string.trim_start_matches("0x"), 16).context("Invalid hex string")
}

/// A generic retry mechanism for async operations that can fail.
///
/// # Type Parameters
/// * `F` - A future-producing closure that returns Result<T, E>
/// * `T` - The success type of the operation
/// * `E` - The error type that can be converted into eyre::Error
///
/// # Arguments
/// * `operation` - The async operation to retry
/// * `max_retries` - Maximum number of retry attempts
/// * `operation_name` - Name of the operation for logging purposes
///
/// # Returns
/// * `Result<T>` - The result of the operation if successful, or the last error encountered
pub async fn make_retrying_call<F, Fut, T>(
    operation: F,
    max_retries: u32,
    operation_name: &str,
) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut attempts = 0;
    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                attempts += 1;
                if attempts > max_retries {
                    warn!(
                        "{} with error: {:?}. Max retries reached",
                        operation_name, e
                    );
                    return Err(e);
                }
                let backoff = Duration::from_secs(2_u64.pow(attempts));
                warn!(
                    "{} with error: {:?}. Retrying in {:?} (Attempt {}/{})",
                    operation_name, e, backoff, attempts, max_retries
                );
                sleep(backoff).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_convert_hex_string_to_i64() {
        assert_eq!(convert_hex_string_to_i64("0x1").unwrap(), 1);
        assert_eq!(convert_hex_string_to_i64("0x2").unwrap(), 2);
        assert_eq!(convert_hex_string_to_i64("0xb").unwrap(), 11);
    }

    #[test]
    fn test_convert_hex_string_to_i64_invalid() {
        assert!(convert_hex_string_to_i64("invalid").is_err());
        assert!(convert_hex_string_to_i64("0xinvalid").is_err());
    }

    #[test]
    fn test_convert_hex_string_to_i64_no_prefix() {
        assert_eq!(convert_hex_string_to_i64("1").unwrap(), 1);
        assert_eq!(convert_hex_string_to_i64("2").unwrap(), 2);
        assert_eq!(convert_hex_string_to_i64("b").unwrap(), 11);
    }

    #[test]
    fn test_convert_hex_string_to_i32() {
        assert_eq!(convert_hex_string_to_i32("0x1").unwrap(), 1);
        assert_eq!(convert_hex_string_to_i32("0x2").unwrap(), 2);
        assert_eq!(convert_hex_string_to_i32("0xb").unwrap(), 11);
    }

    #[test]
    fn test_convert_hex_string_to_i32_invalid() {
        assert!(convert_hex_string_to_i32("invalid").is_err());
        assert!(convert_hex_string_to_i32("0xinvalid").is_err());
    }

    #[test]
    fn test_convert_hex_string_to_i32_no_prefix() {
        assert_eq!(convert_hex_string_to_i32("1").unwrap(), 1);
        assert_eq!(convert_hex_string_to_i32("2").unwrap(), 2);
        assert_eq!(convert_hex_string_to_i32("b").unwrap(), 11);
    }

    #[tokio::test]
    async fn test_make_retrying_call_success_first_try() {
        let operation = || async { Ok::<_, eyre::Report>(42) };
        let result = make_retrying_call(operation, 3, "test_operation").await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_make_retrying_call_success_after_retries() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let operation = move || {
            let current = counter_clone.fetch_add(1, Ordering::SeqCst);
            async move {
                if current < 2 {
                    Err::<_, eyre::Report>(eyre::eyre!("Expected error"))
                } else {
                    Ok(42)
                }
            }
        };

        let result = make_retrying_call(operation, 3, "test_operation").await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 3); // Operation was called 3 times
    }

    #[tokio::test]
    async fn test_make_retrying_call_max_retries_exceeded() {
        let operation = || async { Err::<i32, eyre::Report>(eyre::eyre!("Persistent error")) };
        let result = make_retrying_call(operation, 2, "test_operation").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Persistent error"));
    }

    #[tokio::test]
    async fn test_make_retrying_call_zero_retries() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let operation = move || {
            let current = counter_clone.fetch_add(1, Ordering::SeqCst);
            async move {
                if current < 2 {
                    Err::<_, eyre::Report>(eyre::eyre!("Expected error"))
                } else {
                    Ok(42)
                }
            }
        };

        let result = make_retrying_call(operation, 0, "test_operation").await;
        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 1); // Operation should be called once
    }
}
