# Sprint 004 Backlog

This is the working backlog for Sprint 004: Job Control and Recovery.

Status legend:

- `todo`: not started
- `doing`: in progress
- `blocked`: waiting on a decision or prerequisite
- `done`: complete

## State Machine and Persistence

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S4-STATE-001 | done | Audit job state transition rules | Current allowed/rejected transitions are documented in tests |
| S4-STATE-002 | done | Add cancellation and timeout transition tests | `cancelled` and `timed_out` behavior is explicit and enforced |
| S4-STATE-003 | done | Add guarded terminal state persistence | Stale workers cannot overwrite terminal states |
| S4-STATE-004 | done | Add clear domain errors for illegal transitions | API/worker can surface understandable transition failures |

## Cancellation

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S4-CANCEL-001 | done | Add repository cancel operation | Queued runs can be atomically marked `cancelled` |
| S4-CANCEL-002 | done | Add API cancel endpoint | `POST /v1/jobs/runs/{id}/cancel` returns updated run state |
| S4-CANCEL-003 | done | Add API cancellation tests | queued, running, terminal, and missing run paths are covered |
| S4-CANCEL-004 | done | Add CLI cancel command | `capsulet cancel <run-id>` calls the API and prints state |
| S4-CANCEL-005 | done | Add worker cancellation observation | Worker can notice cancellation while waiting for Kubernetes completion |
| S4-CANCEL-006 | done | Delete or stop Kubernetes Jobs on cancellation | Running Kubernetes work stops and final state becomes `cancelled` |
| S4-CANCEL-007 | done | Add cancellation smoke job | Local smoke can cancel a predictable long-running job |

## Timeout and Retry

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S4-TIMEOUT-001 | done | Map runner timeout to `timed_out` | Active deadline or wait timeout does not become generic failure |
| S4-TIMEOUT-002 | done | Add timeout tests | runner and worker timeout mapping is covered |
| S4-TIMEOUT-003 | done | Add timeout smoke fixture | Local Kubernetes smoke can produce `timed_out` deterministically |
| S4-RETRY-001 | done | Add minimal retry policy model | max attempts and fixed delay can be represented |
| S4-RETRY-002 | done | Persist retry policy fields | Retry settings survive API/worker restarts |
| S4-RETRY-003 | done | Schedule retries for failed/timed-out runs | Eligible runs move to `retry_scheduled` and then back to `queued` |
| S4-RETRY-004 | done | Stop retrying after exhaustion | Exhausted runs end in terminal failure/timeout state |
| S4-RETRY-005 | done | Prevent retry after cancellation | Cancelled runs remain terminal |
| S4-RETRY-006 | done | Add retry tests | scheduling, exhaustion, and cancellation interactions are covered |

## Lease Recovery

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S4-LEASE-001 | done | Add expired lease recovery query | Expired leased runs can be found and requeued safely |
| S4-LEASE-002 | done | Run recovery from worker loop | Worker calls recovery before leasing or on a configured cadence |
| S4-LEASE-003 | done | Add recovery idempotency tests | Repeated recovery calls do not corrupt state |
| S4-LEASE-004 | done | Decide running Job reattachment boundary | Sprint documents whether running Job reattach is implemented or deferred |

## Kubernetes Cleanup

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S4-K8S-001 | done | Add Job TTL cleanup config | Runner can set `ttlSecondsAfterFinished` from config |
| S4-K8S-002 | done | Expose cleanup settings in Helm values | Values and schema include cleanup options |
| S4-K8S-003 | done | Test cleanup field rendering | Unit tests prove Job TTL is rendered when enabled |
| S4-K8S-004 | done | Document cleanup behavior | Local guide explains how to inspect Jobs before cleanup |

## API, CLI, and Docs

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S4-API-001 | done | Document cancel endpoint | API docs include request, response, and errors |
| S4-API-002 | done | Document status meanings | User-facing docs define queued/running/succeeded/failed/cancelled/timed_out/retry_scheduled |
| S4-CLI-001 | done | Update CLI docs | CLI docs include `cancel` and retry/timeout examples |
| S4-DOC-001 | done | Update local Kubernetes smoke checklist | Checklist covers success, failure, timeout, retry, cancellation, and cleanup |
| S4-DOC-002 | done | Update worker runner docs | Worker docs explain cancellation, timeout, retry, and lease recovery |
| S4-DOC-003 | done | Add troubleshooting notes | Stuck leased/running and Kubernetes delete failures are documented |

## Quality

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S4-QA-001 | done | Keep formatting passing | `cargo fmt --check` passes |
| S4-QA-002 | done | Keep clippy passing | `cargo clippy --workspace --all-targets -- -D warnings` passes |
| S4-QA-003 | done | Keep workspace tests passing | `cargo test --workspace` passes |
| S4-QA-004 | done | Keep Helm checks passing | `helm lint` and `helm template` pass |
| S4-QA-005 | done | Complete minikube smoke | Manual smoke records success, failure, timeout, retry, cancellation, and cleanup outcomes |

## Completed Notes

- Added `POST /v1/jobs/runs/{id}/cancel` and `capsulet cancel <run-id>`.
- Added guarded final-state persistence so stale worker completions do not overwrite cancellation.
- Added runner cancellation checks and Kubernetes Job/pod deletion on cancellation, including Helm RBAC for pod deletion.
- Added `timed_out` mapping for Kubernetes `DeadlineExceeded` and runner wait deadlines.
- Added minimal fixed-delay retry policy on job definitions with persisted `retry_max_attempts`, `retry_delay_seconds`, and `retry_ready_at`.
- Added worker promotion of ready retries and recovery of expired `leased` runs.
- Added Kubernetes `ttlSecondsAfterFinished` cleanup config through execution pools and Helm values/schema.
- Seeded example definitions now include `job_hello_python`, `job_sleep_python`, `job_fail_python`, and `job_timeout_python`.
- Running Kubernetes Job reattachment after a worker crash is intentionally deferred; Sprint 004 recovery requeues expired `leased` and `running` rows and may create a replacement Job after the lease expires.
- Minikube smoke completed against the Helm install: `run_s4_hello` succeeded, `run_s4_fail` retried then failed after exhaustion, `run_s4_timeout2` timed out, and `run_s4_cancel4` cancelled with no remaining Kubernetes Job or run pod.

## Sprint Risks

- Running cancellation may require changing the runner wait loop. Keep the API surface small and poll cancellation between Kubernetes status checks.
- Retry scheduling can expand into a scheduler service. Keep Sprint 004 to fixed delay and worker/database-driven retry readiness.
- Worker restart recovery can become reconciliation. Start with expired leased runs; document running Job reattachment if it does not fit.
- Kubernetes cleanup must not remove persisted logs. Cleanup should delete Kubernetes resources only, not Capsulet run records.

## Sprint Exit

Move any unfinished `todo` items back into `planning/backlog/product-backlog.md` or into Sprint 005 planning.
