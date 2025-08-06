-- Add up migration script here
-- Add requests_hash column to blockheaders table
ALTER TABLE blockheaders ADD COLUMN requests_hash VARCHAR(66);