pub fn convert_hex_string_to_i64(hex_string: &str) -> i64 {
    i64::from_str_radix(hex_string.trim_start_matches("0x"), 16).expect("Invalid hex string")
}
