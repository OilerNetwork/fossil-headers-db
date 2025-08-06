# Fossil Headers DB - Ethereum Blockchain Indexer

[![CI](https://github.com/OilerNetwork/fossil-headers-db/actions/workflows/ci.yml/badge.svg)](https://github.com/OilerNetwork/fossil-headers-db/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/codecov/c/github/OilerNetwork/fossil-headers-db)](https://codecov.io/gh/OilerNetwork/fossil-headers-db)
[![License](https://img.shields.io/badge/License-GNU%20GPL-blue)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org/)

High-performance Ethereum blockchain indexer that fetches and stores block headers and transaction data from genesis to the latest block. Built in Rust for production deployment on AWS ECS.

## Quick Start

```bash
# Setup development environment
make dev-setup

# Run production indexer
make run-indexer

# Run tests
make test
```

## Development Commands

| Command | Description |
|---------|-------------|
| `make help` | Show all available commands |
| `make install` | Install all dependencies |
| `make build` | Build project in release mode |
| `make test` | Run all tests |
| `make lint` | Run clippy linter |
| `make format` | Format code with rustfmt |
| `make coverage` | Generate test coverage report |

## Environment Commands

| Command | Description |
|---------|-------------|
| `make dev-up` | Start local Indexer and PostgreSQL database |
| `make dev-down` | Stop local development environment |
| `make dev-clean` | Stop and clean environment (removes volumes) |
| `make dev-setup` | Complete setup (install + start environment) |
| `make dev-test` | Full test suite (format + lint + test) |

## Application Commands

| Command | Description |
|---------|-------------|
| `make run-indexer` | Run modern indexer service (production) |
| `make run-cli ARGS='update --start 0 --end 1000'` | Run legacy CLI update mode |
| `make run-cli ARGS='fix --start 0 --end 1000'` | Run legacy CLI fix mode |

## Docker Commands

| Command | Description |
|---------|-------------|
| `make docker-build` | Build Docker images |
| `make docker-push REGISTRY=your-registry.com` | Push images to registry |

## Configuration

Create `.env` file:
```bash
DB_CONNECTION_STRING=postgresql://postgres:postgres@localhost:5432/postgres
NODE_CONNECTION_STRING=<your_ethereum_rpc_endpoint>
ROUTER_ENDPOINT=0.0.0.0:3000
RUST_LOG=info
INDEX_TRANSACTIONS=false
START_BLOCK_OFFSET=1024
```

## Architecture

- **`fossil_indexer`**: Production service with batch + real-time indexing
- **`fossil_headers_db`**: Legacy CLI tool for debugging/maintenance
- **Database**: PostgreSQL with automated migrations
- **Health Checks**: HTTP endpoint on port 3000
- **Deployment**: Optimized for AWS ECS

## API Endpoints

- `GET /health` - Health check for load balancers
- `GET /mmr` - Latest Merkle Mountain Range state
- `GET /mmr/<block_number>` - MMR proof for specific block

## Project Structure

```
├── src/                 # Rust source code
├── migrations/          # Database migrations
├── tests/               # Integration tests
├── docker/              # Docker configurations
├── build-scripts/       # Build and deployment scripts
├── docs/                # Documentation
└── Makefile            # Development commands
```
