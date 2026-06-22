# Notebook Authoring and Python SDK Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Let users author a workflow as Python code cells or decorated Python functions, deploy it as a validated DAG, and pass produced artifacts to dependent steps.

**Architecture:** A dependency-free Python SDK compiles decorated functions into the existing job-definition and workflow HTTP resources. The dashboard notebook uses the same resource contract. The worker stages successful predecessor artifacts under `/capsulet/inputs/<step-id>/`, while job outputs remain files under `/capsulet/artifacts/`.

**Tech Stack:** Python 3.10+ standard library, Rust/Axum/SQLx, Next.js/React, Playwright, Docker Compose.

---

### Task 1: Python workflow compiler

**Files:**
- Create: `sdk/python/pyproject.toml`
- Create: `sdk/python/src/capsulet/__init__.py`
- Create: `sdk/python/src/capsulet/workflow.py`
- Test: `sdk/python/tests/test_workflow.py`

- [x] Write tests proving `@task` captures source, outputs, image, and pool; `@workflow` infers edges from task-result arguments; duplicate task invocation IDs are stable; and cycles/foreign results are rejected.
- [x] Run `python -m unittest discover -s sdk/python/tests -v` and verify the new tests fail before implementation.
- [x] Implement immutable task/result/spec types and compilation without importing third-party packages.
- [x] Run the unit tests and verify they pass.

### Task 2: HTTP client and deployment

**Files:**
- Create: `sdk/python/src/capsulet/client.py`
- Modify: `sdk/python/src/capsulet/workflow.py`
- Test: `sdk/python/tests/test_client.py`

- [x] Write a fake-HTTP-server test asserting deployment upserts deterministic job definitions before the workflow definition and preserves dependency IDs.
- [x] Implement JSON requests, structured API errors, readiness, deployment, triggering, run polling, and artifact download with `urllib`.
- [x] Run all SDK tests and verify they pass.

### Task 3: Upstream artifact staging

**Files:**
- Modify: `crates/runner/src/lib.rs`
- Modify: `crates/runner/src/tests.rs`
- Modify: `crates/worker/src/worker.rs`
- Modify: `crates/worker/src/tests.rs`
- Modify: `crates/postgres/src/workflow_runs.rs`

- [x] Add failing runner tests that expect input artifacts to be mounted under `/capsulet/inputs/<producer-step>/` for process and Kubernetes runners.
- [x] Add a worker-store query returning successful predecessor artifact metadata and load bounded bytes through object storage before execution.
- [x] Extend `RunExecution` with staged input artifacts and update all runner implementations.
- [x] Run `cargo test --workspace --locked` and `cargo clippy --all-targets --all-features --locked -- -D warnings`.

### Task 4: Notebook workflow editor

**Files:**
- Modify: `dashboard/app/workflows/page.tsx`
- Modify: `dashboard/app/globals.css`
- Modify: `dashboard/app/lib/api.ts`
- Modify: `dashboard/tests/e2e/workflow-dag.spec.ts`

- [x] Replace the job-picker-first form with ordered Python cells containing code, name, runtime image, execution pool, declared output names, and dependency selectors.
- [x] Compile each cell to a job definition, then create the workflow with the returned definition IDs.
- [x] Keep the validated topology view, add keyboard-accessible cell actions, mobile layout, visible focus, and reduced-motion behavior.
- [x] Add Playwright coverage proving two cells create two jobs and one dependency.
- [x] Run `npm test`, `npm run lint`, and `npm run build` in `dashboard`.

### Task 5: Executable CSV example and documentation

**Files:**
- Create: `examples/workflows/csv_pipeline.py`
- Create: `examples/workflows/README.md`
- Modify: `README.md`

- [x] Define `generate_csv` producing `customers.csv` and `summarize_csv` consuming that staged artifact and producing `customer-summary.csv`.
- [x] Document install, deploy, trigger, wait, and artifact-download commands.
- [x] Execute the example against Docker Compose with the process runner and assert the final downloaded CSV contents.

### Task 6: End-to-end validation

- [x] Build and start Compose services with the modified images.
- [x] Verify API/dashboard/worker/scheduler readiness.
- [x] Run the SDK CSV pipeline, wait for terminal success, and download both artifacts.
- [x] Run the Playwright workflow notebook test against the Compose dashboard.
- [x] Record exact test results and any environment-only limitation in the final handoff.
