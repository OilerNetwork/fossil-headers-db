#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::arithmetic_side_effects)]
#[allow(unused_imports)]
mod unit_tests {
    use super::*;
    use crate::db::DB_MAX_CONNECTIONS;
    use crate::utils::convert_hex_string_to_i64;

    mod hex_conversion_tests {
        use super::*;

        #[test]
        fn test_convert_hex_string_to_i64_valid() {
            let hex_str = "0x1a2b3c";
            let result = convert_hex_string_to_i64(hex_str);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 1715004);
        }

        #[test]
        fn test_convert_hex_string_to_i64_without_prefix() {
            let hex_str = "1a2b3c";
            let result = convert_hex_string_to_i64(hex_str);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 1715004);
        }

        #[test]
        fn test_convert_hex_string_to_i64_zero() {
            let hex_str = "0x0";
            let result = convert_hex_string_to_i64(hex_str);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 0);
        }

        #[test]
        fn test_convert_hex_string_to_i64_max_value() {
            let hex_str = "0x7FFFFFFFFFFFFFFF";
            let result = convert_hex_string_to_i64(hex_str);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), i64::MAX);
        }

        #[test]
        fn test_convert_hex_string_to_i64_invalid_characters() {
            let hex_str = "0xGHIJ";
            let result = convert_hex_string_to_i64(hex_str);
            assert!(result.is_err());
        }

        #[test]
        fn test_convert_hex_string_to_i64_empty_string() {
            let hex_str = "";
            let result = convert_hex_string_to_i64(hex_str);
            assert!(result.is_err());
        }

        #[test]
        fn test_convert_hex_string_to_i64_just_prefix() {
            let hex_str = "0x";
            let result = convert_hex_string_to_i64(hex_str);
            assert!(result.is_err());
        }

        #[test]
        fn test_convert_hex_string_to_i64_large_value() {
            let hex_str = "0xFFFFFFFFFFFFFFFF";
            let result = convert_hex_string_to_i64(hex_str);
            assert!(result.is_err() || result.unwrap() < 0);
        }
    }

    mod database_constants_tests {
        use super::*;

        #[test]
        fn test_db_max_connections_constant() {
            assert!(DB_MAX_CONNECTIONS > 0);
            assert!(DB_MAX_CONNECTIONS <= 1000);
        }

        #[test]
        fn test_db_constants_reasonable() {
            assert!(DB_MAX_CONNECTIONS >= 10);
        }
    }
}
