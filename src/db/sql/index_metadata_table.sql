CREATE TABLE
    IF NOT EXISTS index_metadata (
        id INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
        current_latest_block_number BIGINT NOT NULL,
        indexing_starting_block_number BIGINT NOT NULL,
        is_backfilling BOOLEAN DEFAULT TRUE,
        updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
    )