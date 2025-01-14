-- Add down migration script here
ALTER TABLE transactions ADD CONSTRAINT transactions_block_number_fkey FOREIGN KEY (block_number) REFERENCES blockheaders (number) ON DELETE CASCADE;