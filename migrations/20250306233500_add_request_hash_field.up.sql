-- Add up migration script here
ALTER TABLE blockheaders ADD COLUMN requests_hash TEXT;
