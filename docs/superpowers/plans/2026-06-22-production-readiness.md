# Capsulet Production Readiness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the documented production gaps in identity, trigger execution, evaluation, concurrency, observability, retention, Kubernetes recovery, isolation, and operational UX.

**Architecture:** PostgreSQL remains the durable coordination boundary. API credentials and RBAC protect the synchronous control plane; durable trigger events feed an independently deployable evaluator; workers lease capacity atomically and reconcile deterministic Kubernetes Jobs; metrics and cleanup run as explicit service loops. The dashboard uses a same-origin HttpOnly credential bridge and preserves the existing visual token system.

**Tech Stack:** Rust 1.96, Axum/Tower, SQLx/PostgreSQL, Tokio, kube, Prometheus, Next.js/TypeScript, Docker Compose, Helm, kind/minikube, Playwright.

---

### Task 1: Authentication, RBAC, and dashboard session boundary

**Files:**
- Create: `crates/api/src/auth.rs`
- Create: `dashboard/app/api/auth/login/route.ts`
- Create: `dashboard/app/api/auth/logout/route.ts`
- Create: `dashboard/app/login/page.tsx`
- Modify: `crates/api/src/{lib.rs,state.rs,runtime.rs,http.rs,error.rs,models.rs}`
- Modify: `crates/api/Cargo.toml`, `Cargo.toml`, `compose.yaml`, Helm values/templates
- Modify: `dashboard/app/api/capsulet/[...path]/route.ts`, `dashboard/app/lib/api.ts`, `dashboard/app/globals.css`
- Test: `crates/api/src/tests.rs`, `dashboard/tests/api.test.ts`, `dashboard/tests/e2e/auth.spec.ts`

- [ ] Add SHA-256 token digests and constant-time verification for configured credentials.
- [ ] Parse `CAPSULET_API_TOKENS` as JSON records containing `name`, `role`, and `token`; reject missing credentials unless `CAPSULET_AUTH_DISABLED=true` is explicit.
- [ ] Add `viewer < operator < admin` authorization and classify every protected route by HTTP method and resource sensitivity.
- [ ] Keep only liveness/readiness and webhook ingestion public; return structured `401` and `403` responses.
- [ ] Add `/v1/auth/me`, API tests for missing/invalid/insufficient credentials, and positive tests for every role.
- [ ] Store the dashboard credential only in a Secure/SameSite/HttpOnly cookie, forward it server-side, and add login/logout/current-user UX.
- [ ] Run `cargo test -p capsulet-api` and `npm test --prefix dashboard` expecting all tests to pass.

### Task 2: Durable trigger events and evaluator

**Files:**
- Create: `migrations/20260622100000_trigger_events.sql`
- Create: `crates/core/src/domain/trigger_event.rs`
- Create: `crates/postgres/src/trigger_events.rs`
- Create: `crates/evaluator/src/{lib.rs,service.rs,runtime.rs}`
- Modify: automation repositories, evaluator binary/config, Compose and Helm workloads
- Test: evaluator unit tests and PostgreSQL integration tests

- [ ] Persist trigger events with unique idempotency keys, payload, occurrence time, lease owner/expiry, attempt count, and terminal evaluation result.
- [ ] Implement condition-tree evaluation against the set of matched trigger names and create exactly one workflow run per satisfied event group.
- [ ] Use `FOR UPDATE SKIP LOCKED`, guarded completion, exponential retry, and dead-letter state so evaluator replicas scale safely.
- [ ] Expose evaluator liveness/readiness and run it continuously in Compose and Helm.
- [ ] Run evaluator and PostgreSQL tests with duplicate delivery, lease expiry, false conditions, and successful workflow creation.

### Task 3: Cron, SQL, webhook, and custom trigger producers

**Files:**
- Create: `crates/evaluator/src/{schedule.rs,sql_trigger.rs,custom_trigger.rs}`
- Create: `crates/api/src/webhooks.rs`
- Modify: automation validation/models, runner execution request, chart network/RBAC configuration
- Test: API/evaluator/runner integration tests and dashboard trigger form tests

- [ ] Parse timezone-aware cron expressions, persist next-fire timestamps, and claim due schedules atomically without duplicate firing.
- [ ] Execute SQL triggers with a dedicated read-only connection URL, statement timeout, single-statement validation, bounded rows/bytes, and configurable truth/result mapping.
- [ ] Ingest `POST /v1/webhooks/{automation_id}/{trigger_name}` using timestamped HMAC-SHA256 signatures, replay windows, body limits, and idempotency keys.
- [ ] Execute custom trigger images through the hardened runner contract with bounded JSON output and no control-plane credentials.
- [ ] Expand automation UX with validated per-kind fields, secret-safe webhook instructions, test actions, and last-event/error status.
- [ ] Verify cron, SQL, webhook replay rejection, and custom trigger output end-to-end.

### Task 4: Atomic execution-pool concurrency

**Files:**
- Create: `migrations/20260622110000_execution_pool_leases.sql`
- Modify: `crates/postgres/src/job_runs.rs`, worker configuration/runtime, API pool responses, dashboard pool page
- Test: concurrent PostgreSQL leasing and worker tests

- [ ] Materialize configured pool limits and acquire a pool slot in the same transaction that leases a queued run.
- [ ] Count non-expired leased/running rows, reclaim expired capacity, and preserve fair oldest-first selection across pools.
- [ ] Release capacity on every terminal/cancellation/retry path and expose limit/running/queued/available values.
- [ ] Prove with concurrent database tests that active leases never exceed the configured maximum.

### Task 5: Metrics, audit trail, and retention cleanup

**Files:**
- Create: `migrations/20260622120000_audit_and_retention.sql`
- Create: `crates/core/src/domain/retention.rs`
- Create: `crates/postgres/src/{metrics.rs,retention.rs,audit.rs}`
- Create: service metrics modules and retention loop
- Modify: API settings/security pages, Compose, Helm ServiceMonitor/values
- Test: metrics snapshots, retention integration tests, object-store deletion tests

- [ ] Export Prometheus request latency/status, queue depth, pool utilization, evaluator outcomes, worker outcomes/retries/recovery, runner duration, and cleanup counters without high-cardinality run labels.
- [ ] Persist actor/action/resource/outcome/request-id audit records for security-sensitive mutations.
- [ ] Add configurable terminal-run, log, artifact, trigger-event, audit, and session retention periods.
- [ ] Claim cleanup batches with `SKIP LOCKED`, delete object bytes before metadata, retry partial failures safely, and support dry-run reporting.
- [ ] Expose operational settings and cleanup state using existing dashboard tables, badges, spacing, focus, reduced-motion, and scrollbar conventions.

### Task 6: Kubernetes Job reconciliation and reattachment

**Files:**
- Modify: `crates/runner/src/lib.rs`, worker execution/recovery, job-attempt persistence, Kubernetes RBAC
- Test: fake Kubernetes API tests plus kind/minikube restart E2E

- [ ] Persist the deterministic Kubernetes Job name before creation and label Jobs with run/attempt identity.
- [ ] Change create to idempotent get-or-create and validate that any existing Job belongs to the expected attempt.
- [ ] On expired running leases, adopt the active attempt/Job under a new worker lease instead of creating replacement work.
- [ ] Reattach watches, cancellation, log/artifact collection, and terminal guarded writes after restart.
- [ ] Test worker termination after Job creation and prove one Job and one terminal run remain.

### Task 7: Hostile-code isolation baseline

**Files:**
- Modify: runner pod spec and validation, `charts/capsulet/templates/{jobs.yaml,rbac.yaml}`, values/schema/docs
- Create: chart NetworkPolicy and ResourceQuota/LimitRange templates
- Test: runner spec snapshots, Helm lint/template policy tests, cluster smoke tests

- [ ] Require digest-pinned images in strict mode, non-root UID/GID, read-only root filesystem, seccomp RuntimeDefault, dropped capabilities, no privilege escalation, bounded CPU/memory/ephemeral storage, and active deadlines.
- [ ] Disable service-account token mounting and host namespaces/paths; reject privileged, host-network, and unsafe override requests.
- [ ] Default-deny ingress/egress for execution pods with explicit DNS and operator-approved destination exceptions.
- [ ] Separate control-plane and execution service accounts/namespaces and document gVisor/Kata RuntimeClass as the stronger sandbox boundary.
- [ ] Surface the effective isolation posture and actionable deviations in the security page.

### Task 8: End-to-end release gate and documentation

**Files:**
- Modify: CI workflows, `compose.e2e.yaml`, Playwright specs, README/architecture/API/operations/security docs

- [ ] Run `cargo fmt --all -- --check`, strict Clippy, workspace tests, dashboard lint/typecheck/unit tests, SDK tests, Helm lint, and template schema validation.
- [ ] Run Docker Compose with authentication enabled and verify login, authoring, all trigger kinds, evaluator creation, pool throttling, metrics, audit, artifacts, and retention.
- [ ] Run kind/minikube with Kubernetes runner, kill a worker during an active Job, verify reattachment, and verify isolation/network policies.
- [ ] Record exact release commands, configuration/secrets requirements, backup/restore, capacity planning, alerting, retention, and incident procedures.
- [ ] Remove every implemented limitation from architecture docs only after its acceptance test passes.
