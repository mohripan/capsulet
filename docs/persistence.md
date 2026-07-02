# Persistence

Capsulet uses PostgreSQL as its durable agent execution-graph control-plane store and work queue. Filesystem or S3-compatible object storage holds script, full-log, and artifact bytes for deterministic tool/job execution. The later memory graph should add separate claim/entity/event/evidence tables rather than overloading execution graph tables.

## Schema ownership

Append-only SQLx migrations under `migrations/` define:

- `job_definitions`, `job_runs`, `job_attempts`, `job_run_logs`, and `job_artifacts`;
- `graph_definitions`, `graph_nodes`, `graph_ports`, `graph_hyperedges`, `graph_hyperedge_endpoints`, and `graph_transition_actions`;
- `agent_definitions`, `agent_termination_conditions`, `agent_runs`, `agent_state_snapshots`, and `agent_trace_events`;
- compatibility `workflow_definitions`, `workflow_steps`, `workflow_step_dependencies`, `workflow_runs`, and `workflow_step_runs`;
- `automations`, `automation_triggers`, `custom_trigger_plugins`, durable trigger events/evaluations, audit events, and retention-cleanup markers.

Agent runs carry current status, state version, and current state JSON. Every saved state version is also stored in `agent_state_snapshots`, and runtime decisions are appended to `agent_trace_events` with run-local sequence numbers.

Job run rows carry cancellation/retry fields, lease owner/expiry, and heartbeat timestamps. Workflow-run removal is represented explicitly so operational history can be hidden without silently deleting unrelated definitions.

Use timestamped names:

```text
migrations/YYYYMMDDHHMMSS_description.sql
```

After a migration is shared, do not edit it. Add a later migration.

## Adapter boundary

`capsulet-core` owns typed IDs, execution graph/agent aggregates, compatibility workflow state, transitions, graph validation, and repository/object-storage ports. `capsulet-application` owns the agent runtime use case and persistence ports for state and trace emission. `capsulet-postgres` implements persistence with SQLx and translates rows into validated domain values. Service crates use `PostgresStore` for operations that need transactions, leases, or multi-aggregate reconciliation.

The API, scheduler, and worker run embedded migrations on startup. The Helm chart also provides a migration Job through `CAPSULET_MIGRATE_ONLY=true`; startup migration remains idempotent.

## Durable queue and recovery

Job runs use PostgreSQL as a queue. A worker leases the oldest `queued` row using `FOR UPDATE SKIP LOCKED`, records `lease_owner` and `lease_expires_at`, and creates an attempt when execution starts. While the runner is active, the worker refreshes `heartbeat_at` and extends the lease.

Before leasing new work, workers:

- promote due `retry_scheduled` rows to `queued`;
- recover expired `leased` or `running` rows to `queued`.

Final updates are guarded by status and lease ownership. This prevents stale completions from overwriting cancellation or a replacement attempt. Kubernetes runs use deterministic run/attempt Job names: a replacement worker adopts the expired running lease and reattaches to the existing Job without creating another attempt.

The agent runtime writes current run state and trace events through repository ports so it can later be driven by a dedicated worker without changing persistence. The compatibility scheduler also uses PostgreSQL transactions to create due interval workflow runs and reconcile DAG nodes. Dependency edges are persisted separately from step position.

## Object boundary

PostgreSQL stores object keys and metadata; it does not store script or artifact bytes.

- `bundles/job-definitions/<job-definition-id>/main.py`
- `bundles/<run-id>/main.py`
- `logs/<run-id>/stdout.log`
- `artifacts/<run-id>/<name>`

Logs up to 64 KiB remain inline. Larger logs retain an inline preview and create an object-backed `stdout.log` artifact record. Artifact metadata includes ownership, display name, object key, content type, size, checksum when available, kind, and creation time.

## Local database

```sh
docker compose up -d postgres
```

Default URL:

```text
postgres://capsulet:capsulet@localhost:55432/capsulet
```

PowerShell test setup:

```powershell
$env:CAPSULET_TEST_DATABASE_URL = "postgres://capsulet:capsulet@localhost:55432/capsulet"
cargo test -p capsulet-postgres
```

Database-backed tests skip when `CAPSULET_TEST_DATABASE_URL` is absent.
