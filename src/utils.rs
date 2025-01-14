use eyre::{Context, Result};

pub fn convert_hex_string_to_i64(hex_string: &str) -> Result<i64> {
    i64::from_str_radix(hex_string.trim_start_matches("0x"), 16).context("Invalid hex string")
}

pub fn convert_hex_string_to_i32(hex_string: &str) -> Result<i32> {
    i32::from_str_radix(hex_string.trim_start_matches("0x"), 16).context("Invalid hex string")
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
