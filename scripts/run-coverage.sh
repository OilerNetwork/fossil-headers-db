export DATABASE_URL="postgresql://postgres:postgres@localhost:5433/fossil_test"

# Run tests with coverage using cargo-llvm-cov.
# Note that you will have to install the tarpaulin binary via cargo install cargo-llvm-cov.
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info

