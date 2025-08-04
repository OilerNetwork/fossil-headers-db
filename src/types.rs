use crate::errors::{BlockchainError, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, Sub};
use std::str::FromStr;

/// A blockchain block number with validation
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BlockNumber(i64);

impl BlockNumber {
    /// Creates a new `BlockNumber` with validation
    pub fn new(value: i64) -> Result<Self> {
        if value < 0 {
            return Err(BlockchainError::invalid_format(
                "block_number",
                &format!("Block number cannot be negative: {value}"),
            ));
        }
        if value > i64::MAX - 1000 {
            return Err(BlockchainError::invalid_format(
                "block_number",
                &format!("Block number too large: {value}"),
            ));
        }
        Ok(Self(value))
    }

    /// Creates a `BlockNumber` without validation (for trusted sources)
    #[must_use]
    pub const fn from_trusted(value: i64) -> Self {
        Self(value)
    }

    /// Gets the inner value
    #[must_use]
    pub const fn value(&self) -> i64 {
        self.0
    }

    /// Creates a `BlockNumber` from a hex string
    pub fn from_hex(hex: &str) -> Result<Self> {
        let cleaned = hex.strip_prefix("0x").unwrap_or(hex);
        let value = i64::from_str_radix(cleaned, 16).map_err(|e| {
            BlockchainError::invalid_format(
                "block_number",
                &format!("Invalid hex block number '{hex}': {e}"),
            )
        })?;
        Self::new(value)
    }
}

impl fmt::Display for BlockNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<BlockNumber> for i64 {
    fn from(block_number: BlockNumber) -> Self {
        block_number.0
    }
}

impl FromStr for BlockNumber {
    type Err = BlockchainError;

    fn from_str(s: &str) -> Result<Self> {
        let value = s.parse::<i64>().map_err(|e| {
            BlockchainError::invalid_format(
                "block_number",
                &format!("Invalid block number '{s}': {e}"),
            )
        })?;
        Self::new(value)
    }
}

impl Add<i64> for BlockNumber {
    type Output = Self;

    fn add(self, other: i64) -> Self {
        Self::from_trusted(self.0 + other)
    }
}

impl Sub<i64> for BlockNumber {
    type Output = Self;

    fn sub(self, other: i64) -> Self {
        Self::from_trusted(self.0 - other)
    }
}

impl Sub<Self> for BlockNumber {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self::from_trusted(self.0 - other.0)
    }
}

/// A blockchain block hash with hex validation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockHash(String);

impl BlockHash {
    /// Creates a new `BlockHash` with validation
    pub fn new(value: String) -> Result<Self> {
        Self::validate_hex_hash(&value, "block_hash")?;
        Ok(Self(value))
    }

    /// Creates a `BlockHash` without validation (for trusted sources)
    #[must_use]
    pub const fn from_trusted(value: String) -> Self {
        Self(value)
    }

    /// Gets the inner value
    #[must_use]
    pub fn value(&self) -> &str {
        &self.0
    }

    /// Gets the inner value as owned String
    #[must_use]
    pub fn into_value(self) -> String {
        self.0
    }

    fn validate_hex_hash(value: &str, field_name: &str) -> Result<()> {
        let cleaned = value.strip_prefix("0x").unwrap_or(value);

        if cleaned.len() != 64 {
            return Err(BlockchainError::invalid_format(
                field_name,
                &format!(
                    "Hash must be 64 hex characters (got {}): {}",
                    cleaned.len(),
                    value
                ),
            ));
        }

        if !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(BlockchainError::invalid_format(
                field_name,
                &format!("Hash contains non-hex characters: {value}"),
            ));
        }

        Ok(())
    }
}

impl fmt::Display for BlockHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<BlockHash> for String {
    fn from(block_hash: BlockHash) -> Self {
        block_hash.0
    }
}

impl FromStr for BlockHash {
    type Err = BlockchainError;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s.to_string())
    }
}

/// A blockchain transaction hash with hex validation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransactionHash(String);

impl TransactionHash {
    /// Creates a new `TransactionHash` with validation
    pub fn new(value: String) -> Result<Self> {
        Self::validate_hex_hash(&value, "transaction_hash")?;
        Ok(Self(value))
    }

    /// Creates a `TransactionHash` without validation (for trusted sources)
    #[must_use]
    pub const fn from_trusted(value: String) -> Self {
        Self(value)
    }

    /// Gets the inner value
    #[must_use]
    pub fn value(&self) -> &str {
        &self.0
    }

    /// Gets the inner value as owned String
    #[must_use]
    pub fn into_value(self) -> String {
        self.0
    }

    fn validate_hex_hash(value: &str, field_name: &str) -> Result<()> {
        let cleaned = value.strip_prefix("0x").unwrap_or(value);

        if cleaned.len() != 64 {
            return Err(BlockchainError::invalid_format(
                field_name,
                &format!(
                    "Hash must be 64 hex characters (got {}): {}",
                    cleaned.len(),
                    value
                ),
            ));
        }

        if !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(BlockchainError::invalid_format(
                field_name,
                &format!("Hash contains non-hex characters: {value}"),
            ));
        }

        Ok(())
    }
}

impl fmt::Display for TransactionHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<TransactionHash> for String {
    fn from(transaction_hash: TransactionHash) -> Self {
        transaction_hash.0
    }
}

impl FromStr for TransactionHash {
    type Err = BlockchainError;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s.to_string())
    }
}

/// An Ethereum address with format validation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Address(String);

impl Address {
    /// Creates a new Address with validation
    pub fn new(value: String) -> Result<Self> {
        Self::validate_address(&value)?;
        Ok(Self(value))
    }

    /// Creates an Address without validation (for trusted sources)
    #[must_use]
    pub const fn from_trusted(value: String) -> Self {
        Self(value)
    }

    /// Gets the inner value
    #[must_use]
    pub fn value(&self) -> &str {
        &self.0
    }

    /// Gets the inner value as owned String
    #[must_use]
    pub fn into_value(self) -> String {
        self.0
    }

    fn validate_address(value: &str) -> Result<()> {
        let cleaned = value.strip_prefix("0x").unwrap_or(value);

        if cleaned.len() != 40 {
            return Err(BlockchainError::invalid_format(
                "address",
                &format!(
                    "Address must be 40 hex characters (got {}): {}",
                    cleaned.len(),
                    value
                ),
            ));
        }

        if !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(BlockchainError::invalid_format(
                "address",
                &format!("Address contains non-hex characters: {value}"),
            ));
        }

        Ok(())
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Address> for String {
    fn from(address: Address) -> Self {
        address.0
    }
}

impl FromStr for Address {
    type Err = BlockchainError;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_number_valid() {
        let bn = BlockNumber::new(12345).unwrap();
        assert_eq!(bn.value(), 12345);
    }

    #[test]
    fn test_block_number_negative() {
        assert!(BlockNumber::new(-1).is_err());
    }

    #[test]
    fn test_block_number_from_hex() {
        let bn = BlockNumber::from_hex("0x1a").unwrap();
        assert_eq!(bn.value(), 26);
    }

    #[test]
    fn test_block_hash_valid() {
        let hash = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        let bh = BlockHash::new(hash.to_string()).unwrap();
        assert_eq!(bh.value(), hash);
    }

    #[test]
    fn test_block_hash_invalid_length() {
        let hash = "0x1234";
        assert!(BlockHash::new(hash.to_string()).is_err());
    }

    #[test]
    fn test_block_hash_invalid_chars() {
        let hash = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdeg";
        assert!(BlockHash::new(hash.to_string()).is_err());
    }

    #[test]
    fn test_transaction_hash_valid() {
        let hash = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        let th = TransactionHash::new(hash.to_string()).unwrap();
        assert_eq!(th.value(), hash);
    }

    #[test]
    fn test_address_valid() {
        let addr = "0x1234567890123456789012345678901234567890";
        let address = Address::new(addr.to_string()).unwrap();
        assert_eq!(address.value(), addr);
    }

    #[test]
    fn test_address_invalid_length() {
        let addr = "0x1234";
        assert!(Address::new(addr.to_string()).is_err());
    }

    #[test]
    fn test_address_invalid_chars() {
        let addr = "0x123456789012345678901234567890123456789g";
        assert!(Address::new(addr.to_string()).is_err());
    }
}
