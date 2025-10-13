# Configuration Guide

This guide explains all configuration options for the Fossil Headers DB indexer.

## Environment Variables

The indexer is configured primarily through environment variables. These can be set in:
- `.env` file (development, requires `IS_DEV=true`)
- Shell environment
- Docker/ECS task definitions
- CI/CD pipelines

### Required Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `DB_CONNECTION_STRING` | PostgreSQL connection URL | `postgresql://user:pass@localhost:5432/dbname` |
| `NODE_CONNECTION_STRING` | Ethereum RPC endpoint URL | `https://eth-mainnet.g.alchemy.com/v2/KEY` |

### Optional Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ROUTER_ENDPOINT` | `0.0.0.0:3000` | HTTP server bind address and port |
| `RUST_LOG` | `info` | Logging level (see [Logging](#logging)) |
| `INDEX_TRANSACTIONS` | `false` | Whether to index full transaction data |
| `START_BLOCK_OFFSET` | `1024` | Blocks before latest to start backfill |
| `IS_DEV` | `false` | Load `.env` file if true |

## Configuration File (.env)

### Development Environment

```bash
# .env for local development
DB_CONNECTION_STRING=postgresql://postgres:postgres@localhost:5432/postgres
NODE_CONNECTION_STRING=https://eth-mainnet.g.alchemy.com/v2/YOUR_KEY
ROUTER_ENDPOINT=0.0.0.0:3000
RUST_LOG=debug
INDEX_TRANSACTIONS=false
START_BLOCK_OFFSET=1024
IS_DEV=true
```

### Production Environment

For production, use environment variables directly (not `.env` file):

```bash
# Docker Compose
environment:
  - DB_CONNECTION_STRING=postgresql://...
  - NODE_CONNECTION_STRING=https://...
  - RUST_LOG=info
  - INDEX_TRANSACTIONS=false
  - IS_DEV=false  # Important: do not load .env in production
```

Or AWS ECS task definition:

```json
{
  "environment": [
    {"name": "DB_CONNECTION_STRING", "value": "postgresql://..."},
    {"name": "NODE_CONNECTION_STRING", "value": "https://..."},
    {"name": "RUST_LOG", "value": "info"},
    {"name": "INDEX_TRANSACTIONS", "value": "false"}
  ]
}
```

## Database Configuration

### Connection String Format

```
postgresql://[user[:password]@][host][:port][/database][?parameters]
```

**Examples:**

```bash
# Local development
DB_CONNECTION_STRING=postgresql://postgres:postgres@localhost:5432/postgres

# Docker internal network
DB_CONNECTION_STRING=postgresql://postgres:postgres@db:5432/postgres

# AWS RDS with SSL
DB_CONNECTION_STRING=postgresql://admin:password@mydb.abc.us-west-2.rds.amazonaws.com:5432/fossil?sslmode=require

# Connection pooling parameters
DB_CONNECTION_STRING=postgresql://user:pass@host:5432/db?pool_max=20&pool_timeout=30
```

### Connection Pool Settings

The indexer uses SQLx connection pooling with these defaults:

```rust
// Defined in src/db/mod.rs
pub const DB_MAX_CONNECTIONS: u32 = 100;
pub const DB_MIN_CONNECTIONS: u32 = 5;
pub const DB_ACQUIRE_TIMEOUT_SECS: u64 = 30;
```

To override, modify the connection string:

```bash
DB_CONNECTION_STRING=postgresql://...?pool_max=50&pool_min=10&pool_timeout=60
```

### SSL/TLS Configuration

For secure connections (production):

```bash
# Require SSL
DB_CONNECTION_STRING=postgresql://...?sslmode=require

# Verify CA certificate
DB_CONNECTION_STRING=postgresql://...?sslmode=verify-ca&sslrootcert=/path/to/ca.pem

# Verify full certificate
DB_CONNECTION_STRING=postgresql://...?sslmode=verify-full&sslrootcert=/path/to/ca.pem
```

## RPC Configuration

### RPC Endpoint Selection

The `NODE_CONNECTION_STRING` should point to an Ethereum mainnet RPC endpoint.

**Provider Recommendations:**

| Provider | Free Tier | Paid Plans | Best For |
|----------|-----------|------------|----------|
| **Alchemy** | 300M CU/month | From $49/month | Development & Production |
| **Infura** | 100K req/day | From $50/month | Development |
| **QuickNode** | Limited | From $9/month | Production |
| **Self-hosted** | Free | Infrastructure costs | High-volume production |

### RPC Timeout and Retries

RPC behavior is controlled programmatically via `IndexingConfig`:

```rust
// src/indexer/lib.rs
let config = IndexingConfig::builder()
    .rpc_timeout(300)        // 5 minutes per request
    .rpc_max_retries(5)      // 5 retries before failing
    .build()?;
```

**Default values** (production-ready):

| Parameter | Default | Description |
|-----------|---------|-------------|
| `rpc_timeout` | `300` seconds | Maximum time for single RPC call |
| `rpc_max_retries` | `5` | Retry attempts on failure |

**For development** (faster feedback):

```rust
let config = IndexingConfig::builder()
    .rpc_timeout(60)         // 1 minute
    .rpc_max_retries(3)      // 3 retries
    .build()?;
```

### Rate Limiting Handling

The indexer includes exponential backoff retry logic:

```rust
// Automatic retry with exponential backoff
// Retry 1: 1 second wait
// Retry 2: 2 seconds wait
// Retry 3: 4 seconds wait
// Retry 4: 8 seconds wait
// Retry 5: 16 seconds wait
```

This helps handle temporary rate limits from RPC providers.

## Indexing Configuration

### Indexing Strategy

The indexer runs two concurrent services with separate configurations:

#### Quick Indexer Configuration

```rust
// src/indexer/lib.rs - Quick Indexer
let quick_config = QuickIndexConfig::builder()
    .should_index_txs(false)       // Index transactions?
    .index_batch_size(100)         // Blocks per batch
    .max_retries(10)               // Max retry attempts
    .poll_interval(10)             // Seconds between polls
    .rpc_timeout(300)              // RPC timeout in seconds
    .build()?;
```

| Parameter | Default | Description |
|-----------|---------|-------------|
| `should_index_txs` | `false` | Include transaction data |
| `index_batch_size` | `100` | Blocks per processing batch |
| `max_retries` | `10` | Retry attempts per block |
| `poll_interval` | `10` | Seconds between finality checks |
| `rpc_timeout` | `300` | RPC call timeout (seconds) |

#### Batch Indexer Configuration

```rust
// src/indexer/lib.rs - Batch Indexer
let batch_config = BatchIndexConfig::builder()
    .should_index_txs(false)        // Index transactions?
    .index_batch_size(1000)         // Blocks per batch
    .max_retries(10)                // Max retry attempts
    .poll_interval(10)              // Seconds between gap checks
    .rpc_timeout(300)               // RPC timeout
    .max_concurrent_requests(10)    // Parallel RPC requests
    .task_timeout(300)              // Task timeout (seconds)
    .build()?;
```

| Parameter | Default | Description |
|-----------|---------|-------------|
| `should_index_txs` | `false` | Include transaction data |
| `index_batch_size` | `1000` | Blocks per batch |
| `max_retries` | `10` | Retry attempts per batch |
| `poll_interval` | `10` | Seconds between batch iterations |
| `rpc_timeout` | `300` | RPC call timeout (seconds) |
| `max_concurrent_requests` | `10` | Parallel block fetches |
| `task_timeout` | `300` | Maximum time per batch |

### Transaction Indexing

By default, only block **headers** are indexed. To index full **transaction data**:

```bash
# .env
INDEX_TRANSACTIONS=true
```

**Storage Impact:**
- Headers only: ~0.5 KB per block
- With transactions: ~5 KB per block (10x increase)

**Use Cases:**
- Enable if you need transaction hashes, from/to addresses, values, gas prices
- Required for some Light Client proof generation scenarios
- Keep disabled for header-only MMR construction

### Start Block Configuration

Control where backfilling begins:

```bash
# Start backfilling from 1024 blocks before latest
START_BLOCK_OFFSET=1024

# Start backfilling from 10000 blocks before latest
START_BLOCK_OFFSET=10000

# Start from as recent as possible
START_BLOCK_OFFSET=100
```

**How it works:**
1. Indexer fetches latest finalized block (e.g., 20,000,000)
2. Calculates start block: `20,000,000 - START_BLOCK_OFFSET`
3. Batch indexer backfills from start block down to genesis (0)
4. Quick indexer syncs from latest forward

**Recommendations:**
- **Development**: `START_BLOCK_OFFSET=1024` (fast initial indexing)
- **Production**: `START_BLOCK_OFFSET=10000` (safety margin)
- **Full historical**: Set to very large number, or let it default

## Logging Configuration

### Log Levels

Set via `RUST_LOG` environment variable:

```bash
# Global log level
RUST_LOG=info        # Production (default)
RUST_LOG=debug       # Development
RUST_LOG=trace       # Debugging
RUST_LOG=warn        # Warnings only
RUST_LOG=error       # Errors only
```

### Module-Specific Logging

Configure different log levels per module:

```bash
# Debug indexer, trace RPC calls
RUST_LOG=fossil_headers_db::indexer=debug,fossil_headers_db::rpc=trace,info

# Trace database operations, info for everything else
RUST_LOG=fossil_headers_db::db=trace,info

# Quiet most logs, only errors from RPC
RUST_LOG=error,fossil_headers_db::rpc=error
```

### Log Format

The indexer uses structured logging with these fields:

```
[2025-10-14T12:00:00Z INFO  fossil_headers_db::indexer] Starting indexer service
[timestamp]           [level] [module]                   [message]
```

**Log Levels:**
- `ERROR`: Critical failures requiring immediate attention
- `WARN`: Issues that don't stop operation but need investigation
- `INFO`: Key operational milestones and progress updates
- `DEBUG`: Detailed execution flow for troubleshooting
- `TRACE`: Very verbose internal state for deep debugging

### Production Logging Best Practices

```bash
# Production: Info level, structured output
RUST_LOG=info

# Enable debug for specific issues
RUST_LOG=fossil_headers_db::indexer::batch_service=debug,info

# CloudWatch/Datadog: Use info level with JSON formatting (future)
RUST_LOG=info
LOG_FORMAT=json  # Not yet implemented, but planned
```

## HTTP Server Configuration

### Router Endpoint

```bash
# Listen on all interfaces, port 3000 (default)
ROUTER_ENDPOINT=0.0.0.0:3000

# Listen on localhost only
ROUTER_ENDPOINT=127.0.0.1:3000

# Custom port
ROUTER_ENDPOINT=0.0.0.0:8080
```

**Production Considerations:**
- Use `0.0.0.0` to accept connections from load balancers
- In ECS, ensure port mapping matches this configuration
- Health check endpoint will be available at `http://<ROUTER_ENDPOINT>/health`

## Configuration Presets

### Development Preset

Fast feedback, verbose logging, local resources:

```bash
# .env.development
DB_CONNECTION_STRING=postgresql://postgres:postgres@localhost:5432/postgres
NODE_CONNECTION_STRING=https://eth-mainnet.g.alchemy.com/v2/DEV_KEY
ROUTER_ENDPOINT=0.0.0.0:3000
RUST_LOG=debug
INDEX_TRANSACTIONS=false
START_BLOCK_OFFSET=100
IS_DEV=true
```

### Staging Preset

Production-like configuration with higher logging:

```bash
# ECS task definition for staging
DB_CONNECTION_STRING=postgresql://admin:pass@staging-db.rds.amazonaws.com:5432/fossil
NODE_CONNECTION_STRING=https://eth-mainnet.g.alchemy.com/v2/STAGING_KEY
ROUTER_ENDPOINT=0.0.0.0:3000
RUST_LOG=info
INDEX_TRANSACTIONS=false
START_BLOCK_OFFSET=10000
```

### Production Preset

Optimized for reliability and performance:

```bash
# ECS task definition for production
DB_CONNECTION_STRING=postgresql://admin:pass@prod-db.rds.amazonaws.com:5432/fossil?sslmode=require
NODE_CONNECTION_STRING=https://eth-mainnet.g.alchemy.com/v2/PROD_KEY
ROUTER_ENDPOINT=0.0.0.0:3000
RUST_LOG=info
INDEX_TRANSACTIONS=false
START_BLOCK_OFFSET=10000
```

## Performance Tuning

### For Maximum Throughput

```rust
// Increase batch size
index_batch_size(5000)

// More concurrent requests
max_concurrent_requests(20)

// Longer timeouts for slow RPCs
rpc_timeout(600)
```

**Requirements:**
- High-throughput RPC endpoint (dedicated node or premium tier)
- Adequate database I/O capacity
- Sufficient network bandwidth

### For Reliability

```rust
// Smaller batches, less likely to timeout
index_batch_size(500)

// Conservative concurrency
max_concurrent_requests(5)

// Shorter timeouts, fail faster
rpc_timeout(120)

// More retry attempts
max_retries(15)
```

**Best for:**
- Shared/free-tier RPC endpoints
- Unstable network connections
- Resource-constrained environments

### For Low Resource Usage

```rust
// Small batches
index_batch_size(100)

// Minimal concurrency
max_concurrent_requests(2)

// Conservative retries
max_retries(5)
```

**Best for:**
- Development machines
- CI/CD environments
- Cost optimization

## Security Configuration

### Database Credentials

**Never commit credentials to version control:**

```bash
# Bad - Do not do this
DB_CONNECTION_STRING=postgresql://admin:MyP@ssw0rd@prod-db.com:5432/fossil

# Good - Use environment variables
DB_CONNECTION_STRING=$DB_CONN_FROM_SECRETS_MANAGER
```

**Best Practices:**
1. Use AWS Secrets Manager or similar
2. Rotate credentials regularly
3. Use IAM database authentication when possible
4. Enable SSL/TLS for database connections

### RPC API Keys

```bash
# Bad - hardcoded key
NODE_CONNECTION_STRING=https://eth-mainnet.g.alchemy.com/v2/abc123...

# Good - use environment variable or secrets manager
NODE_CONNECTION_STRING=$RPC_ENDPOINT_FROM_SECRETS
```

## Validation

After configuring, validate your setup:

```bash
# Check environment variables are set
env | grep -E '(DB_CONNECTION|NODE_CONNECTION)'

# Test database connection
psql "$DB_CONNECTION_STRING" -c "SELECT 1;"

# Test RPC endpoint
curl -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
  "$NODE_CONNECTION_STRING"

# Start indexer and check logs
make run-indexer
```

## Troubleshooting Configuration

### Environment Variables Not Loading

**Problem**: Indexer doesn't see `.env` variables

**Solution**:
```bash
# Ensure IS_DEV is set
IS_DEV=true

# Or manually export variables
export $(grep -v '^#' .env | xargs)
```

### Database Connection Fails

**Check connection string format**:
```bash
# Test with psql
psql "$DB_CONNECTION_STRING" -c "SELECT version();"
```

### RPC Endpoint Not Working

**Test endpoint manually**:
```bash
curl -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"eth_chainId","params":[],"id":1}' \
  "$NODE_CONNECTION_STRING"

# Should return: {"jsonrpc":"2.0","id":1,"result":"0x1"}  (mainnet)
```

## Next Steps

- [Run the indexer](quick-start.md)
- [Deploy to production](../guides/deployment.md)
- [Monitor performance](../guides/monitoring.md)
- [Tune for your use case](../architecture/indexing-services.md)
