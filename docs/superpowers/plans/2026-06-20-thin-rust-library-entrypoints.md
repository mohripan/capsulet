# Thin Rust Library Entrypoints Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the scheduler, storage, and worker `lib.rs` files module-only entrypoints while preserving behavior and consistently styling every dashboard scrollbar.

**Architecture:** Each crate keeps a thin public facade in `lib.rs`; existing implementation moves unchanged into a responsibility-named module and is re-exported to preserve current crate-root API paths. Dashboard overflow surfaces join the existing cross-browser scrollbar selector set rather than introducing page-specific colors.

**Tech Stack:** Rust 2024 workspace, Cargo, Next.js, CSS

---

### Task 1: Refactor scheduler entrypoint

**Files:**
- Create: `crates/scheduler/src/service.rs`
- Modify: `crates/scheduler/src/lib.rs`

- [ ] Move the scheduler runtime, health server, and environment parsing implementation from `lib.rs` to `service.rs` without changing behavior.
- [ ] Declare `pub mod service;` and re-export `run` from the crate root so `capsulet_scheduler::run()` remains valid.
- [ ] Run `cargo test -p capsulet-scheduler`; expect all tests and compilation to pass.

### Task 2: Refactor storage entrypoint

**Files:**
- Create: `crates/storage/src/object_store.rs`
- Modify: `crates/storage/src/lib.rs`

- [ ] Move object-store traits, adapters, key validation, errors, and colocated tests from `lib.rs` to `object_store.rs` unchanged.
- [ ] Declare `pub mod object_store;` and re-export its public API so existing imports such as `capsulet_storage::ObjectStore` remain valid.
- [ ] Run `cargo test -p capsulet-storage`; expect all unit tests to pass.

### Task 3: Refactor worker entrypoint

**Files:**
- Create: `crates/worker/src/worker.rs`
- Modify: `crates/worker/src/lib.rs`
- Modify: `crates/worker/src/tests.rs`

- [ ] Move the worker store boundary, execution use case, helpers, outcome, and error implementation from `lib.rs` to `worker.rs`.
- [ ] Keep `runtime` and test module declarations in `lib.rs`, expose `worker`, and re-export its public API to preserve crate-root imports.
- [ ] Update unit-test parent paths to the new implementation module while keeping tests behaviorally unchanged.
- [ ] Run `cargo test -p capsulet-worker`; expect all unit tests to pass.

### Task 4: Normalize dashboard scrollbars

**Files:**
- Modify: `dashboard/app/globals.css`

- [ ] Add `.dagStepEditor`, `.dagCanvas`, and responsive `.navList` to the existing Firefox and WebKit scrollbar color, size, track, thumb, hover, and corner selectors.
- [ ] Run `npm run lint` and the dashboard test command defined in `package.json`; expect both to pass.

### Task 5: Verify the workspace

**Files:**
- Verify: `Cargo.toml`
- Verify: `dashboard/package.json`

- [ ] Run `cargo fmt --all -- --check`; expect no formatting changes.
- [ ] Run `cargo test --workspace`; expect the complete Rust suite to pass.
- [ ] Review `git diff --check`; expect no whitespace errors.
