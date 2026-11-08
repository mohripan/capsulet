<!-- capsulet-claims: CAP-AGENT-001, CAP-AGENT-002, CAP-AGENT-008, CAP-MEMORY-001, CAP-WORKFLOW-001, CAP-AUTOMATION-001, CAP-PERSISTENCE-001 -->
# Persistence

Capsulet uses PostgreSQL as its durable agent execution-graph control-plane store and work queue. Filesystem or S3-compatible object storage holds script, full-log, and artifact bytes for deterministic tool/job execution. The later memory graph should add separate claim/entity/event/evidence tables rather than overloading execution graph tables.

## Schema ownership

Append-only SQLx migrations under `migrations/` define:

- `job_definitions`, `job_runs`, `job_attempts`, `job_run_logs`, and `job_artifacts`;
- `graph_definitions`, `graph_nodes`, `graph_ports`, `graph_hyperedges`, `graph_hyperedge_endpoints`, and `graph_transition_actions`;
- `agent_definitions`, `agent_termination_conditions`, `agent_runs`, `agent_state_snapshots`, and `agent_trace_events`;
- compatibility `workflow_definitions`, `workflow_steps`, `workflow_step_dependencies`, `workflow_runs`, and `workflow_step_runs`;
- `automations`, `automation_triggers`, `custom_trigger_plugins`, durable trigger events/evaluations, audit events, and retention-cleanup markers.

An agent run links to the IR run executing it. Its status, state version, usage and final state are read from that run's append-only log and from `ir_run_values`, which holds the bytes behind the log's digests, scoped to a project. The `status` and `state_json` columns on `agent_runs` hold what the run was created with. `agent_state_snapshots` and `agent_trace_events` hold history from before agent runs executed. A run's payloads can be erased while its record stays; see [Erasing a run's payloads](#erasing-a-runs-payloads). An IR run also keeps the W3C `traceparent` of the request that started it in `ir_runs.trace_parent`, so the leases that advance it join that request's trace; nothing reads it except telemetry, and it is empty when tracing was off.

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

The graph worker writes a run's history as events under a fencing epoch, so a worker that lost its lease cannot append to a run it no longer owns. The compatibility scheduler also uses PostgreSQL transactions to create due interval workflow runs and reconcile DAG nodes. Dependency edges are persisted separately from step position.

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

The URL names a server, not the database the tests write to. Each database-backed test binary
creates a database of its own on that server, `test_<package>_<binary>` (for example
`test_capsulet_graph_worker_agent_runs`), dropping and recreating it the first time the process asks.
No two binaries share tables, so fixture ids are stable (`tenant_1`, `run_2`), and what a failing run
left behind stays there to inspect until that binary runs again. The role in the URL must be allowed
to create databases; the Compose `capsulet` role and the verifier's disposable server both are.
Without the variable, a database-backed test fails and says how to run it rather than passing.

## Erasing a run's payloads

A run's log holds digests, never the bytes behind them. The bytes are in `ir_run_values`, stored once per
project for each distinct content, and `ir_run_value_refs` records which runs hold each one: a run holds
every value it stores, as it stores it. The starting state also has copies on the `agent_runs` row and in
`agent_state_snapshots`. Erasing a finished agent run (`POST /v1/agent-runs/{id}/erase`, or the retention
schedule below) separates the content from the record:

| Erased | Kept |
| --- | --- |
| The run's hold on every value it stored: its inputs, its outputs, and what those name (an embedding's vector, a retrieved claim's text) | The append-only log (`ir_run_events`): every event, every digest, every decision |
| The bytes of each of those values that no other run holds | Bytes another run still holds, until that run is erased too |
| The starting state on `agent_runs` and in `agent_state_snapshots` | The definition version, the admission record, and any certificate |
| | A tombstone for each erased value: its digest, `erased_at`, and `erased_by` |

The same bytes are often held by many runs at once: an empty string, a common validator result, the same
question asked twice. So erasing one run never takes anything from another. A value another run holds
stays for it, the erasure reports it as kept, and its bytes go when the last run holding it is erased.
Erasure and storage take a per-project lock, so a run storing the same bytes at the moment they are
erased either holds them before the erasure decides, or stores them again after it.

Decisions replay from the log alone, so `fold` and `decide` give the same answers after an erasure as
before it. What no longer works is anything that needs the bytes: reading the run's state, re-running a
node, or re-checking a value against its schema. A node that reads an erased value fails with
`missing_evidence`, saying the value was erased on request rather than that it is missing. Producing the
same bytes again later stores them again; an erasure is about what was held, not a ban on a string.

The log carries no content to erase. A schema check records where a value failed and why, never the value
itself (the API still shows a caller what it sent), and a model refusal records the status code, the
provider and the model, not the provider's error body. Failure details name kinds, nodes, ports and rules,
which come from the definition.

Capsulet does not encrypt stored values itself. Encryption at rest is the database's and its volume's;
use an encrypted volume or a managed PostgreSQL with storage encryption. Raw values leave Capsulet only
through the API that reads a run's state, which is project-scoped, and never through telemetry or a debug
bundle.

### Retention

`CAPSULET_AGENT_RUN_PAYLOAD_RETENTION_DAYS` on the evaluator (`evaluator.agentRunPayloadRetentionDays`
in Helm) erases the payloads of every agent run whose terminal event was recorded more than that many days
ago, on the evaluator's retention interval and at most 1000 runs per sweep. It is unset by default, so an
upgrade never starts erasing on its own. Tombstones it leaves say `capsulet-retention`.
