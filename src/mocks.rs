//! # Mock Implementations for Testing
//!
//! This module provides mock implementations of core traits to enable isolated unit testing
//! without requiring external dependencies like live Ethereum nodes or database connections.
//!
//! ## Available Mocks
//!
//! - [`MockEthereumRpcProvider`] - Mock implementation of [`crate::rpc::EthereumRpcProvider`]
//! - [`MockDatabaseProvider`] - Mock implementation for database operations
//!

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::arithmetic_side_effects)]
#![allow(dead_code)]
//! ## Usage
//!
//! These mocks are only available when building with the `cfg(test)` attribute and are
//! automatically imported when running tests.

#[cfg(test)]
use mockall::mock;

#[cfg(test)]
use crate::errors::Result;
#[cfg(test)]
use crate::rpc::{BlockHeader, EthereumRpcProvider};
#[cfg(test)]
use crate::types::BlockNumber;

#[cfg(test)]
mock! {
    /// Mock implementation of the Ethereum RPC provider trait.
    ///
    /// This mock allows for testing RPC-dependent code without making actual network calls.
    /// You can configure expected method calls and return values using the mockall framework.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use fossil_headers_db::mocks::MockEthereumRpcProvider;
    /// use fossil_headers_db::types::BlockNumber;
    ///
    /// let mut mock_rpc = MockEthereumRpcProvider::new();
    /// mock_rpc
    ///     .expect_get_latest_finalized_blocknumber()
    ///     .times(1)
    ///     .returning(|_| Box::pin(async { Ok(BlockNumber::from_trusted(12345)) }));
    /// ```
    pub EthereumRpcProvider {}

    impl EthereumRpcProvider for EthereumRpcProvider {
        async fn get_latest_finalized_blocknumber(&self, timeout: Option<u64>) -> Result<BlockNumber>;
        async fn get_full_block_by_number(
            &self,
            number: BlockNumber,
            include_tx: bool,
            timeout: Option<u64>,
        ) -> Result<BlockHeader>;
    }
}

#[cfg(test)]
#[async_trait::async_trait]
pub trait DatabaseProvider {
    async fn get_last_stored_blocknumber(&self) -> Result<BlockNumber>;
    async fn find_first_gap(
        &self,
        start: BlockNumber,
        end: BlockNumber,
    ) -> Result<Option<BlockNumber>>;
    async fn check_connection(&self) -> Result<()>;
    async fn insert_block_header(
        &self,
        block_number: BlockNumber,
        block_data: String,
    ) -> Result<()>;
    async fn block_exists(&self, block_number: BlockNumber) -> Result<bool>;
}

#[cfg(test)]
mock! {
    /// Mock implementation for database operations.
    ///
    /// This mock provides testable implementations of core database operations
    /// without requiring a real database connection.
    pub DatabaseProvider {}

    #[async_trait::async_trait]
    impl DatabaseProvider for DatabaseProvider {
        async fn get_last_stored_blocknumber(&self) -> Result<BlockNumber>;
        async fn find_first_gap(&self, start: BlockNumber, end: BlockNumber) -> Result<Option<BlockNumber>>;
        async fn check_connection(&self) -> Result<()>;
        async fn insert_block_header(&self, block_number: BlockNumber, block_data: String) -> Result<()>;
        async fn block_exists(&self, block_number: BlockNumber) -> Result<bool>;
    }
}
