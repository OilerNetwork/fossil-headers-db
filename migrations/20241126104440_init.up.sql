-- Add up migration script here
CREATE TABLE
    IF NOT EXISTS blockheaders (
        block_hash CHAR(66) UNIQUE,
        number BIGINT PRIMARY KEY,
        gas_limit BIGINT NOT NULL,
        gas_used BIGINT NOT NULL,
        base_fee_per_gas VARCHAR(78),
        nonce VARCHAR(78) NOT NULL,
        transaction_root CHAR(66),
        receipts_root CHAR(66),
        state_root CHAR(66),
        parent_hash VARCHAR(66),
        miner VARCHAR(42),
        logs_bloom VARCHAR(1024),
        difficulty VARCHAR(78),
        totalDifficulty VARCHAR(78),
        sha3_uncles VARCHAR(66),
        timestamp VARCHAR(100),
        extra_data VARCHAR(1024),
        mix_hash VARCHAR(66),
        withdrawals_root VARCHAR(66),
        blob_gas_used VARCHAR(78),
        excess_blob_gas VARCHAR(78),
        parent_beacon_block_root VARCHAR(66)
    );

CREATE TABLE
    IF NOT EXISTS transactions (
        block_number BIGINT REFERENCES blockheaders (number),
        transaction_hash CHAR(66) PRIMARY KEY,
        from_addr CHAR(42),
        to_addr CHAR(42),
        value VARCHAR(78) NOT NULL,
        gas_price VARCHAR(78) NOT NULL,
        max_priority_fee_per_gas VARCHAR(78),
        max_fee_per_gas VARCHAR(78),
        transaction_index INTEGER NOT NULL,
        gas VARCHAR(78) NOT NULL,
        chain_id VARCHAR(78)
    );