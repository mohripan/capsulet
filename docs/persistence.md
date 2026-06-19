# Persistence

Capsulet uses PostgreSQL as its durable control-plane store and work queue. Filesystem or S3-compatible object storage holds script, full-log, and artifact bytes.

## Schema ownership

Append-only SQLx migrations under `migrations/` define:

- `job_definitions`, `job_runs`, `job_attempts`, `job_run_logs`, and `job_artifacts`;
- `workflow_definitions`, `workflow_steps`, `workflow_step_dependencies`, `workflow_runs`, and `workflow_step_runs`;
- `automations`, `automation_triggers`, and `custom_trigger_plugins`.

Run rows also carry cancellation/retry fields, lease owner/expiry, and heartbeat timestamps. Workflow-run removal is represented explicitly so operational history can be hidden without silently deleting unrelated definitions.

Use timestamped names:

```text
migrations/YYYYMMDDHHMMSS_description.sql
```

After a migration is shared, do not edit it. Add a later migration.

## Adapter boundary

`capsulet-core` owns typed IDs, aggregates, transitions, graph validation, and repository/object-storage ports. `capsulet-postgres` implements persistence with SQLx and translates rows into validated domain values. Service crates use `PostgresStore` for operations that need transactions, leases, or multi-aggregate reconciliation.

The API, scheduler, and worker run embedded migrations on startup. The Helm chart also provides a migration Job through `CAPSULET_MIGRATE_ONLY=true`; startup migration remains idempotent.

## Durable queue and recovery

Job runs use PostgreSQL as a queue. A worker leases the oldest `queued` row using `FOR UPDATE SKIP LOCKED`, records `lease_owner` and `lease_expires_at`, and creates an attempt when execution starts. While the runner is active, the worker refreshes `heartbeat_at` and extends the lease.

Before leasing new work, workers:

- promote due `retry_scheduled` rows to `queued`;
- recover expired `leased` or `running` rows to `queued`.

Final updates are guarded by status and lease ownership. This prevents stale completions from overwriting cancellation or a replacement attempt. Recovery is at-least-once because Kubernetes Job reattachment is not implemented.

The scheduler also uses PostgreSQL transactions to create due interval workflow runs and reconcile DAG nodes. Dependency edges are persisted separately from step position.

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
postgres://capsulet:capsulet@localhost:5432/capsulet
```

PowerShell test setup:

```powershell
$env:CAPSULET_TEST_DATABASE_URL = "postgres://capsulet:capsulet@localhost:5432/capsulet"
cargo test -p capsulet-postgres
```

Database-backed tests skip when `CAPSULET_TEST_DATABASE_URL` is absent.
