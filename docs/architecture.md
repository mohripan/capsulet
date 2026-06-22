# Architecture Overview

Capsulet stores durable control-plane state in PostgreSQL and executes jobs through a pluggable runner. The Kubernetes runner creates isolated Kubernetes Jobs; local process and deterministic stub runners support development and tests.

The detailed system design and implementation boundaries are in the repository-level [ARCHITECTURE.md](../ARCHITECTURE.md).

## Components

- **API:** Axum HTTP control plane for job definitions, workflow DAGs, automations and trigger metadata, job/workflow runs, health, logs, cancellation, and artifacts.
- **Scheduler:** PostgreSQL polling loop that fires due legacy interval automations and advances every ready node in workflow DAGs.
- **Worker:** promotes retries, recovers expired leases, leases queued jobs, heartbeats active work, invokes a runner, and persists outcomes.
- **Runner library:** stub, trusted local-process, and Kubernetes Job execution backends.
- **PostgreSQL adapter:** SQLx persistence for definitions, runs, attempts, leases/heartbeats, workflow dependency edges, automation metadata, logs, and artifact metadata.
- **Object storage adapter:** filesystem or S3-compatible storage for Python scripts, complete large logs, and artifact bytes.
- **Dashboard:** Next.js UI that reaches the API through a same-origin server proxy.
- **CLI:** HTTP client for submission and job-run operations.
- **Evaluator:** durable cron, SQL, webhook, and isolated custom-plugin trigger production and condition evaluation.

## Dependency view

```mermaid
flowchart LR
    ui[Dashboard] --> api[API]
    cli[CLI] --> api
    api --> db[(PostgreSQL)]
    api --> store[(Object storage)]
    scheduler[Scheduler] --> db
    worker[Worker] --> db
    worker --> store
    worker --> runner[Runner]
    runner --> kube[Kubernetes API]
    kube --> pods[Job pods]
```

PostgreSQL is the source of truth for metadata and state transitions. It is also the durable work queue. Object storage is not authoritative for run state; it contains bytes referenced by PostgreSQL metadata.

## Job lifecycle

1. The API validates a job definition, input contract, and execution pool, then inserts a `queued` run.
2. A worker promotes due retries, recovers expired leases, and leases the oldest queued run with row locking.
3. The worker creates an attempt and heartbeats the run while its runner is active.
4. The runner returns a terminal outcome, logs, and collected artifacts.
5. The worker stores an inline log preview, offloads complete large logs and artifacts to object storage, and commits a guarded final state.
6. Failed or timed-out runs may enter `retry_scheduled`; cancellation is terminal.

Lease recovery provides at-least-once execution. Deterministic attempt-scoped Kubernetes Job identity lets a replacement worker validate and reattach to existing work without incrementing the attempt.

## Workflow lifecycle

Workflows are DAGs. Dependency edges can express fan-out and fan-in; the API rejects cycles and invalid edges. Omitting the dependency field creates a position-ordered compatibility chain, while an explicit empty list creates independent roots.

An automation or manual action creates a workflow run. On each tick, the scheduler queues every step whose predecessors have succeeded. It reconciles step outcomes into the workflow result. Resume keeps successful checkpoints and retries only the unfinished part of the graph.

## Automations

The authoring model supports named `manual`, `schedule`, `sql`, `webhook`, and `custom` trigger definitions, custom-trigger plugin metadata, and validated boolean condition expressions. Trigger events, leases, correlation, retries, and exactly-once workflow-run creation are durable in PostgreSQL.

## Storage keys

- job-definition scripts: `bundles/job-definitions/<job-definition-id>/main.py`
- submitted run scripts: `bundles/<run-id>/main.py`
- complete large logs: `logs/<run-id>/stdout.log`
- artifacts: `artifacts/<run-id>/<name>`

Logs up to 64 KiB are stored inline. Larger logs keep a bounded inline preview and an object-backed `stdout.log` artifact.

## Deployment

Docker Compose supplies PostgreSQL, MinIO, API, scheduler, evaluator, a stub-runner worker, dashboard, and Mailpit for local evaluation. The Helm chart deploys the platform services, migration/bucket initialization jobs, separated service accounts, health/metrics, static execution pools, default-deny execution networking, and optional bundled PostgreSQL/MinIO. External PostgreSQL and S3-compatible storage are the production-shaped dependency mode.

API, scheduler, evaluator, and worker expose `/livez`, `/readyz`, and `/metrics`. Readiness depends on PostgreSQL connectivity.

## Security boundary

Execution pods run non-root with a read-only root filesystem, dropped capabilities, RuntimeDefault seccomp, no service-account token, bounded writable volumes, and default-deny egress. A separate unprivileged execution ServiceAccount and optional RuntimeClass are supported. Operators running hostile multi-tenant code should configure gVisor, Kata, or another sandboxed runtime; ordinary Linux containers are not a virtual-machine boundary.
