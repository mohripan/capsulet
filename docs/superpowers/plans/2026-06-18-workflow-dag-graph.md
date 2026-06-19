# Workflow DAG Graph Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Evolve workflow definitions, execution, APIs, and dashboard authoring from linear chains to validated deterministic DAGs while preserving legacy behavior.

**Architecture:** PostgreSQL stores workflow-local dependency edges, while a pure core `WorkflowGraph` owns validation, topological ordering, and ready-step decisions. The API validates requests before transactional persistence; the scheduler reconciles every active run from persisted step/job states and atomically starts every ready node. The dashboard edits dependencies and renders an accessible topological preview.

**Tech Stack:** Rust, Axum, SQLx/PostgreSQL, Next.js/React/TypeScript, Docker Compose, Playwright.

---

### Task 1: Core graph domain

**Files:** Create `crates/core/src/domain/workflow_graph.rs`; modify `workflow.rs`, `domain/mod.rs`, and `lib.rs`.

- [ ] Add failing unit tests for chains, fan-out/fan-in, invalid endpoints, cross-workflow nodes, duplicate/self edges, direct/indirect cycles, deterministic ordering, and ready-node discovery.
- [ ] Add `WorkflowStepDependency`, `WorkflowGraphError`, and `WorkflowGraph` using `BTreeMap` adjacency and Kahn sorting by `(position, id)`.
- [ ] Add dependencies to `WorkflowDefinition`, including a legacy constructor path that derives a chain when the API field is omitted.
- [ ] Run `cargo test -p capsulet-core` and expect all graph tests to pass.

### Task 2: Transactional persistence

**Files:** Create `migrations/20260618120000_workflow_step_dependencies.sql`; modify `crates/postgres/src/workflows.rs`, `rows.rs`, and `tests.rs`.

- [ ] Add a workflow-scoped edge table with composite foreign keys, duplicate/self-edge protection, indexes, and cascade deletion.
- [ ] Save steps and edges in one transaction, deleting edges before replacing steps.
- [ ] Load edges in stable order and reconstruct validated workflow definitions.
- [ ] Add integration tests for round trips, duplicate rejection, and cascade behavior; run `cargo test -p capsulet-postgres` against PostgreSQL.

### Task 3: API contract and validation

**Files:** Modify `crates/api/src/models.rs`, `http.rs`, `store.rs`, `tests.rs`, and `docs/api.md`.

- [ ] Extend create/update/detail models with optional input dependencies and always-present output dependencies.
- [ ] Convert omitted dependencies to a position-ordered chain; preserve an explicitly empty dependency list as independent roots.
- [ ] Map graph validation failures to actionable HTTP 400 responses before persistence.
- [ ] Add API tests for DAG create/read/update, cycles, invalid endpoints, duplicates, and omitted legacy dependencies.

### Task 4: DAG scheduler reconciliation

**Files:** Modify `crates/postgres/src/workflow_runs.rs`, migration constraints, and persistence/API tests.

- [ ] Replace the single-current-position algorithm with one transaction per run guarded by `FOR UPDATE SKIP LOCKED` semantics.
- [ ] Synchronize step-run states from job runs, start all deterministic ready steps idempotently, and keep a unique `(workflow_run_id, workflow_step_id)` invariant.
- [ ] Complete only when every node succeeds; fail when any prerequisite failure leaves no runnable/active work; cancel all active jobs when a workflow is cancelled.
- [ ] Test fan-out, fan-in, idempotent ticks, success, failure blocking, and cancellation.

### Task 5: Dashboard dependency editor and graph preview

**Files:** Modify `dashboard/app/lib/api.ts`, `dashboard/app/workflows/page.tsx`, `dashboard/app/globals.css`; add focused client components/tests as needed.

- [ ] Add per-step “Depends on” multi-select controls with self-dependency prevention and local cycle feedback.
- [ ] Render a responsive, keyboard-accessible topological lane preview with visible root/fan-out/fan-in semantics and a clear empty state.
- [ ] Submit dependency edges through the API and retain server validation errors verbatim.
- [ ] Run `npm test`, `npm run lint`, and `npm run build` in `dashboard`.

### Task 6: Production verification

**Files:** Modify Docker or test configuration only where verification exposes a concrete issue.

- [ ] Run workspace formatting, `cargo test --workspace`, and strict Clippy.
- [ ] Build and start the Docker Compose stack, verify health checks, create and execute a fan-out/fan-in DAG through HTTP, and inspect persisted run states.
- [ ] Run Playwright against the containerized dashboard to create a DAG, verify the preview, save it, reload it, and confirm API persistence.
- [ ] Validate the existing Helm chart with `helm lint`; use kind/minikube only if Compose cannot exercise a deployment-specific requirement.
- [ ] Record exact commands and outcomes in the final handoff.
