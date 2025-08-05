-- Add down migration script here
-- Remove requests_hash column from blockheaders table
ALTER TABLE blockheaders DROP COLUMN requests_hash;