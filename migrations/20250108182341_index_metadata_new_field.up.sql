-- Add up migration script here
ALTER TABLE index_metadata ADD COLUMN backfilling_block_number BIGINT;