# Repository Documentation Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Align Capsulet's architecture and operator/developer documentation with the current implementation.

**Architecture:** Treat Rust domain/runtime code, migrations, dashboard client contracts, Compose, and Helm configuration as authoritative. Separate implemented behavior from planned behavior and avoid duplicating detailed contracts across overview documents.

**Tech Stack:** Rust workspace, Axum, SQLx/PostgreSQL, object_store/S3, Kubernetes Jobs, Helm, Next.js, Markdown/Mermaid.

---

### Task 1: Reconcile the architecture documents

**Files:**
- Modify: `ARCHITECTURE.md`
- Modify: `docs/architecture.md`

- [x] **Step 1:** Replace the obsolete planning-stage system view with the implemented component, dependency, data, execution, workflow-DAG, reliability, and deployment views.
- [x] **Step 2:** Mark evaluator/event-channel/authentication/retention capabilities as future work instead of deployed behavior.
- [x] **Step 3:** Check every component and flow against crate entry points, migrations, Compose, and Helm templates.

### Task 2: Reconcile detailed reference documentation

**Files:**
- Modify: `docs/api.md`
- Modify: `docs/persistence.md`
- Modify: `docs/worker-runner.md`
- Modify as required by verified drift: `docs/development.md`, `docs/installation.md`, `docs/helm-values.md`, `docs/troubleshooting.md`, `docs/README.md`
- Modify status notices: `docs/design/authoring-workflow-mvp.md`, `docs/design/mvp-implementation-plan.md`, `docs/design/workflow-dag-graph-plan.md`

- [x] **Step 1:** Document all routes registered by the Axum router and current workflow/automation behavior.
- [x] **Step 2:** Align persistence and worker lifecycle descriptions with migrations, leases, heartbeats, DAG scheduling, retries, logs, and artifacts.
- [x] **Step 3:** Correct stale commands, environment variables, health probes, and configuration claims found in operational docs.

### Task 3: Validate the documentation set

**Files:**
- Test: all modified Markdown files

- [x] **Step 1:** Run formatting and whitespace checks with `git diff --check`.
- [x] **Step 2:** scan relative Markdown links and verify every target exists.
- [x] **Step 3:** Compare documented API paths and environment variables against source and configuration.
- [x] **Step 4:** Review the final diff for unsupported claims and accidental changes outside documentation.
