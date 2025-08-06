export DATABASE_URL="postgresql://postgres:postgres@localhost:5432/postgres"

# Install llvm-tools-preview if not already installed
rustup component add llvm-tools-preview --toolchain stable-x86_64-unknown-linux-gnu || true

# Run tests with coverage using cargo-llvm-cov.
# Note that you will have to install the tarpaulin binary via cargo install cargo-llvm-cov.
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info --no-cfg-coverage

