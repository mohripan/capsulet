# Reliability and Recovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make job execution survive worker loss, preserve completed workflow checkpoints, expose meaningful health checks, and verify the complete Docker/Kubernetes product path.

**Architecture:** PostgreSQL remains the source of truth. Workers renew owner-bound leases while a runner is active; stale leases are safely requeued, and workflow reconciliation uses persisted successful step runs as checkpoints. API liveness is process-only while readiness verifies PostgreSQL, and worker/scheduler processes expose HTTP probes suitable for Compose and Kubernetes.

**Tech Stack:** Rust 2024, Tokio, Axum, SQLx/PostgreSQL, Next.js, Docker Compose, Helm, kind/minikube, Playwright.

---

### Task 1: Owner-bound job heartbeats

**Files:**
- Create: `migrations/20260619090000_job_heartbeats.sql`
- Modify: `crates/postgres/src/job_runs.rs`
- Modify: `crates/postgres/src/tests.rs`

- [ ] Add `heartbeat_at TIMESTAMPTZ` and an index for active runs.
- [ ] Write a PostgreSQL integration test proving only the lease owner can renew an active lease.
- [ ] Add `heartbeat_run(id, worker_id, lease_seconds) -> bool` using one conditional update:

```sql
UPDATE job_runs
SET heartbeat_at = now(), lease_expires_at = now() + ($3 * interval '1 second')
WHERE id = $1 AND lease_owner = $2 AND status IN ('leased', 'running')
```

- [ ] Ensure lease acquisition initializes `heartbeat_at`, terminal transitions clear it, and stale recovery clears it.
- [ ] Run `cargo test -p capsulet-postgres --locked` and expect all tests to pass (database-only tests may skip when `DATABASE_URL` is absent).

### Task 2: Heartbeat during execution

**Files:**
- Modify: `crates/worker/src/lib.rs`
- Modify: `crates/worker/src/runtime.rs`
- Modify: `crates/worker/src/tests.rs`

- [ ] Add the heartbeat operation to `WorkerStore` and its PostgreSQL implementation.
- [ ] Write worker tests that observe repeated heartbeat calls and abort finalization when lease ownership is lost.
- [ ] Run runner execution and a `tokio::time::interval` in `tokio::select!`; renew every configured interval until execution completes.
- [ ] Validate `heartbeat_seconds > 0`, `lease_seconds > heartbeat_seconds`, and expose `CAPSULET_WORKER_HEARTBEAT_SECONDS`.
- [ ] Run `cargo test -p capsulet-worker --locked` and expect all tests to pass.

### Task 3: Explicit workflow resume from durable checkpoints

**Files:**
- Modify: `crates/postgres/src/workflow_runs.rs`
- Modify: `crates/postgres/src/tests.rs`
- Modify: `crates/api/src/store.rs`
- Modify: `crates/api/src/http.rs`
- Modify: `crates/api/src/tests.rs`
- Modify: `docs/api.md`

- [ ] Write a persistence test where step A succeeded, step B failed, resume is requested, and only B receives a new job run.
- [ ] Implement a transactionally locked `resume_workflow_run`: retain successful step runs as checkpoints, delete/recreate failed descendants, reset the workflow to running, and reject active/successful runs.
- [ ] Add `POST /v1/workflow-runs/{id}/resume` and return the updated run with step state.
- [ ] Confirm normal reconciliation never duplicates a successful step because `(workflow_run_id, workflow_step_id)` remains unique.
- [ ] Run API and PostgreSQL test suites and expect all tests to pass.

### Task 4: Liveness, readiness, and service probes

**Files:**
- Modify: `crates/api/src/store.rs`
- Modify: `crates/api/src/http.rs`
- Modify: `crates/api/src/tests.rs`
- Create: `crates/core/src/health.rs`
- Modify: `crates/worker/src/runtime.rs`
- Modify: `crates/scheduler/src/lib.rs`

- [ ] Add `/livez` returning 200 without dependencies and `/readyz` returning 200 only when `SELECT 1` succeeds; retain `/healthz` as a compatibility alias.
- [ ] Start lightweight worker/scheduler probe servers with liveness and readiness based on recent successful loop ticks and database reachability.
- [ ] Add tests for healthy and unavailable stores plus stale tick detection.
- [ ] Run `cargo test --workspace --all-targets --locked` and expect all tests to pass.

### Task 5: Container and Helm reliability wiring

**Files:**
- Modify: `crates/Dockerfile`
- Modify: `Dockerfile.rust`
- Modify: `compose.yaml`
- Modify: `charts/capsulet/templates/deployments.yaml`
- Modify: `charts/capsulet/values.yaml`
- Modify: `charts/capsulet/values.schema.json`

- [ ] Run Rust images as UID/GID 10001 and use application probe binaries/endpoints without shell-only assumptions.
- [ ] Add health checks and restart policies to API, dashboard, worker, and scheduler; configure dependency ordering from readiness.
- [ ] Add worker/scheduler ports and Kubernetes startup/readiness/liveness probes with configurable timing.
- [ ] Render with `helm lint charts/capsulet` and `helm template capsulet charts/capsulet`; expect both to succeed.

### Task 6: Documentation, screenshot, and end-to-end evidence

**Files:**
- Create: `docs/images/capsulet-dashboard.png`
- Modify: `README.md`
- Modify: `docs/development.md`

- [ ] Start `docker compose up --build -d`, wait for all health checks, and exercise create workflow → fail step → resume → success via the HTTP API.
- [ ] Capture the running dashboard with Playwright at 1440×900 into `docs/images/capsulet-dashboard.png`.
- [ ] Rewrite README claims to match the current DAG workflow, automation, runner, artifact, retry, checkpoint/resume, and health capabilities; embed the screenshot using a relative Markdown image.
- [ ] Build local images, load them into kind (or minikube), install the Helm chart, wait for rollout, and run the chart connection test.
- [ ] Run formatting, clippy, Rust tests, dashboard tests/build, Compose validation, Helm lint/template, and report exact results.
