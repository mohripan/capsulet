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
| S3-K8S-001 | done | Choose and add Rust Kubernetes client stack | Decision is documented and runner crate dependencies are added |
| S3-K8S-002 | done | Add Kubernetes runner module | Runner implements the existing `Runner` boundary |
| S3-K8S-003 | done | Render Kubernetes Job specs | Unit tests prove run, definition, and pool config map to expected Job spec |
| S3-K8S-004 | done | Create Kubernetes Jobs | Worker can create a Job for a leased run in a local cluster |
| S3-K8S-005 | done | Make Job creation idempotent by run ID | Re-running worker for the same run does not create duplicate Jobs |
| S3-K8S-006 | done | Watch Job terminal state | Kubernetes success/failure maps to run success/failure |
| S3-K8S-007 | done | Add Kubernetes runner configuration | Namespace, runner mode, and timeout defaults can be configured locally |

## Job Definitions and Execution Pools

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S3-JOB-001 | done | Add worker job definition lookup | Worker can load the definition referenced by a leased run |
| S3-JOB-002 | done | Add PostgreSQL definition lookup adapter | Store can fetch one job definition by ID |
| S3-JOB-003 | done | Handle missing job definitions in worker | Missing definition returns a clear worker error |
| S3-POOL-001 | done | Load static execution pool config | Worker can resolve `mini` and `large` pool settings |
| S3-POOL-002 | done | Apply pool resources to Job pods | Rendered Job includes requests and limits for the selected pool |
| S3-POOL-003 | done | Apply pool placement to Job pods | Rendered Job includes node selectors and tolerations |
| S3-POOL-004 | done | Reject missing pool config before Job creation | Unknown or unconfigured pool fails without creating Kubernetes resources |

## Logs

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S3-LOG-001 | done | Add bounded log persistence migration | Log table exists with run/attempt relationship and documented size cap |
| S3-LOG-002 | done | Add log repository methods | Logs can be saved and fetched by run ID |
| S3-LOG-003 | done | Capture pod logs after completion | Worker stores bounded logs for a completed Kubernetes Job |
| S3-LOG-004 | done | Keep missing logs non-fatal | Successful run is not failed only because no pod logs are available |
| S3-LOG-005 | done | Add log persistence tests | Save/fetch and cap behavior are covered |

## API and CLI

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S3-API-001 | done | Add `GET /v1/jobs/runs/{id}/logs` | API returns captured logs for one run |
| S3-API-002 | done | Add logs API errors | Missing run and missing logs return clear errors |
| S3-API-003 | done | Add API logs tests | Success and failure paths are covered |
| S3-CLI-001 | done | Add `capsulet status <run-id>` | CLI prints status-focused run details |
| S3-CLI-002 | done | Add `capsulet logs <run-id>` | CLI prints captured logs |
| S3-CLI-003 | done | Add CLI parsing and formatting tests | New commands are covered without requiring a live API |

## Helm and Local Kubernetes

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S3-HELM-001 | done | Add runner config values | Chart values expose runner mode, namespace, and timeout defaults |
| S3-HELM-002 | done | Update worker environment template | Worker deployment receives Kubernetes runner config |
| S3-HELM-003 | done | Verify RBAC for execution | Chart grants Job and pod-log access needed by the runner |
| S3-HELM-004 | done | Keep chart checks passing | `helm lint` and `helm template` pass |
| S3-DOC-001 | done | Add local Kubernetes runner guide | Contributor can run hello Python through kind or minikube |
| S3-DOC-002 | done | Update installation and development docs | Docs reflect the new local cluster execution path |

## Quality

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S3-QA-001 | done | Keep formatting passing | `cargo fmt --check` passes |
| S3-QA-002 | done | Keep clippy passing | `cargo clippy --workspace --all-targets -- -D warnings` passes |
| S3-QA-003 | done | Keep workspace tests passing | `cargo test --workspace` passes |
| S3-QA-004 | done | Add manual Kubernetes smoke checklist | Smoke checklist records exact commands and expected run/log outcome |

## Completed Notes

Kubernetes runner foundation completed:

- Added `kube` and `k8s-openapi` on versions compatible with Rust 1.87.
- Added `KubernetesRunner` behind the existing runner boundary.
- Added Kubernetes Job rendering from run, job definition, and execution pool config.
- Added idempotent Job creation behavior for already-existing run-derived Job names.
- Added terminal Job polling and bounded pod-log capture.
- Added execution pool YAML parsing and pool-to-pod spec mapping.

Persistence and log surface completed:

- Added a generic `JobRunLogRepository` boundary in `capsulet-core`.
- Added PostgreSQL-backed bounded log persistence and migration.
- Added ADR 0010 documenting PostgreSQL logs as a temporary implementation and object storage as the preferred future backend.
- Added `GET /v1/jobs/runs/{id}/logs`.
- Added `capsulet status <run-id>` and `capsulet logs <run-id>`.

Helm and local workflow completed:

- Added `Dockerfile.rust` and `.dockerignore` for local API/worker images.
- Added worker runner values, environment, loop mode, and execution pool ConfigMap mount.
- Added API chart config for `0.0.0.0:8080`, example seeding, and execution pool names.
- Added local minikube documentation in `docs/local-kubernetes-runner.md`.

Live minikube verification completed:

- Built `capsulet-api:dev` and `capsulet-worker:dev` into minikube with `Dockerfile.rust`.
- Installed the Helm chart with API and worker enabled, scheduler/evaluator/dashboard disabled, local image pull policy, and Kubernetes runner mode.
- Used an in-cluster temporary `postgres:16-alpine` deployment after `host.minikube.internal` was not reachable from the minikube network.
- Labeled the minikube node with `capsulet.dev/pool=mini` so default `mini` pool jobs can schedule.
- Submitted `job_hello_python`; Kubernetes created `job.batch/capsulet-run-1780408845522`, pulled `python:3.12-slim`, and completed the runner pod with exit code 0.
- Verified persisted status and logs in PostgreSQL: `run_1780408845522` reached `succeeded` with `attempt_count = 1`, and `job_run_logs.log_text` contained `hello from capsulet`.
- Live validation found and fixed Kubernetes Job name sanitization for run IDs containing underscores.

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
