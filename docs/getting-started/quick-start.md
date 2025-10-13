# Quick Start Guide

Get the Fossil Headers DB indexer running in 5 minutes. This guide assumes you've already completed the [Installation](installation.md).

## Prerequisites Check

Before starting, verify:

```bash
# Rust toolchain
cargo --version  # Should show 1.70+

# Docker (if using Docker-based development)
docker --version
docker compose version

# PostgreSQL (if not using Docker)
psql --version   # Should show 16+
```

## 5-Minute Setup

### Step 1: Start the Environment (1 minute)

```bash
cd fossil-headers-db
make dev-up
```

**What this does:**
- Creates Docker network
- Starts PostgreSQL 16 container
- Runs database migrations automatically
- Database ready on `localhost:5432`

**Expected output:**
```
Creating network if it doesn't exist...
[+] Running 2/2
 ✔ Network fossil-network  Created
 ✔ Container postgres      Started
Database available at localhost:5432
Waiting for database to be ready...
Running migrations...
Applied 6 migrations
Development environment ready!
```

### Step 2: Configure RPC Endpoint (1 minute)

Create `.env` file:

```bash
cat > .env << EOF
DB_CONNECTION_STRING=postgresql://postgres:postgres@localhost:5432/postgres
NODE_CONNECTION_STRING=https://eth-mainnet.g.alchemy.com/v2/YOUR_API_KEY
ROUTER_ENDPOINT=0.0.0.0:3000
RUST_LOG=info
INDEX_TRANSACTIONS=false
START_BLOCK_OFFSET=1024
IS_DEV=true
EOF
```

**Replace `YOUR_API_KEY`** with your Ethereum RPC provider API key:
- [Alchemy](https://www.alchemy.com/) (Free tier: 300M compute units/month)
- [Infura](https://www.infura.io/) (Free tier: 100K requests/day)
- [QuickNode](https://www.quicknode.com/)

### Step 3: Run the Indexer (1 minute)

```bash
make run-indexer
```

**Expected output:**
```
Starting modern indexer service...
Connecting to DB
Run migrations
Starting Indexer
[router] Starting router service
[quick_index] Starting quick indexer service
[batch_index] Starting batch indexer service
[quick_index] Latest finalized block: 20000000
[batch_index] Backfilling from block 19999000 to 0
```

The indexer is now running! Keep this terminal open.

### Step 4: Verify Operation (1 minute)

Open a **new terminal** and check the health endpoint:

```bash
curl http://localhost:3000/health
```

**Expected response:**
```json
{
  "status": "healthy",
  "timestamp": "2025-10-14T12:00:00.000Z"
}
```

Check the database for indexed blocks:

```bash
psql postgresql://postgres:postgres@localhost:5432/postgres \
  -c "SELECT COUNT(*) as total_blocks, MIN(number) as first_block, MAX(number) as latest_block FROM block_header;"
```

**Expected output:**
```
 total_blocks | first_block | latest_block
--------------+-------------+--------------
          42  |    19999958 |    20000000
(1 row)
```

The indexer is working! Block counts will grow as indexing continues.

### Step 5: Monitor Progress (1 minute)

Watch the indexer logs to see real-time progress:

**Quick Indexer** (real-time sync):
```
[quick_index] Indexed block 20000001
[quick_index] Indexed block 20000002
[quick_index] Waiting for new finalized blocks...
```

**Batch Indexer** (historical backfill):
```
[batch_index] Indexed batch: blocks 19999000-19998000 (1000 blocks in 12.3s)
[batch_index] Indexed batch: blocks 19998000-19997000 (1000 blocks in 11.8s)
[batch_index] Backfill progress: 2000/19999000 blocks (0.01%)
```

**Performance**: Expect 50-100 blocks/second during backfilling depending on your RPC provider and hardware.

## Understanding the Indexer

The Fossil Headers DB runs **two concurrent services**:

### Quick Indexer
- **Purpose**: Real-time synchronization
- **Strategy**: Polls for new finalized blocks every 10 seconds
- **Target**: Latest block → forward
- **Speed**: ~6-12 seconds per block (matches Ethereum finality)

### Batch Indexer
- **Purpose**: Historical backfilling and gap filling
- **Strategy**: Processes blocks in batches of 1000
- **Target**: `START_BLOCK_OFFSET` → genesis (block 0)
- **Speed**: 50-100 blocks/second (RPC limited)

Both services run simultaneously to ensure:
- New blocks are indexed immediately (quick)
- Historical blocks are filled in the background (batch)
- No gaps in the block sequence

## Monitoring Indexer Progress

### Check Latest Indexed Block

```bash
psql postgresql://postgres:postgres@localhost:5432/postgres \
  -c "SELECT MAX(number) as latest_block FROM block_header;"
```

### Check Backfill Status

```bash
psql postgresql://postgres:postgres@localhost:5432/postgres \
  -c "SELECT
        current_latest_block_number as quick_indexer_position,
        backfilling_block_number as batch_indexer_position,
        is_backfilling,
        indexing_starting_block_number as target_block
      FROM index_metadata;"
```

**Example output:**
```
 quick_indexer_position | batch_indexer_position | is_backfilling | target_block
------------------------+------------------------+----------------+--------------
               20000002 |               19995000 | t              |            0
```

**Interpretation:**
- Quick indexer is at block 20,000,002 (near tip)
- Batch indexer is backfilling at block 19,995,000
- Backfilling is active (`t` = true)
- Target is block 0 (genesis)

### Check for Gaps

```bash
psql postgresql://postgres:postgres@localhost:5432/postgres \
  -c "SELECT number
      FROM generate_series(0, (SELECT MAX(number) FROM block_header)) AS number
      WHERE NOT EXISTS (SELECT 1 FROM block_header WHERE block_header.number = number.number)
      LIMIT 10;"
```

If this returns rows, those block numbers are missing and will be filled by the batch indexer.

## Stopping the Indexer

### Graceful Shutdown

In the terminal running the indexer, press **Ctrl+C**:

```
^C
Received Ctrl+C
Waiting for current processes to finish...
[router] Router service completed normally
[quick_index] Quick indexer completed normally
[batch_index] Batch indexer completed normally
All indexing services completed successfully
```

### Stop Database

```bash
make dev-down
```

This stops the PostgreSQL container but **preserves data** in Docker volumes.

### Clean Environment (Delete Data)

```bash
make dev-clean
```

**Warning**: This removes all indexed data. Use only when you want to start fresh.

## Next Steps

Now that your indexer is running:

### Customize Configuration
- Adjust indexing speed: [Configuration Guide](configuration.md)
- Enable transaction indexing
- Configure logging levels

### Integration with Light Client
- Learn how the Light Client uses indexed data: [Light Client Integration](../integration/light-client.md)
- Understand the data flow: [Architecture Overview](../architecture/overview.md)

### Production Deployment
- Deploy to AWS ECS: [Deployment Guide](../guides/deployment.md)
- Set up monitoring: [Monitoring Guide](../guides/monitoring.md)
- Configure backups: [Database Management](../guides/database-management.md)

### Development
- Run tests: [Testing Guide](testing.md)
- Contribute code: [Development Workflow](../guides/local-development.md)

## Common Quick Start Issues

### Issue: Cannot connect to RPC endpoint

```
Error: RPC request failed: connection timeout
```

**Solution**:
1. Verify your RPC endpoint is correct in `.env`
2. Test the endpoint:
   ```bash
   curl -X POST -H "Content-Type: application/json" \
     --data '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
     YOUR_RPC_ENDPOINT
   ```
3. Check your API key is valid and has remaining quota

### Issue: Database connection refused

```
Error: Failed to connect to database
```

**Solution**:
```bash
# Check if PostgreSQL container is running
docker ps | grep postgres

# If not running, start it
make dev-up

# Test connection manually
psql postgresql://postgres:postgres@localhost:5432/postgres -c "SELECT 1;"
```

### Issue: Indexer seems slow

**Symptoms**: Only a few blocks/second during backfilling

**Causes and Solutions**:

1. **RPC rate limiting**
   - Symptom: Many timeout/429 errors in logs
   - Solution: Use premium RPC tier or dedicated node

2. **Network latency**
   - Symptom: High RPC response times
   - Solution: Choose RPC provider closer to your region

3. **Database I/O bottleneck**
   - Symptom: High disk wait times
   - Solution: Use SSD storage, increase PostgreSQL work_mem

4. **Conservative default settings**
   - Solution: Increase batch size in configuration:
     ```rust
     // Programmatic configuration
     index_batch_size(2000)  // Default is 1000
     ```

### Issue: Indexer exits unexpectedly

**Check logs for errors**:
```bash
# Last 100 lines
docker compose -f docker/docker-compose.local.yml logs --tail 100

# Follow logs in real-time
docker compose -f docker/docker-compose.local.yml logs -f indexer
```

**Common causes**:
- RPC endpoint became unavailable
- Database connection lost
- Insufficient disk space
- API key quota exceeded

## Testing the Installation

Run the test suite to verify everything works:

```bash
# Run all tests
make test

# Run specific test categories
cargo test --lib          # Unit tests
cargo test --test '*'     # Integration tests

# Run with verbose output
cargo test -- --nocapture
```

**Expected**: All tests pass with green output.

## Quick Reference Commands

```bash
# Start environment
make dev-up

# Run indexer
make run-indexer

# Check health
curl http://localhost:3000/health

# Stop environment
make dev-down

# Clean everything
make dev-clean

# Run tests
make test

# Format code
make format

# Run linter
make lint

# Build release binary
make build
```

## Need Help?

- **Documentation**: Browse the [full docs](../)
- **Troubleshooting**: See [Troubleshooting Guide](../guides/troubleshooting.md)
- **Configuration**: See [Configuration Guide](configuration.md)
- **Issues**: Report on [GitHub Issues](https://github.com/OilerNetwork/fossil-headers-db/issues)
