# Installation Guide

This guide walks you through setting up the Fossil Headers DB indexer on your local machine or server.

## Prerequisites

### Required Software

1. **Rust Toolchain** (1.70 or later)
   ```bash
   # Install rustup (Rust installer and version manager)
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

   # Verify installation
   rustc --version
   cargo --version
   ```

2. **PostgreSQL** (16 or later)

   **Option A: Install locally (macOS)**
   ```bash
   brew install postgresql@16
   brew services start postgresql@16
   ```

   **Option B: Install locally (Linux)**
   ```bash
   # Ubuntu/Debian
   sudo apt-get update
   sudo apt-get install postgresql-16 postgresql-contrib
   sudo systemctl start postgresql
   sudo systemctl enable postgresql
   ```

   **Option C: Use Docker** (Recommended for development)
   ```bash
   # Docker will be started automatically by `make dev-up`
   docker --version  # Ensure Docker is installed
   ```

3. **Git**
   ```bash
   # macOS
   brew install git

   # Linux (Ubuntu/Debian)
   sudo apt-get install git

   # Verify
   git --version
   ```

### Optional Tools

- **SQLx CLI** (for manual database migrations)
  ```bash
  cargo install sqlx-cli --version "=0.8.6" --no-default-features --features postgres
  ```

- **Docker Compose** (for containerized development)
  ```bash
  # Usually included with Docker Desktop
  docker compose version
  ```

## Installation Steps

### 1. Clone the Repository

```bash
git clone https://github.com/OilerNetwork/fossil-headers-db.git
cd fossil-headers-db
```

### 2. Install Dependencies

The project uses the Makefile for easy dependency management:

```bash
make install
```

This command will:
- Fetch all Rust dependencies via Cargo
- Install SQLx CLI for database migrations
- Verify toolchain compatibility

**Manual alternative:**
```bash
# Fetch Rust dependencies
cargo fetch

# Install SQLx CLI
cargo install sqlx-cli --version "=0.8.6" --no-default-features --features postgres
```

### 3. Set Up the Database

**Option A: Automated setup (Docker)**
```bash
make dev-up
```

This will:
- Create a Docker network (`fossil-network`)
- Start PostgreSQL 16 in a Docker container
- Wait for the database to be healthy
- Automatically run all database migrations
- Expose PostgreSQL on `localhost:5432`

**Option B: Manual setup (Local PostgreSQL)**

```bash
# Create database
createdb fossil_headers

# Set DATABASE_URL for migrations
export DATABASE_URL="postgresql://postgres:password@localhost:5432/fossil_headers"

# Run migrations
sqlx migrate run
```

### 4. Configure Environment Variables

Create a `.env` file in the project root:

```bash
cp .env.example .env
```

Edit `.env` with your configuration:

```bash
# Database connection
DB_CONNECTION_STRING=postgresql://postgres:postgres@localhost:5432/postgres

# Ethereum RPC endpoint (required - get from Alchemy, Infura, or your node)
NODE_CONNECTION_STRING=https://eth-mainnet.g.alchemy.com/v2/YOUR_API_KEY_HERE

# HTTP server configuration
ROUTER_ENDPOINT=0.0.0.0:3000
RUST_LOG=info

# Indexer settings
INDEX_TRANSACTIONS=false          # Set to true to index transaction data
START_BLOCK_OFFSET=1024          # Start backfilling from latest - 1024 blocks

# Development mode (loads .env file)
IS_DEV=true
```

**Important**: Replace `YOUR_API_KEY_HERE` with your actual Ethereum RPC provider API key.

### 5. Verify Installation

Build the project to verify everything is set up correctly:

```bash
make build
```

This compiles the project in release mode. If successful, you'll see:
```
   Compiling fossil_headers_db v0.1.0
    Finished release [optimized] target(s) in X.XXs
```

### 6. Run Tests

Verify the installation by running the test suite:

```bash
make test
```

All tests should pass. If you see failures, check:
- PostgreSQL is running and accessible
- Database migrations have been applied
- `.env` file is properly configured

## Ethereum RPC Providers

You'll need an Ethereum RPC endpoint to index block data. Here are recommended providers:

### Free Tier Options

1. **Alchemy** (Recommended)
   - Sign up at https://www.alchemy.com/
   - Create an app for "Ethereum Mainnet"
   - Copy the HTTPS endpoint
   - Free tier: 300M compute units/month

2. **Infura**
   - Sign up at https://www.infura.io/
   - Create a project
   - Copy the mainnet endpoint
   - Free tier: 100K requests/day

3. **QuickNode**
   - Sign up at https://www.quicknode.com/
   - Create an Ethereum mainnet endpoint
   - Free tier available

### Self-Hosted Node

For production deployments, consider running your own Ethereum node:

```bash
# Using Geth
geth --http --http.api eth,net,web3 --http.addr 0.0.0.0 --http.port 8545

# Using Erigon (recommended for disk efficiency)
erigon --http --http.api eth,net,web3 --http.addr 0.0.0.0 --http.port 8545
```

**Note**: Syncing a full Ethereum node requires:
- 2TB+ SSD storage
- 16GB+ RAM
- Several days to weeks for initial sync

## Database Configuration

### Local PostgreSQL Setup

If using a local PostgreSQL instance (not Docker):

```bash
# Create user and database
sudo -u postgres psql
postgres=# CREATE USER fossil WITH PASSWORD 'your_password';
postgres=# CREATE DATABASE fossil_headers OWNER fossil;
postgres=# GRANT ALL PRIVILEGES ON DATABASE fossil_headers TO fossil;
postgres=# \q

# Update .env with your credentials
DB_CONNECTION_STRING=postgresql://fossil:your_password@localhost:5432/fossil_headers
```

### Cloud Database Options

For production, consider managed PostgreSQL:

1. **AWS RDS**
   - Create PostgreSQL 16 instance
   - Enable automatic backups
   - Configure security groups for access
   - Minimum: db.t3.micro (1 vCPU, 1GB RAM)
   - Recommended: db.t3.small or larger

2. **Google Cloud SQL**
   - Create PostgreSQL 16 instance
   - Configure authorized networks
   - Enable automated backups

3. **Azure Database for PostgreSQL**
   - Create Flexible Server
   - PostgreSQL 16
   - Configure firewall rules

## Troubleshooting Installation

### Rust Installation Issues

**Problem**: `rustc` command not found after installation

**Solution**:
```bash
# Reload shell configuration
source $HOME/.cargo/env

# Or add to .bashrc / .zshrc
echo 'source $HOME/.cargo/env' >> ~/.bashrc
source ~/.bashrc
```

### Database Connection Issues

**Problem**: Cannot connect to PostgreSQL

**Solutions**:
```bash
# Check if PostgreSQL is running
pg_isready -h localhost -p 5432

# For Docker setup
docker ps | grep postgres

# Check PostgreSQL logs
# Docker:
docker compose -f docker/docker-compose.local.yml logs db

# Local (macOS):
tail -f /usr/local/var/log/postgres.log

# Local (Linux):
sudo tail -f /var/log/postgresql/postgresql-16-main.log
```

### SQLx CLI Installation Issues

**Problem**: `sqlx` command not found

**Solution**:
```bash
# Ensure Cargo bin directory is in PATH
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc

# Reinstall SQLx CLI
cargo install sqlx-cli --version "=0.8.6" --no-default-features --features postgres --force
```

### Compilation Errors

**Problem**: Build fails with dependency errors

**Solution**:
```bash
# Clean build artifacts
cargo clean

# Update dependencies
cargo update

# Rebuild
cargo build --release
```

**Problem**: OpenSSL linking errors

**Solution**:
```bash
# macOS
brew install openssl
export OPENSSL_DIR=$(brew --prefix openssl)

# Linux (Ubuntu/Debian)
sudo apt-get install libssl-dev pkg-config
```

## Next Steps

Once installation is complete:

1. **Configure the indexer**: See [Configuration](configuration.md)
2. **Run the indexer**: See [Quick Start](quick-start.md)
3. **Verify operation**: See [Testing](testing.md)
4. **Deploy to production**: See [Deployment Guide](../guides/deployment.md)

## System Requirements

### Development Environment

- **CPU**: 2+ cores
- **RAM**: 4GB minimum, 8GB recommended
- **Storage**: 10GB for source code and dependencies
- **Database Storage**: Starts small, grows ~10GB per million blocks indexed

### Production Environment

- **CPU**: 2+ vCPU recommended
- **RAM**: 4GB minimum, 8GB recommended for high-throughput indexing
- **Database Storage**: Plan for growth
  - Headers only: ~0.5KB per block = ~10GB per million blocks
  - With transactions: ~5KB per block = ~100GB per million blocks
  - Ethereum has 20M+ blocks (as of 2024)

### Network Requirements

- **Outbound HTTPS** to Ethereum RPC endpoint
- **Inbound HTTP** on port 3000 (health checks)
- **Database connection** to PostgreSQL (port 5432 by default)
- **Bandwidth**: ~1-10 Mbps for RPC calls during backfilling

## Getting Help

If you encounter issues during installation:

1. Check the [Troubleshooting Guide](../guides/troubleshooting.md)
2. Review the [GitHub Issues](https://github.com/OilerNetwork/fossil-headers-db/issues)
3. Join the [Discussion Forum](https://github.com/OilerNetwork/fossil-headers-db/discussions)
