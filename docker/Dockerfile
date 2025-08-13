FROM rust:slim-bookworm AS base

# Install common system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/app

# Migration stage
FROM base AS migration
# Install sqlx-cli with a compatible version
RUN cargo install sqlx-cli --version "=0.8.6" --no-default-features --features postgres

# Copy migrations and scripts
COPY migrations ./migrations
COPY build-scripts ./build-scripts

# Make scripts executable
RUN chmod +x ./build-scripts/run-migrations.sh

CMD ["./build-scripts/run-migrations.sh"]

# Builder stage for indexer
FROM base AS builder
COPY . .
RUN cargo build --release

# Final indexer stage
FROM debian:12-slim AS indexer
WORKDIR /usr/app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl-dev \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy the built binary
COPY --from=builder /usr/app/target/release/fossil_indexer .

# Health check endpoint
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3000/health || exit 1

# Expose port
EXPOSE 3000

ENV RUST_LOG=info

CMD ["/usr/app/fossil_indexer"]
