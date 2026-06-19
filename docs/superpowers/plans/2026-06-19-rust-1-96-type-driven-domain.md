# Rust 1.96 and Type-Driven Domain Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pin every Rust build to 1.96 and make domain parsing, retry policies, and job-run transitions reject invalid states at their construction boundaries.

**Architecture:** Keep wire and database primitives at adapter boundaries, then convert immediately to core domain types. Centralize string-to-enum parsing in `capsulet-core`, represent retry attempts with `NonZeroU32` and delays with `Duration`, and expose intent-based job-run transition events instead of arbitrary status mutation.

**Tech Stack:** Rust 1.96, Cargo workspace, thiserror, SQLx, Axum, Docker, GitHub Actions.

---

### Task 1: Pin the Rust toolchain

**Files:**
- Create: `rust-toolchain.toml`
- Modify: `Cargo.toml`
- Modify: `.github/workflows/rust.yml`
- Modify: `Dockerfile.rust`
- Modify: `crates/Dockerfile`

- [ ] **Step 1: Add the repository toolchain pin**

```toml
[toolchain]
channel = "1.96.0"
components = ["clippy", "rustfmt"]
profile = "minimal"
```

- [ ] **Step 2: Align all build declarations**

Set `workspace.package.rust-version = "1.96"`, the CI toolchain to `1.96.0`, and both Rust builder images to `rust:1.96-bookworm`.

- [ ] **Step 3: Verify the selected compiler**

Run: `rustc --version && cargo --version`
Expected: both report `1.96.0`.

### Task 2: Centralize domain enum parsing

**Files:**
- Modify: `crates/core/src/domain/job.rs`
- Modify: `crates/core/src/domain/workflow.rs`
- Modify: `crates/core/src/domain/automation.rs`
- Modify: `crates/core/src/domain/artifact.rs`
- Modify: `crates/core/src/domain/mod.rs`
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/postgres/src/rows.rs`
- Test: core domain module tests and `crates/postgres/src/tests.rs`

- [ ] **Step 1: Add a typed parse error**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unknown {kind} {value}")]
pub struct ParseDomainValueError {
    kind: &'static str,
    value: String,
}
```

- [ ] **Step 2: Implement `FromStr` beside each persisted enum**

Each implementation exhaustively maps the same canonical values emitted by `Display` and returns `ParseDomainValueError` for unknown values.

- [ ] **Step 3: Delete adapter-owned enum parsers**

Replace `parse_status(&status)?` and equivalent helpers with:

```rust
status.parse().map_err(|error: ParseDomainValueError| {
    PostgresStoreError::InvalidPersistedValue(error.to_string())
})?
```

- [ ] **Step 4: Verify parsing tests**

Run: `cargo test -p capsulet-core -p capsulet-postgres --locked`
Expected: valid values round-trip and invalid persisted values remain errors.

### Task 3: Encode job-run transition intent

**Files:**
- Modify: `crates/core/src/domain/job.rs`
- Modify: `crates/core/src/domain/mod.rs`
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/api/src/tests.rs`
- Modify: `crates/runner/src/tests.rs`

- [ ] **Step 1: Write transition-event tests**

Cover lease, start, success, failure, timeout, cancellation, retry scheduling, requeue, attempt increments, and rejection from incompatible states.

- [ ] **Step 2: Add the intent enum and exhaustive transition table**

```rust
pub enum JobRunTransition {
    Lease,
    StartAttempt,
    Succeed,
    Fail,
    Cancel,
    TimeOut,
    ScheduleRetry,
    Requeue,
}
```

Expose `JobRun::apply(transition)`; derive the target status internally and increment `attempt_count` only for `StartAttempt`. Remove public arbitrary mutation APIs after migrating callers.

- [ ] **Step 3: Verify compile-time API migration**

Run: `cargo test --workspace --all-targets --locked`
Expected: all callers express transitions as domain intent and all tests pass.

### Task 4: Strengthen retry-policy invariants

**Files:**
- Modify: `crates/core/src/domain/job_definition.rs`
- Test: `crates/core/src/domain/job_definition.rs`

- [ ] **Step 1: Preserve boundary behavior with tests**

Test that zero attempts are rejected, one attempt represents no retry, and delay seconds round-trip.

- [ ] **Step 2: Store invariant-bearing standard types**

```rust
pub struct RetryPolicy {
    max_attempts: NonZeroU32,
    delay: Duration,
}
```

Keep numeric getters for persistence compatibility while making an invalid in-memory policy unrepresentable; remove the test-only unchecked constructor.

- [ ] **Step 3: Verify core tests**

Run: `cargo test -p capsulet-core --locked`
Expected: all domain tests pass.

### Task 5: Full verification

**Files:**
- Modify only files required by compiler, formatter, or lint findings.

- [ ] **Step 1: Format**

Run: `cargo fmt --all -- --check`
Expected: exit code 0.

- [ ] **Step 2: Lint all targets and features**

Run: `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
Expected: exit code 0.

- [ ] **Step 3: Run the complete workspace test suite**

Run: `cargo test --workspace --all-targets --locked`
Expected: exit code 0.

- [ ] **Step 4: Run PostgreSQL-backed tests**

Run the PostgreSQL service, set `CAPSULET_TEST_DATABASE_URL`, and run `cargo test -p capsulet-postgres --locked -- --test-threads=1`.
Expected: all persistence tests pass against migrated schema.

- [ ] **Step 5: Validate the production Rust builder**

Run: `docker build -f Dockerfile.rust --build-arg PACKAGE=capsulet-api --build-arg BIN=capsulet-api .`
Expected: the Rust 1.96 builder produces the runtime image successfully.
