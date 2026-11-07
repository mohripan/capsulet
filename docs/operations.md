<!-- capsulet-claims: CAP-AGENT-001, CAP-AGENT-002, CAP-JOB-001, CAP-WORKFLOW-001, CAP-AUTOMATION-001, CAP-PERSISTENCE-001, CAP-OBSERVABILITY-001, CAP-HELM-001 -->
# Operations

This guide operates the implemented workflow/job services and experimental agent, automation, and
Helm surfaces. It does not imply that the planned durable graph worker or runtime-wide correctness
admission exists.

## Agent runtime observability

An agent run is executed by the graph worker, and `GET /v1/agent-runs/{id}/events` returns its whole log: every node, what each one spent, and why the run ended. `GET /v1/agent-runs/{id}` summarizes that, with the reason in `execution.detail` and what a waiting run needs in `execution.waiting_on`.

The graph worker serves `/livez`, `/readyz`, and `/metrics` on `CAPSULET_GRAPH_WORKER_HEALTH_ADDR`, default `0.0.0.0:8084`. Replicas are safe: runs are leased under a fencing epoch, so two workers can never both advance one. It reads:

| Variable | Default | Meaning |
| --- | --- | --- |
| `CAPSULET_GRAPH_WORKER_ID` | `graph-worker` | The worker's identity in the run log. |
| `CAPSULET_GRAPH_WORKER_POLL_SECONDS` | `2` | How long to wait when there is nothing to do. |
| `CAPSULET_GRAPH_WORKER_LEASE_SECONDS` | `60` | How long a lease lasts. The worker heartbeats while a node runs, so a node may take longer than this. |
| `CAPSULET_GRAPH_WORKER_TENANT` | unset | Restricts a dedicated fleet to one tenant's runs. |
| `CAPSULET_OLLAMA_URL` | unset | The local model. Unset, model nodes fail and say so. |

A run stuck `queued` means no graph worker is running. A run stuck `waiting` needs a decision at `POST /v1/agent-runs/{id}/decisions`. A job node waits for its job, so a run stuck `running` on one may mean the job worker is not running.

Important tables:

- `graph_definitions`, `graph_nodes`, `graph_ports`, and `graph_hyperedges`
- `agent_definitions` and `agent_runs`
- `ir_runs`, `ir_run_events`, `ir_run_values`, `ir_run_signals`, and `ir_effect_claims`

## Compatibility scheduler observability

The scheduler exposes:

- `/readyz`: database readiness
- `/metrics`: Prometheus metrics including queue and compatibility workflow advancement state from the PostgreSQL adapter
- structured logs for polling ticks, interval automation advancement, and compatibility workflow reconciliation

Operational checks:

```powershell
curl http://127.0.0.1:8082/readyz
curl http://127.0.0.1:8082/metrics
docker logs capsulet-scheduler
```

If compatibility workflow runs remain queued:

1. Confirm scheduler readiness.
2. Confirm worker readiness.
3. Check execution-pool concurrency metrics.
4. Inspect `workflow_runs` and `job_runs` for terminal blockers.
5. Check Keycloak/API auth only if the dashboard cannot issue commands; scheduler/worker use the database directly.

## Horizontal worker scaling

Workers are horizontally scalable when they share the same database and object store. Leasing uses guarded transitions and pool-level concurrency checks, so multiple workers can poll concurrently.

Recommended settings:

- unique `CAPSULET_WORKER_ID` per replica (Helm derives this from the pod name)
- `CAPSULET_WORKER_LOOP=true`
- `CAPSULET_WORKER_POLL_SECONDS=1-5`
- pool concurrency sized to Kubernetes cluster capacity
- Kubernetes API rate limits monitored through worker logs and metrics

## Backpressure

Backpressure is enforced at two layers:

- API admission can reject new manual job and manual compatibility workflow-trigger submissions before they are persisted.
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

The agent runtime persists state and trace records after each node adapter result. The worker reattaches to deterministic Kubernetes Jobs after lease expiry. The compatibility scheduler reconciles workflow DAGs from durable run/step state and can resume failed workflow runs from completed checkpoints.

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
- Keycloak login and temporary admin login both exercised successfully
- threat model reviewed against deployment settings
