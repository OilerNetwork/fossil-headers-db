#!/bin/sh

set -e

export TEST_DB_CONNECTION_STRING="postgresql://postgres:postgres@localhost:5433/fossil_test"

# Run tests with coverage. Using tarpaulin here.
# Note that you will have to install the tarpaulin binary via cargo install cargo-tarpaulin.
# Outputting html for easier viewing.
cargo tarpaulin