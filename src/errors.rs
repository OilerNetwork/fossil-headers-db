use thiserror::Error;

/// Domain-specific error types for the blockchain indexer
#[derive(Error, Debug)]
pub enum BlockchainError {
    /// Errors related to invalid hex format
    #[error("Invalid hex format: {message}")]
    InvalidHexFormat { message: String },

    /// Errors related to RPC connections and requests
    #[error("RPC connection failed: {message}")]
    RpcConnectionFailed { message: String },

    /// Errors when RPC requests timeout
    #[error("RPC request timed out after {timeout_seconds} seconds")]
    RpcTimeout { timeout_seconds: u64 },

    /// Errors when a block is not found
    #[error("Block not found: {block_identifier}")]
    BlockNotFound { block_identifier: String },

    /// Errors when a transaction is not found
    #[error("Transaction not found: {transaction_hash}")]
    TransactionNotFound { transaction_hash: String },

    /// Database connection errors
    #[error("Database connection failed: {message}")]
    DatabaseConnectionFailed { message: String },

    /// Database transaction errors
    #[error("Database transaction failed: {operation}")]
    DatabaseTransactionFailed { operation: String },

    /// Database query errors
    #[error("Database query failed: {query}")]
    DatabaseQueryFailed { query: String },

    /// Configuration errors
    #[error("Configuration error: {parameter} - {message}")]
    ConfigurationError { parameter: String, message: String },

    /// Network-related errors
    #[error("Network error: {message}")]
    NetworkError { message: String },

    /// Block validation errors
    #[error("Block validation failed for block {block_number}: {reason}")]
    BlockValidationFailed { block_number: i64, reason: String },

    /// Transaction validation errors
    #[error("Transaction validation failed for {transaction_hash}: {reason}")]
    TransactionValidationFailed {
        transaction_hash: String,
        reason: String,
    },

    /// Range errors for block operations
    #[error("Invalid block range: start={start}, end={end}")]
    InvalidBlockRange { start: i64, end: i64 },

    /// Concurrency limit errors
    #[error("Concurrency limit exceeded: {current}/{max}")]
    ConcurrencyLimitExceeded { current: usize, max: usize },

    /// Generic internal errors
    #[error("Internal error: {message}")]
    InternalError { message: String },
}

impl BlockchainError {
    /// Create an invalid hex format error
    #[must_use]
    pub fn invalid_hex(value: &str) -> Self {
        Self::InvalidHexFormat {
            message: format!("Cannot parse hex value: '{value}'"),
        }
    }

    /// Create an invalid format error
    #[must_use]
    pub fn invalid_format(field_name: &str, message: &str) -> Self {
        Self::InvalidHexFormat {
            message: format!("Invalid {field_name}: {message}"),
        }
    }

    /// Create an RPC connection error
    pub fn rpc_connection(message: impl Into<String>) -> Self {
        Self::RpcConnectionFailed {
            message: message.into(),
        }
    }

    /// Create an RPC timeout error
    #[must_use]
    pub const fn rpc_timeout(timeout_seconds: u64) -> Self {
        Self::RpcTimeout { timeout_seconds }
    }

    /// Create a block not found error
    pub fn block_not_found(block_identifier: impl Into<String>) -> Self {
        Self::BlockNotFound {
            block_identifier: block_identifier.into(),
        }
    }

    /// Create a transaction not found error
    pub fn transaction_not_found(transaction_hash: impl Into<String>) -> Self {
        Self::TransactionNotFound {
            transaction_hash: transaction_hash.into(),
        }
    }

    /// Create a database connection error
    pub fn database_connection(message: impl Into<String>) -> Self {
        Self::DatabaseConnectionFailed {
            message: message.into(),
        }
    }

    /// Create a database transaction error
    pub fn database_transaction(operation: impl Into<String>) -> Self {
        Self::DatabaseTransactionFailed {
            operation: operation.into(),
        }
    }

    /// Create a database query error
    pub fn database_query(query: impl Into<String>) -> Self {
        Self::DatabaseQueryFailed {
            query: query.into(),
        }
    }

    /// Create a configuration error
    pub fn configuration(parameter: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ConfigurationError {
            parameter: parameter.into(),
            message: message.into(),
        }
    }

    /// Create a network error
    pub fn network(message: impl Into<String>) -> Self {
        Self::NetworkError {
            message: message.into(),
        }
    }

    /// Create a block validation error
    pub fn block_validation(block_number: i64, reason: impl Into<String>) -> Self {
        Self::BlockValidationFailed {
            block_number,
            reason: reason.into(),
        }
    }

    /// Create a transaction validation error
    pub fn transaction_validation(
        transaction_hash: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::TransactionValidationFailed {
            transaction_hash: transaction_hash.into(),
            reason: reason.into(),
        }
    }

    /// Create an invalid block range error
    #[must_use]
    pub const fn invalid_range(start: i64, end: i64) -> Self {
        Self::InvalidBlockRange { start, end }
    }

    /// Create a concurrency limit error
    #[must_use]
    pub const fn concurrency_limit(current: usize, max: usize) -> Self {
        Self::ConcurrencyLimitExceeded { current, max }
    }

    /// Create an internal error
    pub fn internal(message: impl Into<String>) -> Self {
        Self::InternalError {
            message: message.into(),
        }
    }
}

/// Result type alias for blockchain operations
pub type Result<T> = std::result::Result<T, BlockchainError>;

/// Convert from standard database errors
impl From<sqlx::Error> for BlockchainError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::Database(db_err) => {
                Self::database_query(format!("Database error: {db_err}"))
            }
            sqlx::Error::PoolClosed => {
                Self::database_connection("Connection pool closed".to_string())
            }
            sqlx::Error::PoolTimedOut => {
                Self::database_connection("Connection pool timed out".to_string())
            }
            _ => Self::database_connection(format!("SQLx error: {err}")),
        }
    }
}

/// Convert from reqwest errors
impl From<reqwest::Error> for BlockchainError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            Self::rpc_timeout(30) // Default timeout assumption
        } else if err.is_connect() {
            Self::rpc_connection(format!("Connection error: {err}"))
        } else {
            Self::network(format!("Request error: {err}"))
        }
    }
}

/// Convert from serde JSON errors
impl From<serde_json::Error> for BlockchainError {
    fn from(err: serde_json::Error) -> Self {
        Self::internal(format!("JSON parsing error: {err}"))
    }
}

/// Convert from hex parsing errors
impl From<std::num::ParseIntError> for BlockchainError {
    fn from(err: std::num::ParseIntError) -> Self {
        Self::invalid_hex(&format!("Parse error: {err}"))
    }
}

/// Convert from eyre errors for backward compatibility during migration
impl From<eyre::Report> for BlockchainError {
    fn from(err: eyre::Report) -> Self {
        Self::internal(format!("Legacy error: {err}"))
    }
}
