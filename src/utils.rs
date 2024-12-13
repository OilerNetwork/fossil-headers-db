use eyre::{Context, Result};

pub fn convert_hex_string_to_i64(hex_string: &str) -> Result<i64> {
    i64::from_str_radix(hex_string.trim_start_matches("0x"), 16).context("Invalid hex string")
}
