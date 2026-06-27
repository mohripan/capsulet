# Operations

## Scheduler observability

The scheduler exposes:

- `/readyz`: database readiness
- `/metrics`: Prometheus metrics including queue and workflow advancement state from the PostgreSQL adapter
- structured logs for polling ticks, interval automation advancement, and workflow reconciliation

Operational checks:

```powershell
curl http://127.0.0.1:8082/readyz
curl http://127.0.0.1:8082/metrics
docker logs capsulet-scheduler
```

If workflow runs remain queued:

1. Confirm scheduler readiness.
2. Confirm worker readiness.
3. Check execution-pool concurrency metrics.
4. Inspect `workflow_runs` and `job_runs` for terminal blockers.
5. Check Keycloak/API auth only if the dashboard cannot issue commands; scheduler/worker use the database directly.

## Horizontal worker scaling

Workers are horizontally scalable when they share the same database and object store. Leasing uses guarded transitions and pool-level concurrency checks, so multiple workers can poll concurrently.

Recommended settings:

- unique `CAPSULET_WORKER_ID` per replica
- `CAPSULET_WORKER_LOOP=true`
- `CAPSULET_WORKER_POLL_SECONDS=1-5`
- pool concurrency sized to Kubernetes cluster capacity
- Kubernetes API rate limits monitored through worker logs and metrics

## Backpressure

Backpressure is enforced at two layers:

- API admission can reject new manual job and manual workflow-trigger submissions before they are persisted.
- Worker leasing enforces execution-pool concurrency and retry scheduling after work is queued.

Configure API admission with:

- `CAPSULET_ADMISSION_MAX_QUEUED_RUNS`: global queued job-run cap.
- `CAPSULET_ADMISSION_MAX_QUEUED_RUNS_PER_POOL`: per execution-pool queued job-run cap.
- `CAPSULET_ADMISSION_MAX_QUEUED_WORKFLOW_RUNS`: queued workflow-run cap.

When a configured cap is reached, the API returns `429` with `queue_overloaded`. If the API cannot read admission state from PostgreSQL, it returns `503` with `admission_unavailable`.

Operators should alert on:

- growing queued runs by pool
- long queue age
- high retry/dead-letter counts
- worker readiness failures
- Kubernetes Job creation failures

## Reconciliation loops

The worker reattaches to deterministic Kubernetes Jobs after lease expiry. The scheduler reconciles workflow DAGs from durable run/step state and can resume failed workflow runs from completed checkpoints.

## Load tests

Use the compose smoke script for functional checks. For load characterization, run batches of manual submissions against a controlled pool limit and watch:

- API latency
- queued/running/succeeded/failed counts
- pool saturation
- PostgreSQL CPU/connections
- Kubernetes API throttling

Document results with the Capsulet version, database size, worker replica count, pool limits, Kubernetes version, and node shape.

## Alpha gate checklist

Before an alpha release:

- `cargo test --workspace --all-targets`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo fmt --check`
- dashboard lint/test/build
- `helm lint charts/capsulet`
- `helm template capsulet charts/capsulet`
- `.\scripts\compose-smoke.ps1`
- minikube smoke from `docs/minikube-smoke.md`
- `/openapi.json` validates in a client generator
- Keycloak login and temporary admin login both verified
- threat model reviewed against deployment settings
