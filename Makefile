# Fossil Headers DB - Ethereum Blockchain Indexer
# High-performance indexer for AWS ECS deployment

.PHONY: help install build test lint format clean dev-up dev-down dev-clean run-indexer run-cli docker-build docker-push coverage

# Default target
help: ## Show this help message
	@echo "Fossil Headers DB - Ethereum Blockchain Indexer"
	@echo ""
	@echo "Available commands:"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  %-15s %s\n", $$1, $$2}'

# Development Setup
install: ## Install all dependencies
	@echo "Installing Rust dependencies..."
	cargo fetch
	@echo "Installing sqlx-cli..."
	cargo install sqlx-cli --version "=0.8.6" --no-default-features --features postgres
	@echo "Dependencies installed successfully"

# Build & Test
build: ## Build the project in release mode
	@echo "Building project..."
	cargo build --release

test: ## Run all tests
	@echo "Running tests..."
	./build-scripts/run-test.sh

lint: ## Run clippy linter
	@echo "Running clippy..."
	cargo clippy --lib --bins -- -D warnings
	@echo "Running clippy on tests (with panic allowed)..."
	cargo clippy --tests -- -D warnings -A clippy::unwrap_used -A clippy::expect_used -A clippy::panic

format: ## Format code using rustfmt
	@echo "Formatting code..."
	cargo fmt --all

coverage: ## Generate test coverage report
	@echo "Generating coverage report..."
	./build-scripts/run-coverage.sh

# Development Environment
dev-up: ## Start local development environment
	@echo "Starting local development environment..."
	docker compose -f docker/docker-compose.local.yml up -d
	@echo "Database available at localhost:5432"
	@echo "Waiting for database to be ready..."
	@sleep 5
	@echo "Running migrations..."
	DATABASE_URL=postgresql://postgres:postgres@localhost:5432/postgres sqlx migrate run
	@echo "Development environment ready!"

dev-down: ## Stop local development environment
	@echo "Stopping local development environment..."
	docker compose -f docker/docker-compose.local.yml down

dev-down-volumes: ## Stop local development environment and remove volumes
	@echo "Stopping local development environment and removing volumes..."
	docker compose -f docker/docker-compose.local.yml down --volumes

dev-clean: ## Stop and clean local development environment (removes volumes)
	@echo "Cleaning local development environment..."
	docker compose -f docker/docker-compose.local.yml down -v
	docker volume prune -f

# Application
run-indexer: ## Run the modern indexer service (production)
	@echo "Starting modern indexer service..."
	@if [ -f .env ]; then \
		echo "Loading environment variables from .env..."; \
		export $$(grep -v '^#' .env | xargs) && cargo run --bin fossil_indexer; \
	else \
		echo "No .env file found, using system environment variables"; \
		cargo run --bin fossil_indexer; \
	fi

run-cli: ## Run the legacy CLI tool (debugging)
	@echo "Available CLI commands:"
	@echo "  make run-cli ARGS='update --start 0 --end 1000 --loopsize 10'"
	@echo "  make run-cli ARGS='fix --start 0 --end 1000'"
	@if [ "$(ARGS)" = "" ]; then \
		echo "Usage: make run-cli ARGS='<command>'"; \
		exit 1; \
	fi
	cargo run --bin fossil_headers_db -- $(ARGS)

# Docker
docker-build: ## Build Docker images
	@echo "Building Docker images..."
	docker build -f docker/Dockerfile.indexer -t fossil-indexer .
	docker build -f docker/Dockerfile.migrate -t fossil-migrate .
	@echo "Docker images built successfully"

docker-push: ## Push Docker images to registry (requires REGISTRY variable)
	@if [ "$(REGISTRY)" = "" ]; then \
		echo "Usage: make docker-push REGISTRY=your-registry.com"; \
		exit 1; \
	fi
	@echo "Pushing to registry: $(REGISTRY)"
	docker tag fossil-indexer $(REGISTRY)/fossil-indexer:latest
	docker tag fossil-migrate $(REGISTRY)/fossil-migrate:latest
	docker push $(REGISTRY)/fossil-indexer:latest
	docker push $(REGISTRY)/fossil-migrate:latest

# Maintenance
clean: ## Clean build artifacts
	@echo "Cleaning build artifacts..."
	cargo clean
	@echo "Build artifacts cleaned"

migrate: ## Run database migrations (requires DATABASE_URL)
	@echo "Running database migrations..."
	@if [ "$(DATABASE_URL)" = "" ]; then \
		echo "Usage: make migrate DATABASE_URL=postgresql://user:pass@host:port/db"; \
		exit 1; \
	fi
	sqlx migrate run

# Quick development workflow
dev-setup: install dev-up ## Complete development setup (install deps + start environment)
	@echo "Development environment fully set up!"

dev-test: format lint test ## Run full test suite (format + lint + test)
	@echo "All tests passed!"

# CI/CD helpers
ci-build: build docker-build ## Build everything for CI/CD

ci-test: format lint test ## Run all checks for CI/CD
