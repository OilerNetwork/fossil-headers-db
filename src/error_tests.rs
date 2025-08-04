//! # Comprehensive Error Path Testing
//!
//! This module contains comprehensive tests for all error scenarios across the codebase.

#[cfg(test)]
mod error_scenario_tests {
    use crate::errors::BlockchainError;
    use crate::types::BlockNumber;

    mod blockchain_error_tests {
        use super::*;

        #[test]
        fn test_blockchain_error_construction_and_display() {
            let rpc_error = BlockchainError::RpcConnectionFailed {
                message: "Connection failed".to_string(),
            };
            let error_message = format!("{}", rpc_error);
            assert!(error_message.contains("Connection failed"));

            let db_error = BlockchainError::DatabaseConnectionFailed {
                message: "DB connection lost".to_string(),
            };
            let error_message = format!("{}", db_error);
            assert!(error_message.contains("DB connection lost"));

            let query_error = BlockchainError::DatabaseQueryFailed {
                query: "SELECT * FROM blocks".to_string(),
            };
            let error_message = format!("{}", query_error);
            assert!(error_message.contains("SELECT"));

            let hex_error = BlockchainError::InvalidHexFormat {
                message: "Invalid hex characters in 0xGHIJ".to_string(),
            };
            let error_message = format!("{}", hex_error);
            assert!(error_message.contains("Invalid hex"));

            let block_error = BlockchainError::BlockNotFound {
                block_identifier: "12345".to_string(),
            };
            let error_message = format!("{}", block_error);
            assert!(error_message.contains("12345"));

            let config_error = BlockchainError::ConfigurationError {
                parameter: "DB_URL".to_string(),
                message: "Missing environment variable".to_string(),
            };
            let error_message = format!("{}", config_error);
            assert!(error_message.contains("DB_URL"));
            assert!(error_message.contains("Missing"));
        }

        #[test]
        fn test_blockchain_error_debug_formatting() {
            let error = BlockchainError::InvalidHexFormat {
                message: "Invalid hex format".to_string(),
            };
            let debug_string = format!("{:?}", error);
            assert!(debug_string.contains("InvalidHexFormat"));
        }
    }

    mod error_construction_tests {
        use super::*;

        #[test]
        fn test_all_error_variants_can_be_constructed() {
            let errors = vec![
                BlockchainError::InvalidHexFormat {
                    message: "test".to_string(),
                },
                BlockchainError::RpcConnectionFailed {
                    message: "test".to_string(),
                },
                BlockchainError::RpcTimeout {
                    timeout_seconds: 30,
                },
                BlockchainError::BlockNotFound {
                    block_identifier: "123".to_string(),
                },
                BlockchainError::TransactionNotFound {
                    transaction_hash: "0x123".to_string(),
                },
                BlockchainError::DatabaseConnectionFailed {
                    message: "test".to_string(),
                },
                BlockchainError::DatabaseTransactionFailed {
                    operation: "INSERT".to_string(),
                },
                BlockchainError::DatabaseQueryFailed {
                    query: "SELECT".to_string(),
                },
                BlockchainError::ConfigurationError {
                    parameter: "TEST".to_string(),
                    message: "test".to_string(),
                },
                BlockchainError::NetworkError {
                    message: "test".to_string(),
                },
                BlockchainError::BlockValidationFailed {
                    block_number: 123,
                    reason: "test".to_string(),
                },
            ];

            for error in errors {
                // Test that all errors can be formatted
                let _ = format!("{}", error);
                let _ = format!("{:?}", error);
            }
        }
    }

    mod validation_tests {
        use super::*;

        #[test]
        fn test_block_number_operations() {
            let block1 = BlockNumber::from_trusted(1000);
            let block2 = BlockNumber::from_trusted(2000);

            assert!(block1.value() < block2.value());
            assert_ne!(block1, block2);
            assert_eq!(block1, BlockNumber::from_trusted(1000));
        }

        #[test]
        fn test_error_message_content() {
            let error = BlockchainError::RpcTimeout {
                timeout_seconds: 30,
            };
            let message = format!("{}", error);
            assert!(message.contains("30"));
            assert!(message.contains("timed out"));
        }
    }
}
