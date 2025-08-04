//! Property-based tests for blockchain types and validation functions
//!
//! This module contains property tests that verify the correctness of
//! type validation and conversion functions across a wide range of inputs.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::arithmetic_side_effects)]

use crate::types::{Address, BlockHash, BlockNumber, TransactionHash};
use proptest::prelude::*;

/// Generate valid hex characters (0-9, a-f, A-F)
fn hex_char() -> impl Strategy<Value = char> {
    prop_oneof![
        Just('0'),
        Just('1'),
        Just('2'),
        Just('3'),
        Just('4'),
        Just('5'),
        Just('6'),
        Just('7'),
        Just('8'),
        Just('9'),
        Just('a'),
        Just('b'),
        Just('c'),
        Just('d'),
        Just('e'),
        Just('f'),
        Just('A'),
        Just('B'),
        Just('C'),
        Just('D'),
        Just('E'),
        Just('F'),
    ]
}

/// Generate a hex string of a specific length
fn hex_string(len: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(hex_char(), len).prop_map(|chars| chars.into_iter().collect())
}

/// Generate a hex string with optional 0x prefix
fn hex_string_with_prefix(len: usize) -> impl Strategy<Value = String> {
    (hex_string(len), prop::bool::ANY).prop_map(|(hex, with_prefix)| {
        if with_prefix {
            format!("0x{}", hex)
        } else {
            hex
        }
    })
}

/// Generate valid block numbers (non-negative, within reasonable bounds)
fn valid_block_number() -> impl Strategy<Value = i64> {
    0..=1_000_000_000i64
}

/// Generate invalid block numbers (negative or too large)
fn invalid_block_number() -> impl Strategy<Value = i64> {
    prop_oneof![i64::MIN..=-1, (i64::MAX - 999)..=i64::MAX,]
}

/// Generate non-hex characters
fn non_hex_char() -> impl Strategy<Value = char> {
    any::<char>().prop_filter("Must not be hex", |c| !c.is_ascii_hexdigit())
}

/// Generate strings with invalid hex characters
fn invalid_hex_string(len: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(prop_oneof![hex_char(), non_hex_char()], len)
        .prop_filter("Must contain at least one non-hex char", |chars| {
            chars.iter().any(|c| !c.is_ascii_hexdigit())
        })
        .prop_map(|chars| chars.into_iter().collect())
}

proptest! {
    /// Test that valid block numbers can always be created and round-trip correctly
    #[test]
    fn prop_block_number_valid_creation(value in valid_block_number()) {
        let block_num = BlockNumber::new(value).unwrap();
        prop_assert_eq!(block_num.value(), value);

        // Test round-trip through Display/FromStr
        let displayed = block_num.to_string();
        let parsed: BlockNumber = displayed.parse().unwrap();
        prop_assert_eq!(parsed.value(), value);

        // Test conversion to i64
        let as_i64: i64 = block_num.into();
        prop_assert_eq!(as_i64, value);
    }

    /// Test that invalid block numbers always fail validation
    #[test]
    fn prop_block_number_invalid_creation(value in invalid_block_number()) {
        prop_assert!(BlockNumber::new(value).is_err());
    }

    /// Test that block number hex parsing works for valid hex strings
    #[test]
    fn prop_block_number_hex_parsing(
        value in 0u64..=0xFFFFFFFFFFFFFF,
        with_prefix in prop::bool::ANY
    ) {
        let hex_str = if with_prefix {
            format!("0x{:x}", value)
        } else {
            format!("{:x}", value)
        };

        let block_num = BlockNumber::from_hex(&hex_str).unwrap();
        prop_assert_eq!(block_num.value(), value as i64);
    }

    /// Test that invalid hex strings fail block number parsing
    #[test]
    fn prop_block_number_invalid_hex(invalid_hex in "[g-z]+") {
        prop_assert!(BlockNumber::from_hex(&invalid_hex).is_err());
        let prefixed = format!("0x{}", &invalid_hex);
        prop_assert!(BlockNumber::from_hex(&prefixed).is_err());
    }

    /// Test that valid 64-character hex strings create valid block hashes
    #[test]
    fn prop_block_hash_valid_creation(hex in hex_string_with_prefix(64)) {
        let block_hash = BlockHash::new(hex.clone()).unwrap();
        prop_assert_eq!(block_hash.value(), &hex);

        // Test round-trip through Display/FromStr
        let displayed = block_hash.to_string();
        let parsed: BlockHash = displayed.parse().unwrap();
        prop_assert_eq!(parsed.value(), &hex);

        // Test conversion to String
        let as_string: String = block_hash.into();
        prop_assert_eq!(as_string, hex);
    }

    /// Test that invalid length hex strings fail block hash creation
    #[test]
    fn prop_block_hash_invalid_length(
        short_hex in hex_string_with_prefix(32),  // Too short
        long_hex in hex_string_with_prefix(128)   // Too long
    ) {
        prop_assert!(BlockHash::new(short_hex).is_err());
        prop_assert!(BlockHash::new(long_hex).is_err());
    }

    /// Test that strings with non-hex characters fail block hash creation
    #[test]
    fn prop_block_hash_invalid_chars(invalid_hex in invalid_hex_string(64)) {
        prop_assert!(BlockHash::new(invalid_hex.clone()).is_err());
        let prefixed = format!("0x{}", &invalid_hex);
        prop_assert!(BlockHash::new(prefixed).is_err());
    }

    /// Test that valid 64-character hex strings create valid transaction hashes
    #[test]
    fn prop_transaction_hash_valid_creation(hex in hex_string_with_prefix(64)) {
        let tx_hash = TransactionHash::new(hex.clone()).unwrap();
        prop_assert_eq!(tx_hash.value(), &hex);

        // Test round-trip through Display/FromStr
        let displayed = tx_hash.to_string();
        let parsed: TransactionHash = displayed.parse().unwrap();
        prop_assert_eq!(parsed.value(), &hex);

        // Test conversion to String
        let as_string: String = tx_hash.into();
        prop_assert_eq!(as_string, hex);
    }

    /// Test that strings with non-hex characters fail transaction hash creation
    #[test]
    fn prop_transaction_hash_invalid_chars(invalid_hex in invalid_hex_string(64)) {
        prop_assert!(TransactionHash::new(invalid_hex.clone()).is_err());
        let prefixed = format!("0x{}", &invalid_hex);
        prop_assert!(TransactionHash::new(prefixed).is_err());
    }

    /// Test that valid 40-character hex strings create valid addresses
    #[test]
    fn prop_address_valid_creation(hex in hex_string_with_prefix(40)) {
        let address = Address::new(hex.clone()).unwrap();
        prop_assert_eq!(address.value(), &hex);

        // Test round-trip through Display/FromStr
        let displayed = address.to_string();
        let parsed: Address = displayed.parse().unwrap();
        prop_assert_eq!(parsed.value(), &hex);

        // Test conversion to String
        let as_string: String = address.into();
        prop_assert_eq!(as_string, hex);
    }

    /// Test that strings with non-hex characters fail address creation
    #[test]
    fn prop_address_invalid_chars(invalid_hex in invalid_hex_string(40)) {
        prop_assert!(Address::new(invalid_hex.clone()).is_err());
        let prefixed = format!("0x{}", &invalid_hex);
        prop_assert!(Address::new(prefixed).is_err());
    }

    /// Test that block number arithmetic operations maintain validity
    #[test]
    fn prop_block_number_arithmetic(
        a in 0i64..=1000000,
        b in 0i64..=1000000
    ) {
        let block_a = BlockNumber::new(a).unwrap();
        let block_b = BlockNumber::new(b).unwrap();

        // Test addition (with overflow protection)
        if a.checked_add(b).is_some() {
            let sum = block_a + b;
            prop_assert_eq!(sum.value(), a + b);
        }

        // Test subtraction (with underflow protection)
        if a >= b {
            let diff = block_a - b;
            prop_assert_eq!(diff.value(), a - b);

            let diff2 = block_a - block_b;
            prop_assert_eq!(diff2.value(), a - b);
        }
    }

    /// Test that trusted constructors bypass validation but produce equivalent results for valid inputs
    #[test]
    fn prop_trusted_constructors_equivalence(
        block_num in valid_block_number(),
        hex_64 in hex_string_with_prefix(64),
        hex_40 in hex_string_with_prefix(40)
    ) {
        // BlockNumber
        let validated = BlockNumber::new(block_num).unwrap();
        let trusted = BlockNumber::from_trusted(block_num);
        prop_assert_eq!(validated.value(), trusted.value());

        // BlockHash
        let validated_hash = BlockHash::new(hex_64.clone()).unwrap();
        let trusted_hash = BlockHash::from_trusted(hex_64);
        prop_assert_eq!(validated_hash.value(), trusted_hash.value());

        // Address
        let validated_addr = Address::new(hex_40.clone()).unwrap();
        let trusted_addr = Address::from_trusted(hex_40);
        prop_assert_eq!(validated_addr.value(), trusted_addr.value());
    }

    /// Test batch validation of multiple blockchain types
    #[test]
    fn prop_batch_type_validation(
        block_numbers in prop::collection::vec(valid_block_number(), 1..=10),
        hashes in prop::collection::vec(hex_string_with_prefix(64), 1..=10),
        addresses in prop::collection::vec(hex_string_with_prefix(40), 1..=10)
    ) {
        // All block numbers should validate
        for &num in &block_numbers {
            prop_assert!(BlockNumber::new(num).is_ok());
        }

        // All hashes should validate
        for hash in &hashes {
            prop_assert!(BlockHash::new(hash.clone()).is_ok());
            prop_assert!(TransactionHash::new(hash.clone()).is_ok());
        }

        // All addresses should validate
        for addr in &addresses {
            prop_assert!(Address::new(addr.clone()).is_ok());
        }
    }

    /// Test that block ranges are always valid and properly ordered
    #[test]
    fn prop_block_range_validity(
        start in 0i64..=1000000,
        range in 1i64..=1000
    ) {
        let start_block = BlockNumber::from_trusted(start);
        let end_block = BlockNumber::from_trusted(start + range);

        prop_assert!(start_block.value() >= 0);
        prop_assert!(end_block.value() >= 0);
        prop_assert!(end_block.value() >= start_block.value());

        // Test range calculation
        let diff = end_block - start_block;
        prop_assert!(diff.value() >= 0);
        prop_assert_eq!(diff.value(), range);
    }
}

/// Additional tests for edge cases and boundary conditions
#[cfg(test)]
mod edge_case_tests {
    use super::*;

    #[test]
    fn test_empty_strings() {
        assert!(BlockHash::new(String::new()).is_err());
        assert!(TransactionHash::new(String::new()).is_err());
        assert!(Address::new(String::new()).is_err());
        assert!(BlockNumber::from_hex("").is_err());
    }

    #[test]
    fn test_only_prefix() {
        assert!(BlockHash::new("0x".to_string()).is_err());
        assert!(TransactionHash::new("0x".to_string()).is_err());
        assert!(Address::new("0x".to_string()).is_err());
        assert!(BlockNumber::from_hex("0x").is_err());
    }

    #[test]
    fn test_case_insensitive_hex() {
        // Test mixed case hex strings
        let mixed_case_hash = "0x1234567890ABCdef1234567890abcDEF1234567890AbCdEf1234567890ABCDEF";
        assert!(BlockHash::new(mixed_case_hash.to_string()).is_ok());
        assert!(TransactionHash::new(mixed_case_hash.to_string()).is_ok());

        let mixed_case_addr = "0x1234567890ABCdef1234567890abcDEF12345678";
        assert!(Address::new(mixed_case_addr.to_string()).is_ok());
    }

    #[test]
    fn test_block_number_edge_values() {
        // Test zero
        assert!(BlockNumber::new(0).is_ok());

        // Test maximum safe value
        let max_safe = i64::MAX - 1000;
        assert!(BlockNumber::new(max_safe).is_ok());

        // Test just over maximum safe value
        assert!(BlockNumber::new(max_safe + 1).is_err());
    }
}
