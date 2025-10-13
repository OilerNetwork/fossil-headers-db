# Light Client Integration Guide

This guide explains how the Fossil Light Client ecosystem integrates with and consumes data from the Fossil Headers DB indexer.

## Overview

The Fossil Headers DB indexer is the **foundational data layer** for the entire Fossil Light Client system. It provides:

1. **Validated Ethereum block headers** from genesis to latest finalized
2. **Transaction data** (optional) for comprehensive proofs
3. **Continuous data availability** for MMR construction
4. **Gap-free block sequences** ensuring proof integrity

## System Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                    Ethereum Mainnet                              │
│           (Source of Truth for Block Headers)                    │
└──────────────────────────┬───────────────────────────────────────┘
                           │
                           │ JSON-RPC
                           ▼
┌──────────────────────────────────────────────────────────────────┐
│              Fossil Headers DB Indexer                           │
│  • Indexes block headers continuously                            │
│  • Validates parent-child relationships                          │
│  • Detects and fills gaps automatically                          │
│  • Provides PostgreSQL database for queries                      │
└──────────────────────────┬───────────────────────────────────────┘
                           │
              ┌────────────┴────────────┐
              │                         │
              ▼                         ▼
┌────────────────────────┐   ┌───────────────────────┐
│    MMR Builder         │   │   Light Client        │
│                        │   │                       │
│ • Reads 1024-block     │   │ • Monitors finalized  │
│   batches              │   │   blocks              │
│ • Constructs MMR       │   │ • Verifies MMR state  │
│ • Generates zkVM proofs│   │ • Updates continuously│
│ • Calculates avg fees  │   │ • Handles L1→L2 relay │
└────────┬───────────────┘   └───────┬───────────────┘
         │                           │
         └────────────┬──────────────┘
                      │
                      ▼
┌──────────────────────────────────────────────────────────────────┐
│                    RISC Zero zkVM                                │
│  • Validates block headers cryptographically                     │
│  • Constructs Merkle Mountain Range (MMR)                        │
│  • Calculates hourly average base fees                           │
│  • Produces zero-knowledge proofs                                │
└──────────────────────────┬───────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────┐
│                 Starknet L2 - Fossil Store Contract             │
│  • Stores MMR roots and commitments                              │
│  • Maintains IPFS CIDs for large state                           │
│  • Provides verified fee data for Pitchlake                      │
│  • Emits update events for Light Client sync                     │
└──────────────────────────────────────────────────────────────────┘
```

## Integration Points

### 1. MMR Builder Integration

The MMR Builder is the primary consumer of the Fossil Headers DB. It processes blocks in batches of **1024 blocks** to construct Merkle Mountain Ranges.

#### Database Queries

**Fetch Block Batch for MMR Construction**

```sql
-- Query used by MMR Builder
SELECT
    number,
    hash,
    parent_hash,
    timestamp,
    base_fee_per_gas
FROM block_header
WHERE number >= $start_block AND number < $start_block + 1024
ORDER BY number ASC;
```

**Parameters:**
- `$start_block`: Starting block number (should be multiple of 1024 for clean batches)

**Expected Result:**
- Exactly 1024 rows (one complete batch)
- Ordered sequentially by block number
- No gaps in the sequence

**Validation Before Processing:**

```sql
-- Verify batch completeness
SELECT COUNT(*) = 1024 AS is_complete
FROM block_header
WHERE number >= $start_block AND number < $start_block + 1024;

-- Verify no gaps in sequence
SELECT NOT EXISTS (
    SELECT 1
    FROM generate_series($start_block, $start_block + 1023) AS expected_number
    WHERE NOT EXISTS (
        SELECT 1 FROM block_header WHERE number = expected_number
    )
) AS no_gaps;
```

**Example: Building MMR for blocks 0-1023**

```rust
// Rust code example (simplified)
use sqlx::PgPool;

async fn fetch_mmr_batch(
    pool: &PgPool,
    start_block: i64,
) -> Result<Vec<BlockHeader>> {
    let batch = sqlx::query_as!(
        BlockHeader,
        r#"
        SELECT number, hash, parent_hash, timestamp, base_fee_per_gas
        FROM block_header
        WHERE number >= $1 AND number < $1 + 1024
        ORDER BY number ASC
        "#,
        start_block
    )
    .fetch_all(pool)
    .await?;

    // Validate we got exactly 1024 blocks
    if batch.len() != 1024 {
        return Err(Error::IncompleteBatch {
            expected: 1024,
            got: batch.len(),
        });
    }

    Ok(batch)
}
```

#### MMR Construction Workflow

```
1. Fetch batch of 1024 blocks from database
   ↓
2. Validate block sequence integrity:
   ├─ Check all 1024 blocks present
   ├─ Verify parent_hash continuity
   └─ Validate block number sequence
   ↓
3. Pass blocks to RISC Zero zkVM guest program
   ↓
4. zkVM performs cryptographic validation:
   ├─ Rehash each block header (RLP encoding)
   ├─ Verify computed hash matches stored hash
   ├─ Verify parent-child relationships
   └─ Construct MMR from validated hashes
   ↓
5. zkVM outputs proof journal:
   ├─ MMR root hash
   ├─ Number of leaves (1024)
   ├─ Hourly average base fees
   └─ Block range metadata
   ↓
6. Submit proof to Starknet Fossil Store contract
   ↓
7. Store full MMR state on IPFS (referenced by CID)
```

### 2. Light Client Integration

The Light Client continuously monitors new finalized blocks and updates the MMR incrementally.

#### Monitoring for New Blocks

**Query Latest Indexed Block:**

```sql
SELECT MAX(number) AS latest_block
FROM block_header;
```

**Query Index Metadata:**

```sql
SELECT
    current_latest_block_number,
    is_backfilling,
    backfilling_block_number,
    updated_at
FROM index_metadata;
```

**Use Case:**
- Light Client checks `current_latest_block_number` periodically
- When new blocks detected, fetch and process them
- Continue MMR from previous state

#### Fetching New Blocks Since Last Sync

```sql
-- Get all blocks since last processed block
SELECT
    number,
    hash,
    parent_hash,
    timestamp,
    base_fee_per_gas
FROM block_header
WHERE number > $last_processed_block
ORDER BY number ASC
LIMIT 1024;  -- Process in batches
```

**Example: Light Client Update Logic**

```rust
async fn sync_new_blocks(
    pool: &PgPool,
    last_processed_block: i64,
) -> Result<Vec<BlockHeader>> {
    // Fetch new blocks
    let new_blocks = sqlx::query_as!(
        BlockHeader,
        r#"
        SELECT number, hash, parent_hash, timestamp, base_fee_per_gas
        FROM block_header
        WHERE number > $1
        ORDER BY number ASC
        LIMIT 1024
        "#,
        last_processed_block
    )
    .fetch_all(pool)
    .await?;

    // Process blocks and update MMR
    if !new_blocks.is_empty() {
        update_mmr_with_new_blocks(&new_blocks).await?;
    }

    Ok(new_blocks)
}
```

### 3. Fee Data Aggregation for Pitchlake

The Pitchlake Coprocessor requires hourly average base fees for options pricing calculations.

#### Hourly Fee Aggregation Query

```sql
-- Calculate average base fee per hour
SELECT
    (timestamp / 3600) * 3600 AS hour_timestamp,
    AVG(base_fee_per_gas) AS avg_base_fee,
    COUNT(*) AS block_count,
    MIN(number) AS first_block,
    MAX(number) AS last_block
FROM block_header
WHERE timestamp >= $start_timestamp
  AND timestamp < $end_timestamp
GROUP BY hour_timestamp
ORDER BY hour_timestamp ASC;
```

**Parameters:**
- `$start_timestamp`: Start of time range (Unix timestamp, must be hour-aligned: `timestamp % 3600 == 0`)
- `$end_timestamp`: End of time range (Unix timestamp, must be hour-aligned)

**Example Result:**

| hour_timestamp | avg_base_fee | block_count | first_block | last_block |
|----------------|--------------|-------------|-------------|------------|
| 1704067200 | 45123456789 | 298 | 18900000 | 18900297 |
| 1704070800 | 47234567890 | 301 | 18900298 | 18900598 |
| 1704074400 | 43987654321 | 299 | 18900599 | 18900897 |

**Interpretation:**
- Each row represents one hour of blocks
- `avg_base_fee` is the weighted average (wei)
- `block_count` shows how many blocks in that hour
- Ethereum averages ~300 blocks per hour (12s block time)

#### Pitchlake Integration Flow

```
1. Pitchlake backend requests TWAP, max return, reserve price
   ↓
2. Fossil API receives request with timestamp range
   ↓
3. Message Handler queries Headers DB for hourly fees:
   ├─ Validate timestamps are hour-aligned
   ├─ Fetch hourly aggregated fee data
   └─ Pass fee data to RISC Zero zkVM
   ↓
4. zkVM computes pricing metrics:
   ├─ Time-Weighted Average Price (TWAP)
   ├─ Maximum Return calculation
   └─ Reserve Price calculation
   ↓
5. Generate zero-knowledge proof of computation
   ↓
6. Submit proof to Starknet Pitchlake Verifier
   ↓
7. Extract verified results to Pitchlake Vault contract
```

## Data Guarantees

### Block Continuity Guarantee

The indexer ensures no gaps in the block sequence. This is critical for:

1. **MMR Construction**: MMR requires continuous block hashes as leaves
2. **Proof Generation**: zkVM validation assumes sequential parent-child relationships
3. **Fee Calculations**: Missing blocks would skew average calculations

**How Gaps Are Prevented:**

1. **Quick Indexer**: Processes new finalized blocks immediately
2. **Batch Indexer**: Automatically detects and fills gaps
3. **Gap Detection Query**: Runs periodically to find missing blocks

```sql
-- Gap detection (used internally by batch indexer)
SELECT number AS missing_block
FROM generate_series(0, (SELECT MAX(number) FROM block_header)) AS number
WHERE NOT EXISTS (
    SELECT 1 FROM block_header WHERE block_header.number = number.number
)
ORDER BY number;
```

### Parent-Child Validation

Every block indexed is validated for correct parent-child relationships:

```rust
// Validation logic in indexer
if block.parent_hash != previous_block.hash {
    return Err(BlockchainError::InvalidBlockSequence {
        block_number: block.number,
        expected_parent: previous_block.hash,
        actual_parent: block.parent_hash,
    });
}
```

This ensures the indexed chain represents a valid Ethereum canonical chain.

### Timestamp Monotonicity

Block timestamps are guaranteed to be non-decreasing:

```rust
if block.timestamp < previous_block.timestamp {
    warn!(
        "Non-monotonic timestamp detected: block {} has earlier timestamp than block {}",
        block.number, previous_block.number
    );
}
```

While Ethereum allows non-monotonic timestamps (within limits), this is logged for awareness.

## Querying Best Practices

### 1. Always Check for Completeness

Before processing blocks for MMR or proofs:

```sql
-- Verify required blocks are available
SELECT COUNT(*) AS available_blocks
FROM block_header
WHERE number >= $start AND number <= $end;

-- Should equal ($end - $start + 1)
```

### 2. Use Indexed Columns

The database schema includes indexes on:
- `block_header(number)` - Primary key
- `block_header(hash)` - Fast hash lookups
- `block_header(timestamp)` - Time-based queries

**Optimized Query:**
```sql
-- Good: Uses index on number
SELECT * FROM block_header WHERE number = 12345678;

-- Good: Uses index on hash
SELECT * FROM block_header WHERE hash = '0xabc...';

-- Good: Uses index on timestamp
SELECT * FROM block_header WHERE timestamp >= 1704067200 AND timestamp < 1704153600;
```

**Avoid:**
```sql
-- Bad: Full table scan (no index on parent_hash)
SELECT * FROM block_header WHERE parent_hash = '0xabc...';
```

### 3. Batch Queries for Performance

When fetching multiple blocks, use range queries instead of individual lookups:

**Good:**
```sql
SELECT * FROM block_header
WHERE number >= 1000000 AND number < 1001024
ORDER BY number;
```

**Avoid:**
```sql
-- Bad: 1024 separate queries
SELECT * FROM block_header WHERE number = 1000000;
SELECT * FROM block_header WHERE number = 1000001;
-- ... 1022 more queries
```

### 4. Connection Pooling

Use connection pools to avoid overhead of establishing connections:

```rust
// Create pool once, reuse throughout application
let pool = PgPoolOptions::new()
    .max_connections(20)
    .min_connections(5)
    .acquire_timeout(Duration::from_secs(30))
    .connect(&database_url)
    .await?;

// Reuse pool for all queries
let blocks = fetch_blocks(&pool, start, end).await?;
```

## Error Handling

### Incomplete Batch Errors

**Error**: Not all blocks in a batch are available

**Cause**: Indexer still backfilling, or gap detection hasn't completed

**Handling:**
```rust
match fetch_mmr_batch(&pool, start_block).await {
    Ok(blocks) => {
        // Process complete batch
        build_mmr(blocks).await?;
    }
    Err(Error::IncompleteBatch { expected, got }) => {
        // Wait for indexer to catch up
        warn!("Batch incomplete: {}/{} blocks available", got, expected);
        tokio::time::sleep(Duration::from_secs(60)).await;
        // Retry
    }
    Err(e) => {
        // Other error, propagate
        return Err(e);
    }
}
```

### Gap Detection

**Error**: Gap detected in block sequence

**Handling:**
```rust
// Check for gaps before processing
let has_gaps = check_for_gaps(&pool, start, end).await?;

if has_gaps {
    warn!("Gaps detected in range {}-{}, waiting for batch indexer", start, end);
    // Wait for batch indexer to fill gaps
    tokio::time::sleep(Duration::from_secs(120)).await;
} else {
    // Safe to proceed
    process_blocks(&pool, start, end).await?;
}
```

## Monitoring Integration

### Health Checks

**Indexer Health:**
```bash
curl http://indexer-host:3000/health
```

**Expected Response:**
```json
{
  "status": "healthy",
  "timestamp": "2025-10-14T12:00:00.000Z"
}
```

**Database Health:**
```sql
SELECT
    current_latest_block_number,
    is_backfilling,
    updated_at,
    (EXTRACT(EPOCH FROM NOW()) - EXTRACT(EPOCH FROM updated_at)) AS seconds_since_update
FROM index_metadata;
```

**Alert Conditions:**
- `seconds_since_update > 300` → Indexer may be stuck
- `is_backfilling = true` AND `backfilling_block_number` not decreasing → Backfill stalled

### Progress Monitoring

```sql
-- Calculate indexing progress
SELECT
    current_latest_block_number AS latest_indexed,
    (SELECT MAX(number) FROM block_header) AS highest_block,
    backfilling_block_number AS backfill_position,
    indexing_starting_block_number AS target_block,
    -- Progress percentage
    CASE
        WHEN is_backfilling THEN
            ROUND(
                (current_latest_block_number - backfilling_block_number)::NUMERIC /
                (current_latest_block_number - indexing_starting_block_number)::NUMERIC * 100,
                2
            )
        ELSE 100.00
    END AS progress_pct
FROM index_metadata;
```

## Performance Considerations

### Read Replica for Light Client

**Recommendation**: Use a PostgreSQL read replica for Light Client queries

**Setup:**
1. Configure streaming replication from primary to replica
2. Point Light Client to replica connection string
3. Keep write operations (indexer) on primary

**Benefits:**
- Reduces load on primary database
- Allows independent scaling of read vs. write capacity
- No impact on indexer performance from Light Client queries

### Query Optimization

**For Large Batch Queries:**
```sql
-- Use EXPLAIN ANALYZE to verify query plan
EXPLAIN ANALYZE
SELECT * FROM block_header
WHERE number >= 1000000 AND number < 1001024;

-- Should show "Index Scan" not "Seq Scan"
```

**For Hourly Aggregation:**
```sql
-- Create materialized view for frequently accessed fee data
CREATE MATERIALIZED VIEW hourly_fees AS
SELECT
    (timestamp / 3600) * 3600 AS hour_timestamp,
    AVG(base_fee_per_gas) AS avg_base_fee,
    COUNT(*) AS block_count
FROM block_header
GROUP BY hour_timestamp;

-- Refresh periodically (e.g., every hour)
REFRESH MATERIALIZED VIEW hourly_fees;

-- Query the materialized view instead
SELECT * FROM hourly_fees
WHERE hour_timestamp >= $start AND hour_timestamp < $end;
```

## Example: Complete MMR Builder Integration

Here's a complete example of how the MMR Builder might integrate with the Headers DB:

```rust
use sqlx::PgPool;
use std::time::Duration;

pub struct MmrBuilder {
    db_pool: PgPool,
    batch_size: i64,
}

impl MmrBuilder {
    pub async fn new(database_url: &str) -> Result<Self> {
        let db_pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await?;

        Ok(Self {
            db_pool,
            batch_size: 1024,
        })
    }

    pub async fn build_mmr_batch(&self, start_block: i64) -> Result<MmrProof> {
        // 1. Validate batch availability
        self.validate_batch_completeness(start_block).await?;

        // 2. Fetch blocks
        let blocks = self.fetch_blocks(start_block).await?;

        // 3. Validate block sequence
        self.validate_block_sequence(&blocks)?;

        // 4. Build MMR in zkVM
        let mmr_root = self.construct_mmr_in_zkvm(&blocks).await?;

        // 5. Calculate hourly fees
        let hourly_fees = self.aggregate_hourly_fees(&blocks)?;

        // 6. Generate proof
        let proof = self.generate_proof(mmr_root, hourly_fees).await?;

        Ok(proof)
    }

    async fn validate_batch_completeness(&self, start_block: i64) -> Result<()> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM block_header
             WHERE number >= $1 AND number < $1 + $2"
        )
        .bind(start_block)
        .bind(self.batch_size)
        .fetch_one(&self.db_pool)
        .await?;

        if count != self.batch_size {
            return Err(Error::IncompleteBatch {
                expected: self.batch_size,
                got: count,
            });
        }

        Ok(())
    }

    async fn fetch_blocks(&self, start_block: i64) -> Result<Vec<BlockHeader>> {
        let blocks = sqlx::query_as!(
            BlockHeader,
            r#"
            SELECT number, hash, parent_hash, timestamp, base_fee_per_gas
            FROM block_header
            WHERE number >= $1 AND number < $1 + $2
            ORDER BY number ASC
            "#,
            start_block,
            self.batch_size
        )
        .fetch_all(&self.db_pool)
        .await?;

        Ok(blocks)
    }

    fn validate_block_sequence(&self, blocks: &[BlockHeader]) -> Result<()> {
        for window in blocks.windows(2) {
            let prev = &window[0];
            let curr = &window[1];

            // Check block number sequence
            if curr.number != prev.number + 1 {
                return Err(Error::InvalidBlockSequence {
                    expected: prev.number + 1,
                    got: curr.number,
                });
            }

            // Check parent-child relationship
            if curr.parent_hash != prev.hash {
                return Err(Error::InvalidParentHash {
                    block: curr.number,
                    expected: prev.hash.clone(),
                    got: curr.parent_hash.clone(),
                });
            }
        }

        Ok(())
    }

    async fn construct_mmr_in_zkvm(&self, blocks: &[BlockHeader]) -> Result<MmrRoot> {
        // Implementation: Pass blocks to RISC Zero guest program
        // Returns MMR root hash after cryptographic validation
        todo!()
    }

    fn aggregate_hourly_fees(&self, blocks: &[BlockHeader]) -> Result<Vec<HourlyFee>> {
        // Group blocks by hour and calculate averages
        todo!()
    }

    async fn generate_proof(&self, mmr_root: MmrRoot, fees: Vec<HourlyFee>) -> Result<MmrProof> {
        // Generate zero-knowledge proof
        todo!()
    }
}
```

## Related Documentation

- [Architecture Overview](../architecture/overview.md)
- [Database Schema](../architecture/database-schema.md)
- [Deployment Guide](../guides/deployment.md)
- [API Reference](api-reference.md)
- [Fossil Light Client Technical Specification](https://github.com/OilerNetwork/fossil-light-client/docs/technical-specification.md)
