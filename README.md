# Fossil Headers DB - Ethereum Block Indexer

[![CI](https://github.com/OilerNetwork/fossil-headers-db/actions/workflows/ci.yml/badge.svg)](https://github.com/OilerNetwork/fossil-headers-db/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/codecov/c/github/OilerNetwork/fossil-headers-db)](https://codecov.io/gh/OilerNetwork/fossil-headers-db)
[![License](https://img.shields.io/badge/License-GNU%20GPL-blue)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org/)

High-performance Ethereum blockchain indexer that fetches and stores block headers and transaction data from genesis to the latest finalized block. This indexer is a critical component of the **Fossil Light Client** infrastructure, providing the block header database that feeds the MMR Builder and Light Client synchronization process.

## Overview

**Fossil Headers DB** is a production-ready PostgreSQL-backed indexer designed to efficiently index Ethereum block headers for use by the Fossil Light Client ecosystem. It implements both real-time synchronization and historical backfilling strategies to ensure complete blockchain coverage.

### Key Features

- **Dual Indexing Strategy**: Simultaneous real-time indexing (quick service) and historical backfilling (batch service)
- **Smart Gap Detection**: Automatically identifies and fills missing blocks in the database
- **Fault Tolerant**: Robust retry mechanisms and RPC failover support
- **Production Ready**: Optimized for AWS ECS deployment with health checks and monitoring
- **Light Client Integration**: Provides validated block headers for MMR construction and proof generation
- **Database Migrations**: Automated schema management with SQLx migrations
- **HTTP API**: Health check endpoints for load balancer integration

## Architecture

The indexer consists of two main services that run concurrently:

### 1. Quick Indexer Service
**Purpose**: Real-time synchronization with the Ethereum network
- Polls for the latest finalized blocks every 10 seconds (configurable)
- Indexes new blocks as they become finalized
- Ensures the database stays current with the blockchain tip

### 2. Batch Indexer Service
**Purpose**: Historical backfilling and gap filling
- Processes blocks in large batches (1000 blocks by default)
- Automatically detects gaps in the stored data
- Backfills from a configurable starting block to genesis (block 0)
- Optimized for high-throughput historical data ingestion

### Integration with Fossil Light Client

```
┌─────────────────────────────────────────────────────────────┐
│                   Ethereum Network                          │
│              (Finalized Block Headers)                      │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│              Fossil Headers DB Indexer                      │
│  ┌──────────────────┐      ┌──────────────────┐            │
│  │  Quick Indexer   │      │  Batch Indexer   │            │
│  │ (Real-time sync) │      │ (Backfill/Gaps)  │            │
│  └──────────────────┘      └──────────────────┘            │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│              PostgreSQL Database                            │
│        (Block Headers + Transaction Data)                   │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│              Fossil Light Client                            │
│  ┌──────────────────┐      ┌──────────────────┐            │
│  │   MMR Builder    │      │  Light Client    │            │
│  │ (1024 blocks/    │      │  (Continuous     │            │
│  │  batch proofs)   │      │   sync)          │            │
│  └──────────────────┘      └──────────────────┘            │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│           Starknet (Fossil Store Contract)                  │
│     (MMR Roots, Proofs, Fee Data, IPFS CIDs)                │
└─────────────────────────────────────────────────────────────┘
```

## Documentation

### 📚 Getting Started
- [Installation Guide](docs/getting-started/installation.md) - Set up your development environment
- [Quick Start](docs/getting-started/quick-start.md) - Get the indexer running in 5 minutes
- [Configuration](docs/getting-started/configuration.md) - Environment variables and settings
- [Testing](docs/getting-started/testing.md) - Run tests and verify functionality

### 🏗️ Architecture
- [System Overview](docs/architecture/overview.md) - High-level architecture and design decisions
- [Indexing Services](docs/architecture/indexing-services.md) - Quick and Batch indexer deep dive
- [Database Schema](docs/architecture/database-schema.md) - PostgreSQL tables and relationships
- [RPC Client](docs/architecture/rpc-client.md) - Ethereum RPC interaction and retry logic

### 📖 Guides
- [Local Development](docs/guides/local-development.md) - Running locally with Docker Compose
- [Deployment](docs/guides/deployment.md) - Production deployment on AWS ECS
- [Monitoring](docs/guides/monitoring.md) - Health checks, logging, and metrics
- [Troubleshooting](docs/guides/troubleshooting.md) - Common issues and solutions
- [Database Management](docs/guides/database-management.md) - Migrations, backups, and maintenance

### 🔌 Integration
- [Light Client Integration](docs/integration/light-client.md) - How the Light Client consumes indexed data
- [API Reference](docs/integration/api-reference.md) - HTTP endpoints and responses
- [Database Access](docs/integration/database-access.md) - Direct database query patterns

## Quick Start

### Prerequisites

- Rust 1.70+ (installed via [rustup](https://rustup.rs/))
- PostgreSQL 16+ (for production) or Docker (for local development)
- Ethereum RPC endpoint (Alchemy, Infura, or self-hosted node)

### 1. Clone and Setup

```bash
# Clone the repository
git clone https://github.com/OilerNetwork/fossil-headers-db.git
cd fossil-headers-db

# Install dependencies and start local environment
make dev-setup
```

This command will:
- Install Rust dependencies
- Install SQLx CLI for database migrations
- Start PostgreSQL in Docker
- Run database migrations
- Set up the development environment

### 2. Configure Environment

Create a `.env` file in the project root:

```bash
# Database connection
DB_CONNECTION_STRING=postgresql://postgres:postgres@localhost:5432/postgres

# Ethereum RPC endpoint (replace with your provider)
NODE_CONNECTION_STRING=https://eth-mainnet.g.alchemy.com/v2/YOUR_API_KEY

# HTTP server configuration
ROUTER_ENDPOINT=0.0.0.0:3000
RUST_LOG=info

# Indexer settings
INDEX_TRANSACTIONS=false          # Set to true to index transaction data
START_BLOCK_OFFSET=1024          # Start backfilling from latest - 1024 blocks

# Optional: Development mode (loads .env file)
IS_DEV=true
```

### 3. Run the Indexer

```bash
# Run the modern indexer service (recommended for production)
make run-indexer

# Or run directly with cargo
cargo run --bin fossil_indexer
```

The indexer will:
1. Connect to PostgreSQL and run migrations
2. Start the Quick Indexer (real-time sync)
3. Start the Batch Indexer (historical backfill)
4. Start the HTTP health check server on port 3000

### 4. Verify Operation

```bash
# Check health endpoint
curl http://localhost:3000/health

# Expected response:
# {"status":"healthy","timestamp":"2025-10-14T12:00:00Z"}

# Check database for indexed blocks
psql postgresql://postgres:postgres@localhost:5432/postgres \
  -c "SELECT COUNT(*) as blocks, MIN(number) as min_block, MAX(number) as max_block FROM block_header;"
```

### 5. Monitor Progress

Watch the logs to see indexing progress:

```bash
# Quick indexer logs show real-time block processing
[quick_index] Starting quick indexer service
[quick_index] Indexed block 20000000

# Batch indexer logs show backfill progress
[batch_index] Starting batch indexer service
[batch_index] Backfilling from block 19999000 to 0
[batch_index] Indexed batch: blocks 19999000-19998000 (1000 blocks in 12.3s)
```

## Development Workflow

### Available Commands

| Command | Description |
|---------|-------------|
| `make help` | Show all available commands |
| `make dev-setup` | Complete dev setup (deps + environment) |
| `make dev-up` | Start PostgreSQL database |
| `make dev-down` | Stop development environment |
| `make dev-clean` | Clean environment (remove volumes) |
| `make run-indexer` | Run production indexer service |
| `make run-cli ARGS='...'` | Run legacy CLI tool |
| `make test` | Run all tests |
| `make lint` | Run clippy linter |
| `make format` | Format code with rustfmt |
| `make build` | Build release binary |
| `make docker-build` | Build Docker images |

### Local Development

```bash
# Start development environment
make dev-up

# Run indexer (in another terminal)
make run-indexer

# Run tests
make test

# Stop environment when done
make dev-down
```

### Docker Development

```bash
# Build and run with Docker Compose
docker compose -f docker/docker-compose.local.yml up -d

# View logs
docker compose -f docker/docker-compose.local.yml logs -f indexer

# Stop services
docker compose -f docker/docker-compose.local.yml down
```

## Configuration

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `DB_CONNECTION_STRING` | Yes | - | PostgreSQL connection URL |
| `NODE_CONNECTION_STRING` | Yes | - | Ethereum RPC endpoint URL |
| `ROUTER_ENDPOINT` | No | `0.0.0.0:3000` | HTTP health check server address |
| `RUST_LOG` | No | `info` | Logging level (`error`, `warn`, `info`, `debug`, `trace`) |
| `INDEX_TRANSACTIONS` | No | `false` | Whether to index transaction data (headers only by default) |
| `START_BLOCK_OFFSET` | No | `1024` | Blocks before latest to start backfill from |
| `IS_DEV` | No | `false` | Enable development mode (loads `.env` file) |

### Indexing Strategy Configuration

The indexer behavior is controlled through the `IndexingConfig` structure. While defaults are production-ready, you can customize behavior programmatically:

```rust
use fossil_headers_db::indexer::lib::{IndexingConfig, start_indexing_services};

let config = IndexingConfig::builder()
    .db_conn_string("postgresql://...")
    .node_conn_string("https://...")
    .should_index_txs(false)
    .max_retries(10)              // Max retries per block
    .poll_interval(10)            // Seconds between polls (quick indexer)
    .rpc_timeout(300)             // RPC timeout in seconds
    .rpc_max_retries(5)           // Max RPC retries
    .index_batch_size(1000)       // Blocks per batch
    .start_block_offset(1024)     // Start backfill from latest - offset
    .build()?;
```

## API Endpoints

### Health Check

```bash
GET /health
```

Returns the health status of the indexer service.

**Response:**
```json
{
  "status": "healthy",
  "timestamp": "2025-10-14T12:00:00Z"
}
```

**Use Case**: Load balancer health checks, monitoring systems

### MMR State (Future)

```bash
GET /mmr
GET /mmr/<block_number>
```

These endpoints will be implemented to provide MMR state information for the Light Client.

## Deployment

### AWS ECS Deployment

The indexer is designed for production deployment on AWS ECS using Fargate:

1. **Build and push Docker images:**

```bash
# Build images
make docker-build

# Tag and push to ECR
aws ecr get-login-password --region us-west-2 | docker login --username AWS --password-stdin YOUR_ACCOUNT.dkr.ecr.us-west-2.amazonaws.com

docker tag fossil-indexer:latest YOUR_ACCOUNT.dkr.ecr.us-west-2.amazonaws.com/fossil-indexer:latest
docker tag fossil-migrate:latest YOUR_ACCOUNT.dkr.ecr.us-west-2.amazonaws.com/fossil-migrate:latest

docker push YOUR_ACCOUNT.dkr.ecr.us-west-2.amazonaws.com/fossil-indexer:latest
docker push YOUR_ACCOUNT.dkr.ecr.us-west-2.amazonaws.com/fossil-migrate:latest
```

2. **Run database migrations:**

The migration service should run as a one-time ECS task before deploying the indexer:

```bash
aws ecs run-task \
    --cluster fossil-cluster \
    --task-definition fossil-migrate \
    --launch-type FARGATE \
    --network-configuration "awsvpcConfiguration={subnets=[subnet-xxx],securityGroups=[sg-xxx]}"
```

3. **Deploy indexer service:**

Deploy the indexer as a long-running ECS service with load balancer integration:

```bash
aws ecs create-service \
    --cluster fossil-cluster \
    --service-name fossil-indexer \
    --task-definition fossil-indexer \
    --desired-count 1 \
    --launch-type FARGATE \
    --load-balancers targetGroupArn=arn:aws:elasticloadbalancing:...
```

See [Deployment Guide](docs/guides/deployment.md) for complete ECS configuration details.

### Infrastructure Requirements

- **Database**: PostgreSQL 16+ with 20GB+ storage (grows with indexed data)
- **Compute**: Minimum 1 vCPU, 2GB RAM (2 vCPU, 4GB RAM recommended)
- **Network**: Outbound HTTPS access to Ethereum RPC endpoint
- **Storage**: Database storage grows approximately 10GB per million blocks

## Monitoring and Operations

### Key Metrics

Monitor these metrics in production:

- **Block Processing Rate**: Blocks indexed per minute
- **Database Lag**: Latest indexed block vs. current finalized block
- **RPC Success Rate**: Percentage of successful RPC calls
- **Gap Count**: Number of missing blocks in database
- **Health Check Response Time**: HTTP endpoint latency

### Logging

The indexer uses structured logging with the `tracing` crate:

```bash
# Set log level via RUST_LOG environment variable
export RUST_LOG=info              # Production (default)
export RUST_LOG=debug             # Development
export RUST_LOG=trace             # Debugging

# Module-specific logging
export RUST_LOG=fossil_headers_db::indexer=debug,fossil_headers_db::rpc=trace
```

### Health Monitoring

```bash
# Health check endpoint
curl http://localhost:3000/health

# Database connection check
psql $DB_CONNECTION_STRING -c "SELECT 1;"

# Check indexer progress
psql $DB_CONNECTION_STRING -c "
  SELECT
    current_latest_block_number as latest_indexed,
    backfilling_block_number as backfill_position,
    is_backfilling,
    indexing_starting_block_number as target_block,
    updated_at
  FROM index_metadata;
"
```

## Troubleshooting

### Common Issues

**1. Database Connection Errors**
```
Error: Failed to connect to database
```
**Solution**: Verify `DB_CONNECTION_STRING` is correct and PostgreSQL is running:
```bash
psql $DB_CONNECTION_STRING -c "SELECT 1;"
```

**2. RPC Rate Limiting**
```
Error: Too many requests (HTTP 429)
```
**Solution**: The indexer has built-in retry logic, but consider:
- Using a premium RPC provider with higher rate limits
- Adjusting `rpc_timeout` and `rpc_max_retries` in configuration
- Reducing `index_batch_size` to make fewer concurrent requests

**3. Slow Backfilling**
```
Batch indexer taking too long
```
**Solution**:
- Increase `index_batch_size` (default 1000, try 2000-5000)
- Use a faster RPC endpoint (dedicated node vs. shared service)
- Check database disk I/O performance

**4. Missing Blocks (Gaps)**
```
Gap detected in block sequence
```
**Solution**: The batch indexer automatically detects and fills gaps. Monitor logs:
```bash
[batch_index] Detected gap from block X to Y, filling...
```

See [Troubleshooting Guide](docs/guides/troubleshooting.md) for more details.

## Technology Stack

**Core Technologies:**
- **Rust** (stable) - High-performance systems language
- **Tokio** - Async runtime for concurrent operations
- **Axum** - Modern HTTP framework for health endpoints
- **SQLx** - Compile-time checked SQL queries
- **PostgreSQL** - Robust relational database
- **Reqwest** - HTTP client for RPC calls
- **Serde** - Serialization/deserialization
- **Tracing** - Structured logging and diagnostics

**Infrastructure:**
- **Docker** - Containerization
- **Docker Compose** - Local development orchestration
- **AWS ECS** - Production container orchestration
- **AWS RDS** - Managed PostgreSQL (production)

## Contributing

Before submitting changes:

```bash
# Format code
make format

# Run linter
make lint

# Run all tests
make test

# Or run all checks together
make dev-test
```

### Development Guidelines

1. **Type Safety**: Use the validated types (`BlockNumber`, `BlockHash`, etc.) from `types.rs`
2. **Error Handling**: Always use `Result<T>` with descriptive `BlockchainError` variants
3. **Testing**: Write tests for new functionality (unit tests in module, integration tests in `tests/`)
4. **Documentation**: Add doc comments for public functions and modules
5. **Linting**: Code must pass `cargo clippy` with no warnings
6. **Formatting**: Code must be formatted with `cargo fmt`

## Project Structure

```
fossil-headers-db/
├── src/
│   ├── main.rs                    # Legacy CLI binary
│   ├── lib.rs                     # Library root with public API
│   ├── types.rs                   # Type-safe domain models
│   ├── errors.rs                  # Error types and handling
│   ├── commands/                  # Legacy CLI commands
│   ├── indexer/
│   │   ├── main.rs               # Modern indexer binary
│   │   ├── lib.rs                # Indexer orchestration
│   │   ├── batch_service.rs      # Batch indexer implementation
│   │   └── quick_service.rs      # Quick indexer implementation
│   ├── db/
│   │   └── mod.rs                # Database connection and operations
│   ├── repositories/
│   │   ├── block_header.rs       # Block header repository
│   │   └── index_metadata.rs     # Indexer metadata repository
│   ├── rpc/
│   │   └── mod.rs                # Ethereum RPC client
│   └── router/
│       └── mod.rs                # HTTP health check endpoints
├── migrations/                    # SQLx database migrations
├── tests/                        # Integration tests
├── docker/                       # Docker configurations
│   ├── Dockerfile.indexer        # Production indexer image
│   ├── Dockerfile.migrate        # Migration runner image
│   └── docker-compose.local.yml  # Local development compose
├── docs/                         # Comprehensive documentation
├── Makefile                      # Development commands
└── Cargo.toml                    # Rust dependencies
```

## Related Projects

**Fossil Ecosystem:**
- [Fossil Light Client](https://github.com/OilerNetwork/fossil-light-client) - MMR Builder and Light Client using this indexer's data
- [Fossil Monorepo](https://github.com/OilerNetwork/fossil-monorepo) - Pitchlake Coprocessor (Proving Service, API, StarkNet contracts)

**External Dependencies:**
- [RISC Zero](https://github.com/risc0/risc0) - zkVM for proof generation
- [StarkNet](https://www.starknet.io/) - L2 blockchain for proof verification

## License

**GNU General Public License v3.0**

This program is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

See [LICENSE](LICENSE) for details.

## Support

- **Documentation**: [docs/](docs/)
- **Issues**: [GitHub Issues](https://github.com/OilerNetwork/fossil-headers-db/issues)
- **Discussions**: [GitHub Discussions](https://github.com/OilerNetwork/fossil-headers-db/discussions)

---

**Built with ❤️ by the Fossil Team**
