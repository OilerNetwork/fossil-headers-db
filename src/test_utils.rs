//! # Test Utilities
//!
//! This module provides common utilities and helper functions for testing across the codebase.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::arithmetic_side_effects)]
#![allow(dead_code)]

#[cfg(test)]
pub mod test_data {
    use crate::rpc::{BlockTransaction, Transaction};
    use crate::types::BlockNumber;

    /// Creates a sample transaction with full data
    pub fn create_sample_transaction_full() -> BlockTransaction {
        let transaction = Transaction {
            hash: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string(),
            block_number: "0x1312d00".to_string(), // 20000000 in hex
            transaction_index: "0x0".to_string(),
            value: "0xde0b6b3a7640000".to_string(), // 1 ETH in wei
            gas_price: Some("0x4a817c800".to_string()), // 20 Gwei
            gas: "0x5208".to_string(),              // 21000 gas
            from: Some("0x742d35cc6969c0532b5f52aa3d3f4d7b4a4f8c7e".to_string()),
            to: Some("0x8ba1f109551bd432803012645hac136c1235a67d".to_string()),
            max_priority_fee_per_gas: Some("0x3b9aca00".to_string()), // 1 Gwei
            max_fee_per_gas: Some("0x77359400".to_string()),          // 2 Gwei
            chain_id: Some("0x1".to_string()),                        // Ethereum mainnet
        };
        BlockTransaction::Full(Box::new(transaction))
    }

    /// Creates a sample transaction hash (without full transaction data)
    pub fn create_sample_transaction_hash() -> BlockTransaction {
        BlockTransaction::Hash(
            "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string(),
        )
    }

    /// Creates a sequence of test block numbers
    pub fn create_test_block_range(start: i64, count: usize) -> Vec<BlockNumber> {
        (0..count)
            .map(|i| BlockNumber::from_trusted(start + i as i64))
            .collect()
    }

    /// Creates test data for hex conversion testing
    pub fn create_hex_test_cases() -> Vec<(&'static str, Result<i64, ()>)> {
        vec![
            ("0x0", Ok(0)),
            ("0x1", Ok(1)),
            ("0xa", Ok(10)),
            ("0xff", Ok(255)),
            ("0x1000", Ok(4096)),
            ("0x1312d00", Ok(20000000)),
            ("0x7FFFFFFFFFFFFFFF", Ok(i64::MAX)),
            ("", Err(())),                   // Empty string
            ("0x", Err(())),                 // Just prefix
            ("0xGHIJ", Err(())),             // Invalid hex chars
            ("xyz", Err(())),                // No hex prefix
            ("0xFFFFFFFFFFFFFFFF", Err(())), // Too large for i64
        ]
    }
}

#[cfg(test)]
pub mod assertions {
    use crate::types::BlockNumber;

    /// Asserts that block numbers are in ascending order
    pub fn assert_blocks_ascending(blocks: &[BlockNumber]) {
        for window in blocks.windows(2) {
            assert!(
                window[0].value() < window[1].value(),
                "Blocks not in ascending order: {} >= {}",
                window[0].value(),
                window[1].value()
            );
        }
    }

    /// Asserts that block numbers are within a specific range
    pub fn assert_blocks_in_range(blocks: &[BlockNumber], min: i64, max: i64) {
        for block in blocks {
            assert!(
                block.value() >= min && block.value() <= max,
                "Block {} is outside range [{}, {}]",
                block.value(),
                min,
                max
            );
        }
    }
}

#[cfg(test)]
pub mod test_scenarios {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    /// Creates a termination signal for testing
    pub fn create_immediate_termination() -> Arc<AtomicBool> {
        let signal = Arc::new(AtomicBool::new(false));
        signal.store(true, Ordering::Relaxed);
        signal
    }

    /// Creates a termination signal that won't terminate
    pub fn create_no_termination() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }
}
