# Rust Guidelines Implementation Plan

This document outlines a priority-structured plan to apply Rust best practices from `RUST_CONTEXT.md` to the fossil-headers-db codebase.

## 🔴 HIGH PRIORITY - Critical Issues (Security & Correctness)

### P1.1: Fix Unsafe Error Handling Patterns
**Risk Level**: CRITICAL
**Files**: `src/db/mod.rs`, `src/rpc/mod.rs`, `src/commands/mod.rs`
**Issue**: Production-unsafe `.unwrap_or_default()` and silent error handling

**Tasks**:
- [x] Replace `.unwrap_or_default()` in hex conversions (`src/db/mod.rs:243-244`)
- [x] Fix silent error logging in batch operations (`src/commands/mod.rs:91-94`)
- [x] Add proper error propagation for RPC failures (`src/rpc/mod.rs:147-167`)
- [x] Remove any remaining `unwrap()` calls in production code paths

**Success Criteria**: No `.unwrap*()` calls in production code, all errors properly propagated or handled with explicit decision

### P1.2: Implement Domain-Specific Error Types
**Risk Level**: HIGH
**Files**: New `src/errors.rs`, `src/lib.rs`
**Issue**: Generic error handling makes debugging and error recovery difficult

**Tasks**:
- [x] Add `thiserror` dependency to `Cargo.toml`
- [x] Create `src/errors.rs` with blockchain-specific error types
- [x] Implement `BlockchainError` enum with variants for:
  - Invalid hex format errors
  - RPC connection failures
  - Database transaction failures
  - Block not found errors
- [x] Update all modules to use domain-specific errors
- [x] Add error context throughout the codebase

**Success Criteria**: All errors are typed and provide clear context for debugging

### P1.3: Add Type Safety with Newtype Pattern
**Risk Level**: HIGH
**Files**: New `src/types.rs`, all modules using primitive types
**Issue**: Critical domain concepts lack type safety, risk of parameter confusion

**Tasks**:
- [x] Create `src/types.rs` with domain types:
  - `BlockNumber(i64)` with validation
  - `BlockHash(String)` with hex validation
  - `TransactionHash(String)` with hex validation
  - `Address(String)` with address format validation
- [x] Update function signatures throughout codebase
- [x] Add validation logic in type constructors
- [x] Update database operations to use typed parameters

**Success Criteria**: Cannot accidentally pass wrong primitive types, compile-time prevention of common mistakes

### P1.4: Fix Mixed Async Runtime Usage
**Risk Level**: HIGH
**Files**: `src/commands/mod.rs`, `Cargo.toml`
**Issue**: Using both `tokio` and `async_std` can cause runtime conflicts

**Tasks**:
- [x] Remove `async_std` dependency from `Cargo.toml`
- [x] Replace `async_std::task::sleep` with `tokio::time::sleep` (`src/commands/mod.rs:177`)
- [x] Audit all async operations for consistency
- [x] Update imports to use only `tokio` primitives

**Success Criteria**: Single async runtime throughout application, no runtime conflicts

## 🟡 MEDIUM PRIORITY - Maintainability & Robustness

### P2.1: Add Comprehensive API Documentation
**Risk Level**: MEDIUM
**Files**: All public API files
**Issue**: <10% documentation coverage makes maintenance difficult

**Tasks**:
- [x] Add module-level documentation to all `mod.rs` files
- [x] Document all public functions in `src/rpc/mod.rs`
- [x] Document all public functions in `src/db/mod.rs`
- [x] Document all public functions in `src/repositories/`
- [x] Add usage examples to complex functions
- [x] Add `#[doc(hidden)]` for internal public items

**Success Criteria**: All public APIs documented with examples, clear module purposes

### P2.2: Implement Bounded Concurrency
**Risk Level**: MEDIUM
**Files**: `src/commands/mod.rs`, `src/indexer/batch_service.rs`
**Issue**: Unbounded task spawning can overwhelm system resources

**Tasks**:
- [x] Add `tokio::sync::Semaphore` for task limiting
- [x] Implement timeout handling for spawned tasks
- [x] Add backpressure handling in batch operations
- [x] Configure reasonable concurrency limits
- [x] Add resource cleanup for failed tasks

**Success Criteria**: System remains stable under load, configurable concurrency limits

### P2.3: Add Unit Tests with Mocking
**Risk Level**: MEDIUM
**Files**: All `src/` modules, new test files
**Issue**: Limited unit test coverage, difficult to test in isolation

**Tasks**:
- [x] Add `mockall` dependency for mocking
- [x] Create mock traits for external dependencies (RPC, Database)
- [x] Add unit tests for core business logic functions
- [x] Add error path testing for all error scenarios
- [x] Separate unit tests from integration tests
- [x] Add test utilities for common mock setups

**Success Criteria**: >80% unit test coverage, isolated testing of business logic

### P2.4: Improve Module Organization
**Risk Level**: MEDIUM
**Files**: `src/main.rs`, `src/lib.rs`, all modules
**Issue**: Inconsistent API boundaries, potential circular dependencies

**Tasks**:
- [x] Define clear module boundaries and responsibilities
- [x] Update `src/lib.rs` with clean re-exports
- [x] Move `src/main.rs` logic to use library interface only
- [x] Hide internal implementation details
- [x] Create facade patterns for complex subsystems
- [x] Document module interaction patterns

**Success Criteria**: Clear separation of concerns, stable public API, no circular dependencies

## 🟢 LOW PRIORITY - Code Quality & Modern Patterns

### P3.1: Upgrade to Modern Standard Library Features
**Risk Level**: LOW
**Files**: `src/rpc/mod.rs`, `Cargo.toml`
**Issue**: Using deprecated external crates for features now in std

**Tasks**:
- [x] Replace `once_cell::sync::Lazy` with `std::sync::OnceLock`
- [x] Remove `once_cell` dependency if no longer needed
- [x] Update other dependencies to latest stable versions
- [x] Review for other modern std replacements

**Success Criteria**: Reduced dependency footprint, using latest std features

### P3.2: Add Property-Based Testing
**Risk Level**: LOW
**Files**: New test files for parsing logic
**Issue**: Complex hex parsing lacks thorough testing

**Tasks**:
- [x] Add `proptest` dependency (already present)
- [x] Create property tests for hex conversion functions
- [x] Add property tests for block number validation
- [x] Add property tests for hash format validation
- [x] Create test generators for blockchain data

**Success Criteria**: Edge cases covered, increased confidence in parsing logic

### P3.3: Implement Builder Patterns
**Risk Level**: LOW
**Files**: Configuration structs, complex constructors
**Issue**: Complex configuration creation is error-prone

**Tasks**:
- [x] Identify complex configuration structs
- [x] Implement builder patterns for multi-field structs
- [x] Add validation in builder methods
- [x] Create convenient defaults and presets
- [x] Update construction sites to use builders

**Success Criteria**: Easier configuration creation, reduced construction errors

### P3.4: Add Enhanced Lint Configurations
**Risk Level**: LOW
**Files**: `Cargo.toml`, `src/lib.rs`
**Issue**: Missing strict linting for code quality

**Tasks**:
- [x] Add workspace-level lint configuration
- [x] Enable `clippy::pedantic` and `clippy::nursery`
- [x] Add project-specific lint rules
- [ ] Configure CI to enforce linting
- [x] Address all new lint warnings

**Success Criteria**: Consistent code quality, automated quality checks

## Implementation Strategy

### Phase 1: Security & Correctness (Weeks 1-2)
Focus on HIGH PRIORITY items that could cause production issues:
- P1.1: Fix unsafe error handling
- P1.2: Add domain-specific errors
- P1.3: Implement type safety
- P1.4: Fix async runtime issues

### Phase 2: Robustness (Weeks 3-4)
Address MEDIUM PRIORITY maintainability issues:
- P2.1: Add comprehensive documentation
- P2.2: Implement bounded concurrency
- P2.3: Add unit testing with mocks
- P2.4: Improve module organization

### Phase 3: Quality & Modernization (Week 5)
Polish with LOW PRIORITY improvements:
- P3.1: Upgrade to modern std features
- P3.2: Add property-based testing
- P3.3: Implement builder patterns
- P3.4: Enhanced lint configuration

## Progress Tracking

Mark tasks as completed by checking the boxes above. Each priority section should be completed before moving to the next phase.

### Completion Status
- [x] Phase 1 Complete (All HIGH PRIORITY items)
- [x] Phase 2 Complete (All MEDIUM PRIORITY items)
- [x] Phase 3 Complete (All LOW PRIORITY items)

## References
- `RUST_CONTEXT.md` - Rust guidelines and best practices
- `CLAUDE.md` - Project-specific context and standards
- Rust API Guidelines: https://rust-lang.github.io/api-guidelines/
- Rust Performance Book: https://nnethercote.github.io/perf-book/
