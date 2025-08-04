#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::arithmetic_side_effects)]
#[allow(unused_imports)]
mod unit_tests {
    use super::super::*;

    #[tokio::test]
    async fn test_try_convert_full_tx_vector_success() {
        let transaction = Transaction {
            hash: "0x123".to_string(),
            block_number: "0x1".to_string(),
            transaction_index: "0x0".to_string(),
            value: "0x0".to_string(),
            gas_price: Some("0x1".to_string()),
            gas: "0x5208".to_string(),
            from: Some("0xabc".to_string()),
            to: Some("0xdef".to_string()),
            max_priority_fee_per_gas: None,
            max_fee_per_gas: None,
            chain_id: Some("0x1".to_string()),
        };

        let block_transactions = vec![BlockTransaction::Full(Box::new(transaction.clone()))];
        let result = try_convert_full_tx_vector(block_transactions);

        assert!(result.is_ok());
        let converted = result.unwrap();
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].hash, transaction.hash);
        assert_eq!(converted[0].block_number, transaction.block_number);
    }

    #[tokio::test]
    async fn test_try_convert_full_tx_vector_with_hash_should_fail() {
        let block_transactions = vec![BlockTransaction::Hash("0x123".to_string())];
        let result = try_convert_full_tx_vector(block_transactions);

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_try_convert_full_tx_vector_empty_input() {
        let result = try_convert_full_tx_vector(vec![]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_block_transaction_try_from_full() {
        let transaction = Transaction {
            hash: "0x789".to_string(),
            block_number: "0x2".to_string(),
            transaction_index: "0x0".to_string(),
            value: "0x100".to_string(),
            gas_price: Some("0x2".to_string()),
            gas: "0x5208".to_string(),
            from: Some("0x111".to_string()),
            to: Some("0x222".to_string()),
            max_priority_fee_per_gas: Some("0x1".to_string()),
            max_fee_per_gas: Some("0x3".to_string()),
            chain_id: Some("0x1".to_string()),
        };

        let block_tx = BlockTransaction::Full(Box::new(transaction.clone()));
        let result: Result<Transaction> = block_tx.try_into();

        assert!(result.is_ok());
        let converted = result.unwrap();
        assert_eq!(converted.hash, transaction.hash);
        assert_eq!(converted.block_number, transaction.block_number);
        assert_eq!(converted.value, transaction.value);
        assert_eq!(
            converted.max_priority_fee_per_gas,
            transaction.max_priority_fee_per_gas
        );
        assert_eq!(converted.max_fee_per_gas, transaction.max_fee_per_gas);
    }

    #[tokio::test]
    async fn test_block_transaction_try_from_hash_should_fail() {
        let block_tx = BlockTransaction::Hash("0xabc123".to_string());
        let result: Result<Transaction> = block_tx.try_into();

        assert!(result.is_err());
    }
}
