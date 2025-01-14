-- Add up migration script here
-- Dropping foreign key constraints here as it causes extra checks during insertion and updates, which might affect future performances.
ALTER TABLE transactions DROP CONSTRAINT transactions_block_number_fkey;