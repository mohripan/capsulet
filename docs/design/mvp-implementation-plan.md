# Capsulet MVP Implementation Plan

> Historical implementation plan. Its linear workflow model has been superseded by the implemented workflow DAG engine. See [Architecture Overview](../architecture.md) for current behavior.

## Goal

Build Capsulet to the point where a user can create their own job definitions, create workflows, create automations, trigger them manually or on an interval, and inspect the resulting work end to end through the dashboard.

## Architecture

The existing job-run queue remains the execution primitive. New product objects sit above it:

- job definitions store reusable Python scripts
- workflows define ordered steps that each submit one job run
- automations create workflow runs from manual or interval triggers
- the scheduler advances due interval automations and workflow runs
- the worker continues to execute individual job runs

## Tech Stack

- Rust workspace with Axum API, SQLx PostgreSQL adapter, scheduler, worker, and core domain crate
- PostgreSQL migrations in `migrations/`
- S3-compatible object storage through `capsulet-storage`
- Next.js dashboard in `dashboard/`
- Docker Compose for local end-to-end smoke

## Task 1: Authoring Foundation

Files:

- Modify `crates/core/src/domain/ids.rs`
- Modify `crates/core/src/domain/job_definition.rs`
- Modify `crates/core/src/domain/mod.rs`
- Modify `crates/core/src/lib.rs`
- Modify `crates/postgres/src/lib.rs`
- Modify `crates/api/src/lib.rs`
- Modify `dashboard/app/lib/api.ts`
- Modify `dashboard/app/runs/runs-client.tsx`
- Create or modify `dashboard/app/job-definitions/page.tsx`
- Add migration under `migrations/`

Steps:

- Add reusable job definition CRUD endpoints.
- Store user-created Python scripts as object storage bundles.
- Let run submission select API-backed job definitions.
- Add `GET /v1/execution-pools`.
- Wire dashboard execution pool selectors to the API.
- Remove fake execution-pool management metrics.

Verification:

- `cargo test -p capsulet-api`
- `cargo test -p capsulet-postgres`
- `npm run lint`
- `npm run build`
- Compose smoke: create job definition, submit run, inspect logs/artifacts.

## Task 2: Workflow Foundation

Files:

- Add workflow domain types in `crates/core/src/domain/`
- Add repository methods in `crates/postgres/src/lib.rs`
- Add workflow API routes in `crates/api/src/lib.rs`
- Add migrations for workflow definitions and steps
- Add dashboard workflow list/create/detail pages

Steps:

- Implement linear workflow definitions.
- Each workflow step references one job definition and one execution pool.
- Add API CRUD.
- Add dashboard create/list/detail flow.

Verification:

- Create a workflow with two steps from the dashboard.
- API returns workflow detail with ordered steps.

## Task 3: Workflow Runs and Orchestration

Files:

- Add workflow run domain types in `crates/core/src/domain/`
- Add workflow run persistence in `crates/postgres/src/lib.rs`
- Add orchestration logic in `crates/scheduler/`
- Add workflow run API routes in `crates/api/src/lib.rs`
- Add dashboard workflow run list/detail pages

Steps:

- Create workflow runs.
- Start first queued step by creating a job run.
- Advance to next step when previous job run succeeds.
- Fail workflow run when a step fails, times out, or is cancelled.
- Link each step run to the underlying job run.

Verification:

- Trigger a two-step workflow.
- Worker executes both underlying job runs.
- Dashboard shows workflow run and links each step to job run detail.

## Task 4: Automations

Files:

- Add automation domain types in `crates/core/src/domain/`
- Add automation persistence and migrations
- Add automation API routes
- Extend scheduler to create workflow runs for due interval automations
- Add dashboard automation list/create/detail pages

Steps:

- Implement manual automation trigger.
- Implement interval trigger with `interval_seconds`.
- Track `next_fire_at` and `last_triggered_at`.
- Scheduler creates workflow runs for due enabled automations.
- Add dashboard trigger button.

Verification:

- Create a manual automation and trigger it.
- Create an interval automation with `interval_seconds=3600`.
- Lower interval in local smoke for fast verification.
- Confirm scheduler creates workflow runs without duplicates.

## Task 5: Example Email Workflow

Files:

- Add `examples/send-email/send_email.py`
- Add docs in `docs/examples.md` or relevant quickstart
- Add dashboard copy only where the feature is implemented

Steps:

- Provide a Python email script template using SMTP environment variables.
- Document recipient example `mohripan16@gmail.com`.
- Keep credentials out of source and docs.
- Document how a user can paste the script into a job definition.

Verification:

- Local smoke can run the script in dry-run mode.
- Docs explain real SMTP variables needed for actual email delivery.

## Task 6: Docs, Sprint Planning, and MVP Smoke

Files:

- Update `ROADMAP.md`
- Update `docs/architecture.md`
- Update `docs/api.md`
- Update `docs/development.md`
- Update `docs/troubleshooting.md`
- Add Sprint 009 and Sprint 010 plans/backlogs if needed

Steps:

- Keep alpha after authoring/workflow MVP.
- Document live API endpoints.
- Document dashboard authoring flow.
- Document Compose smoke commands.
- Document minikube smoke commands when Kubernetes execution is used.

Verification:

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `npm run lint`
- `npm run build`
- Docker Compose end-to-end smoke.
