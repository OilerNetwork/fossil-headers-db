# Architecture Overview

This document provides a comprehensive overview of the Fossil Headers DB indexer architecture, design decisions, and system components.

## System Purpose

The Fossil Headers DB indexer is a critical infrastructure component of the Fossil Light Client ecosystem. Its primary purpose is to:

1. **Index Ethereum block headers** from genesis to the latest finalized block
2. **Provide a reliable data source** for the MMR Builder and Light Client
3. **Maintain data integrity** through automated gap detection and filling
4. **Enable trustless verification** of Ethereum state on Starknet

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      Ethereum Network                           │
│                   (JSON-RPC Endpoint)                           │
└──────────────────────────────┬──────────────────────────────────┘
                               │
                               │ eth_getBlockByNumber
                               │ eth_getTransactionByHash
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│                Fossil Headers DB Indexer                        │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              RPC Client Layer                           │   │
│  │  • Retry logic with exponential backoff                 │   │
│  │  • Rate limit handling                                  │   │
│  │  • Connection pooling                                   │   │
│  └─────────────────┬───────────────────────────────────────┘   │
│                    │                                            │
│  ┌─────────────────┴───────────────────────────────────────┐   │
│  │         Indexing Services Coordinator                   │   │
│  │  • Service lifecycle management                         │   │
│  │  • Graceful shutdown handling                           │   │
│  │  • Health check server                                  │   │
│  └─┬────────────────────────────────────────────────┬──────┘   │
│    │                                                │          │
│  ┌─▼───────────────────────┐      ┌────────────────▼──────┐   │
│  │   Quick Indexer         │      │   Batch Indexer       │   │
│  │                         │      │                       │   │
│  │  Strategy:              │      │  Strategy:            │   │
│  │  • Real-time sync       │      │  • Historical backfill│   │
│  │  • Poll every 10s       │      │  • 1000 blocks/batch  │   │
│  │  • Latest → Forward     │      │  • Offset → Genesis   │   │
│  │  • Single block fetch   │      │  • Parallel fetches   │   │
│  └─┬───────────────────────┘      └────────────────┬──────┘   │
│    │                                                │          │
│    └────────────────────┬───────────────────────────┘          │
│                         │                                      │
│  ┌──────────────────────▼──────────────────────────────────┐   │
│  │              Database Layer                             │   │
│  │  • Connection pooling (SQLx)                            │   │
│  │  • Transaction management                               │   │
│  │  • Batch insert optimization                            │   │
│  └──────────────────────┬──────────────────────────────────┘   │
└─────────────────────────┼──────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────────┐
│                    PostgreSQL Database                          │
│                                                                 │
│  Tables:                                                        │
│  • block_header          - Block metadata                      │
│  • block_header_tx       - Transactions (optional)             │
│  • index_metadata        - Indexer state                       │
│                                                                 │
│  Indexes:                                                       │
│  • block_header(number)  - Primary key                         │
│  • block_header(hash)    - Block hash lookup                   │
│  • block_header(timestamp) - Time-based queries                │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           │ Consumed by
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│              Fossil Light Client Ecosystem                      │
│                                                                 │
│  • MMR Builder: Constructs Merkle Mountain Ranges              │
│  • Light Client: Validates and syncs new blocks                │
│  • RISC0 zkVM: Generates proofs of block header validity       │
│  • Starknet: Stores MMR roots and verified proofs              │
└─────────────────────────────────────────────────────────────────┘
```

## Core Components

### 1. RPC Client Layer

**Location**: `src/rpc/mod.rs`

**Responsibilities:**
- Abstracts Ethereum JSON-RPC communication
- Implements retry logic with exponential backoff
- Handles rate limiting and connection failures
- Provides type-safe RPC method interfaces

**Key Features:**
- **Automatic retries**: Up to 5 attempts with exponential backoff
- **Timeout handling**: Configurable timeout per request (default 300s)
- **Error categorization**: Distinguishes between retryable and fatal errors

**Example Usage:**
```rust
let rpc_client = EthereumJsonRpcClient::new(endpoint, max_retries);
let block_number = rpc_client.get_latest_finalized_blocknumber(None).await?;
let block_header = rpc_client.get_blockheader_by_blocknumber(block_number).await?;
```

### 2. Quick Indexer Service

**Location**: `src/indexer/quick_service.rs`

**Purpose**: Real-time synchronization with the Ethereum network tip

**Strategy:**
1. Poll for latest finalized block every 10 seconds
2. Compare with database latest block
3. Index any new finalized blocks sequentially
4. Update index metadata

**Configuration:**
```rust
QuickIndexConfig {
    should_index_txs: false,
    index_batch_size: 100,
    max_retries: 10,
    poll_interval: 10,        // seconds
    rpc_timeout: 300,
}
```

**Performance Characteristics:**
- **Latency**: ~10-15 seconds behind Ethereum finality
- **Throughput**: Limited by finality rate (~12 seconds per block)
- **Resource usage**: Low (single block at a time)

### 3. Batch Indexer Service

**Location**: `src/indexer/batch_service.rs`

**Purpose**: Historical backfilling and gap detection/filling

**Strategy:**
1. Check database for gaps or missing historical blocks
2. Fetch blocks in batches of 1000
3. Process batches with parallel RPC requests (10 concurrent)
4. Handle failures gracefully with retries
5. Automatically disable when reaching genesis or no gaps

**Configuration:**
```rust
BatchIndexConfig {
    should_index_txs: false,
    index_batch_size: 1000,
    max_retries: 10,
    poll_interval: 10,
    rpc_timeout: 300,
    max_concurrent_requests: 10,
    task_timeout: 300,
}
```

**Performance Characteristics:**
- **Throughput**: 50-100 blocks/second (RPC-limited)
- **Concurrency**: 10 parallel block fetches
- **Reliability**: Automatic gap detection and retry

**Gap Detection Algorithm:**

```rust
// Simplified gap detection logic
fn find_gaps(start: BlockNumber, end: BlockNumber) -> Vec<Gap> {
    // Query database for missing block numbers
    SELECT number
    FROM generate_series(start, end) AS number
    WHERE NOT EXISTS (
        SELECT 1 FROM block_header
        WHERE block_header.number = number.number
    )
}
```

### 4. Database Layer

**Location**: `src/db/mod.rs`, `src/repositories/`

**Responsibilities:**
- Connection pool management
- Query abstractions
- Transaction coordination
- Data integrity enforcement

**Connection Pooling:**
```rust
// Pool configuration
pub const DB_MAX_CONNECTIONS: u32 = 100;
pub const DB_MIN_CONNECTIONS: u32 = 5;
pub const DB_ACQUIRE_TIMEOUT_SECS: u64 = 30;
```

**Repositories:**
- `BlockHeaderRepository`: CRUD operations for block headers
- `IndexMetadataRepository`: Indexer state management

### 5. Health Check Server

**Location**: `src/router/mod.rs`

**Purpose**: HTTP endpoints for monitoring and load balancer health checks

**Endpoints:**
- `GET /health` - Returns health status
- `GET /mmr` (future) - MMR state information
- `GET /mmr/<block_number>` (future) - Block-specific MMR proof

## Data Flow

### Quick Indexer Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                 Quick Indexer Process                           │
└─────────────────────────────────────────────────────────────────┘

1. Poll RPC for latest finalized block
   ↓
2. Query database for latest indexed block
   ↓
3. If new blocks available:
   ├─ Fetch block header via RPC
   ├─ Optionally fetch transactions
   ├─ Validate block number sequence
   ├─ Insert into database
   └─ Update index_metadata.current_latest_block_number
   ↓
4. Sleep for poll_interval seconds
   ↓
5. Repeat (unless shutdown signal received)
```

### Batch Indexer Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                 Batch Indexer Process                           │
└─────────────────────────────────────────────────────────────────┘

1. Check index_metadata.is_backfilling flag
   ↓
2. If backfilling enabled:
   ├─ Get current backfilling_block_number
   ├─ Calculate batch range (current - 1000 to current)
   ├─ Fetch batch in parallel (10 concurrent requests)
   ├─ Insert batch into database
   ├─ Update backfilling_block_number
   └─ Check if reached genesis or starting block
   ↓
3. Check for gaps in indexed blocks
   ├─ Query for missing block numbers
   ├─ If gaps found, fetch and fill them
   └─ Continue until no gaps
   ↓
4. If no work to do:
   ├─ Set is_backfilling = false
   └─ Sleep for poll_interval
   ↓
5. Repeat (unless shutdown signal received)
```

## Design Decisions

### 1. Dual Indexing Strategy

**Why separate Quick and Batch indexers?**

| Aspect | Quick Indexer | Batch Indexer |
|--------|---------------|---------------|
| **Goal** | Stay current with network | Fill historical data |
| **Strategy** | Sequential, real-time | Parallel, batch processing |
| **Optimization** | Low latency | High throughput |
| **Priority** | Latest blocks | Historical coverage |

**Benefits:**
- Never miss new blocks (Quick handles latest)
- Efficiently backfill history (Batch optimized for volume)
- Independent failure domains (one can fail without affecting the other)
- Different retry strategies per use case

### 2. Database Schema Design

**Why these specific tables?**

**`block_header` table:**
```sql
CREATE TABLE block_header (
    number BIGINT PRIMARY KEY,
    hash TEXT NOT NULL,
    parent_hash TEXT NOT NULL,
    timestamp BIGINT NOT NULL,
    base_fee_per_gas BIGINT,
    -- Additional fields...
);
```

**Rationale:**
- `number` as primary key: Natural ordering, fast lookups
- `hash` indexed separately: Enable hash-based queries
- `timestamp` indexed: Support time-range queries for Light Client
- `base_fee_per_gas`: Critical for fee calculation in zkVM

**`index_metadata` table:**
```sql
CREATE TABLE index_metadata (
    id SERIAL PRIMARY KEY,
    current_latest_block_number BIGINT NOT NULL,
    indexing_starting_block_number BIGINT NOT NULL,
    is_backfilling BOOLEAN NOT NULL,
    backfilling_block_number BIGINT,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
```

**Rationale:**
- Single row table: Simplifies state management
- `is_backfilling` flag: Control batch indexer behavior
- `backfilling_block_number`: Resume interrupted backfills
- `current_latest_block_number`: Quick indexer checkpoint

### 3. Error Handling Strategy

**Retry vs. Fail Fast:**

```rust
// Retryable errors (temporary issues)
- Network timeouts
- HTTP 429 (Rate limit)
- HTTP 502/503 (Service unavailable)
- Connection refused

// Fatal errors (fail fast)
- Invalid RPC endpoint
- Authentication failure
- Database constraint violations
- Invalid block data format
```

**Exponential Backoff:**
```rust
let wait_time = base_delay * 2^(attempt - 1)
// Attempt 1: 1s
// Attempt 2: 2s
// Attempt 3: 4s
// Attempt 4: 8s
// Attempt 5: 16s
```

This prevents overwhelming RPC providers during temporary issues.

### 4. Type Safety

**Why custom types instead of primitives?**

```rust
// Instead of using i64 directly:
pub struct BlockNumber(i64);  // Validated, non-negative

// Instead of String:
pub struct BlockHash(String);  // Validated hex, 64 characters

// Instead of String:
pub struct Address(String);    // Validated hex, 40 characters
```

**Benefits:**
- Compile-time validation
- Self-documenting code
- Prevents logic errors (e.g., negative block numbers)
- Type safety across RPC, database, and domain layers

## Integration Points

### Light Client Integration

The indexer provides data to the Light Client ecosystem in several ways:

#### 1. Direct Database Access

**MMR Builder** queries the database directly:

```sql
-- Fetch batch of 1024 blocks for MMR construction
SELECT number, hash, parent_hash, timestamp, base_fee_per_gas
FROM block_header
WHERE number BETWEEN $1 AND $2
ORDER BY number ASC;
```

**Use Case**: Construct Merkle Mountain Ranges in batches of 1024 blocks

#### 2. Gap Detection for Proof Validation

**Light Client** ensures no gaps before generating proofs:

```sql
-- Verify continuous block sequence
SELECT COUNT(*) = ($end - $start + 1) AS is_continuous
FROM block_header
WHERE number BETWEEN $start AND $end;
```

**Use Case**: Validate block header availability before proof generation

#### 3. Fee Data Aggregation

**zkVM Prover** fetches hourly fee data:

```sql
-- Aggregate base fees by hour
SELECT
    (timestamp / 3600) * 3600 AS hour_timestamp,
    AVG(base_fee_per_gas) AS avg_fee,
    COUNT(*) AS block_count
FROM block_header
WHERE timestamp BETWEEN $start AND $end
GROUP BY hour_timestamp
ORDER BY hour_timestamp;
```

**Use Case**: Calculate hourly average fees for Pitchlake pricing

## Performance Considerations

### Bottlenecks

1. **RPC Endpoint**: Primary bottleneck
   - Solution: Use premium tier or dedicated node
   - Mitigation: Concurrent requests, retry logic

2. **Database I/O**: Secondary bottleneck
   - Solution: Use SSD storage, tune PostgreSQL
   - Mitigation: Batch inserts, connection pooling

3. **Network Latency**: Regional delays
   - Solution: Deploy close to RPC provider
   - Mitigation: Increase timeout values

### Optimization Strategies

**For High Throughput:**
```rust
index_batch_size(5000)           // Larger batches
max_concurrent_requests(20)      // More parallelism
```

**For Reliability:**
```rust
index_batch_size(500)            // Smaller batches
max_retries(15)                  // More retry attempts
```

**For Low Resources:**
```rust
index_batch_size(100)            // Minimal batches
max_concurrent_requests(2)       // Low concurrency
```

## Scalability

### Horizontal Scaling

**Current Limitation**: Single instance design
- Metadata table uses single row
- No distributed coordination

**Workaround for Multiple Regions:**
1. Run separate instances per region
2. Each instance indexes independent block ranges
3. Merge databases periodically (manual process)

**Future Enhancement**: Distributed indexing with range partitioning

### Vertical Scaling

**Database:**
- Increase connection pool size
- Add read replicas for Light Client queries
- Partition block_header table by block number range

**Indexer:**
- Increase batch size
- Increase concurrent RPC requests
- Tune RPC timeout values

## Security Considerations

### RPC Endpoint Security

**Threats:**
- API key exposure
- Man-in-the-middle attacks
- RPC endpoint poisoning

**Mitigations:**
- Store API keys in secrets manager (AWS Secrets Manager, etc.)
- Always use HTTPS RPC endpoints
- Validate block hash continuity (parent_hash chain)

### Database Security

**Threats:**
- SQL injection (mitigated by SQLx compile-time checking)
- Unauthorized access
- Data corruption

**Mitigations:**
- Use parameterized queries (SQLx default)
- SSL/TLS database connections in production
- Regular backups with point-in-time recovery

### Data Integrity

**Block Header Validation:**
```rust
// Verify parent-child relationship
if block.parent_hash != previous_block.hash {
    return Err(BlockchainError::InvalidBlockSequence);
}

// Verify block number sequence
if block.number != previous_block.number + 1 {
    return Err(BlockchainError::InvalidBlockNumber);
}
```

This ensures the indexed chain is valid and continuous.

## Monitoring and Observability

### Key Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `blocks_indexed_total` | Counter | Total blocks indexed since start |
| `rpc_requests_total` | Counter | Total RPC requests |
| `rpc_request_duration` | Histogram | RPC call latency |
| `database_operations_total` | Counter | Database queries |
| `gap_count` | Gauge | Current number of missing blocks |
| `latest_indexed_block` | Gauge | Highest indexed block number |
| `backfill_position` | Gauge | Current backfill block number |

### Log Events

**Critical Events:**
- Indexer start/stop
- Service failures
- Database connection loss
- Persistent RPC failures

**Info Events:**
- Batch completion
- Gap detection and filling
- Backfill milestones

## Future Enhancements

### Planned Features

1. **Metrics Endpoint**: Prometheus-compatible `/metrics`
2. **MMR State Endpoint**: Direct MMR queries via HTTP
3. **Distributed Indexing**: Coordinate multiple instances
4. **Event Streaming**: Pub/sub for real-time block notifications
5. **Block Reorganization Handling**: Detect and handle chain reorgs

### Light Client Integration Improvements

1. **Optimized Batch Queries**: Specialized queries for 1024-block MMR batches
2. **Checksum Validation**: Pre-compute and store block hash checksums
3. **IPFS Integration**: Direct IPFS pinning of block data
4. **RPC Fallback**: Multiple RPC endpoints with automatic failover

## Related Documentation

- [Indexing Services Deep Dive](indexing-services.md)
- [Database Schema Details](database-schema.md)
- [RPC Client Architecture](rpc-client.md)
- [Light Client Integration](../integration/light-client.md)
