# Sprint 003 Backlog

This is the working backlog for Sprint 003: Kubernetes Job Runner.

Status legend:

- `todo`: not started
- `doing`: in progress
- `blocked`: waiting on a decision or prerequisite
- `done`: complete

## Kubernetes Runner

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S3-K8S-001 | todo | Choose and add Rust Kubernetes client stack | Decision is documented and runner crate dependencies are added |
| S3-K8S-002 | todo | Add Kubernetes runner module | Runner implements the existing `Runner` boundary |
| S3-K8S-003 | todo | Render Kubernetes Job specs | Unit tests prove run, definition, and pool config map to expected Job spec |
| S3-K8S-004 | todo | Create Kubernetes Jobs | Worker can create a Job for a leased run in a local cluster |
| S3-K8S-005 | todo | Make Job creation idempotent by run ID | Re-running worker for the same run does not create duplicate Jobs |
| S3-K8S-006 | todo | Watch Job terminal state | Kubernetes success/failure maps to run success/failure |
| S3-K8S-007 | todo | Add Kubernetes runner configuration | Namespace, runner mode, and timeout defaults can be configured locally |

## Job Definitions and Execution Pools

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S3-JOB-001 | todo | Add worker job definition lookup | Worker can load the definition referenced by a leased run |
| S3-JOB-002 | todo | Add PostgreSQL definition lookup adapter | Store can fetch one job definition by ID |
| S3-JOB-003 | todo | Handle missing job definitions in worker | Missing definition returns a clear worker error |
| S3-POOL-001 | todo | Load static execution pool config | Worker can resolve `mini` and `large` pool settings |
| S3-POOL-002 | todo | Apply pool resources to Job pods | Rendered Job includes requests and limits for the selected pool |
| S3-POOL-003 | todo | Apply pool placement to Job pods | Rendered Job includes node selectors and tolerations |
| S3-POOL-004 | todo | Reject missing pool config before Job creation | Unknown or unconfigured pool fails without creating Kubernetes resources |

## Logs

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S3-LOG-001 | todo | Add bounded log persistence migration | Log table exists with run/attempt relationship and documented size cap |
| S3-LOG-002 | todo | Add log repository methods | Logs can be saved and fetched by run ID |
| S3-LOG-003 | todo | Capture pod logs after completion | Worker stores bounded logs for a completed Kubernetes Job |
| S3-LOG-004 | todo | Keep missing logs non-fatal | Successful run is not failed only because no pod logs are available |
| S3-LOG-005 | todo | Add log persistence tests | Save/fetch and cap behavior are covered |

## API and CLI

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S3-API-001 | todo | Add `GET /v1/jobs/runs/{id}/logs` | API returns captured logs for one run |
| S3-API-002 | todo | Add logs API errors | Missing run and missing logs return clear errors |
| S3-API-003 | todo | Add API logs tests | Success and failure paths are covered |
| S3-CLI-001 | todo | Add `capsulet status <run-id>` | CLI prints status-focused run details |
| S3-CLI-002 | todo | Add `capsulet logs <run-id>` | CLI prints captured logs |
| S3-CLI-003 | todo | Add CLI parsing and formatting tests | New commands are covered without requiring a live API |

## Helm and Local Kubernetes

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S3-HELM-001 | todo | Add runner config values | Chart values expose runner mode, namespace, and timeout defaults |
| S3-HELM-002 | todo | Update worker environment template | Worker deployment receives Kubernetes runner config |
| S3-HELM-003 | todo | Verify RBAC for execution | Chart grants Job and pod-log access needed by the runner |
| S3-HELM-004 | todo | Keep chart checks passing | `helm lint` and `helm template` pass |
| S3-DOC-001 | todo | Add local Kubernetes runner guide | Contributor can run hello Python through kind or minikube |
| S3-DOC-002 | todo | Update installation and development docs | Docs reflect the new local cluster execution path |

## Quality

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S3-QA-001 | todo | Keep formatting passing | `cargo fmt --check` passes |
| S3-QA-002 | todo | Keep clippy passing | `cargo clippy --workspace --all-targets -- -D warnings` passes |
| S3-QA-003 | todo | Keep workspace tests passing | `cargo test --workspace` passes |
| S3-QA-004 | todo | Add manual Kubernetes smoke checklist | Smoke checklist records exact commands and expected run/log outcome |

## ADR Candidates

Create ADRs only if the implementation forces durable choices:

- Kubernetes client stack.
- PostgreSQL-backed bounded logs as a temporary Sprint 003 storage choice.
- Kubernetes Job naming and idempotency strategy.
- Execution namespace model if it becomes more than a local default.

## Sprint Risks

- Kubernetes client integration can consume the sprint. Keep the runner path to one Job and one container.
- Log storage can become object storage work. Keep Sprint 003 logs bounded and temporary.
- Helm install can expand into packaging work. Prefer a local `cargo run` plus cluster workflow unless the runner lands early.
- Watching Kubernetes resources can be flaky in local clusters. Add clear timeouts and manual diagnostics.

## Sprint Exit

Move any unfinished `todo` items back into `planning/backlog/product-backlog.md` or into Sprint 004 planning.
