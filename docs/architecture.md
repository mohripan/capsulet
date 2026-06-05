# Architecture

Capsulet is a Kubernetes-native job queue for running Python scripts and command jobs with durable metadata, controlled execution, logs, and artifacts.

## Components

- API: accepts run submissions, exposes run status, logs, cancellation, and artifact download endpoints.
- Worker: leases queued runs, recovers expired leases, executes one or more worker ticks, and persists final run state.
- Runner: owns execution backend behavior. The current production-shaped runner creates Kubernetes Jobs; the stub runner is used for local tests and smoke checks.
- PostgreSQL: stores control-plane metadata, including job definitions, runs, attempts, inline log previews, and artifact metadata.
- Object storage: stores script bundles, large logs, and artifact bytes through the shared `capsulet-storage` boundary.
- CLI: talks to the API for submit, status, logs, cancellation, and artifact commands.
- Dashboard: browser UI for live run listing, submission, run detail, cancellation, logs, and artifacts. Prototype pages still exist for future automation, workflow, settings, and security surfaces.

## Runtime Flow

1. A user submits a run through the API or CLI.
2. The API validates the requested execution pool and job definition.
3. For `submit-script`, the API stores the Python script as `bundles/<run-id>/main.py` and creates a run-scoped job definition.
4. PostgreSQL stores the queued run and, when relevant, bundle artifact metadata.
5. The worker recovers expired leases, leases a queued run, and records an execution attempt.
6. If the run uses a script bundle, the worker reads the bundle from object storage and rewrites the command to execute the script content.
7. The runner executes the job. In Kubernetes mode, it creates a Kubernetes Job using the selected execution pool resources, node selector, tolerations, timeout, and cleanup TTL.
8. The worker stores a bounded inline log preview. Logs larger than 64 KiB are also uploaded to object storage as `logs/<run-id>/stdout.log`.
9. Files published under `/capsulet/artifacts` are uploaded to object storage as run artifacts.
10. The worker marks the run `succeeded`, `failed`, `timed_out`, `cancelled`, or `retry_scheduled`.

## Dashboard Flow

The dashboard uses a same-origin Next.js proxy route at `/api/capsulet/...`. Browser code calls that route, and the proxy forwards requests to the upstream Capsulet API configured by `CAPSULET_DASHBOARD_API_URL`.

This keeps local browser use simple and avoids CORS requirements for the current unauthenticated alpha dashboard.

## Storage Boundaries

PostgreSQL is the source of truth for metadata and state transitions. It does not store script bundle bytes, large log bytes, or artifact bytes.

Object storage keys are run-scoped:

- `bundles/<run-id>/main.py`
- `logs/<run-id>/stdout.log`
- `artifacts/<run-id>/<name>`

Artifact metadata in PostgreSQL includes the run ID, optional attempt ID, artifact ID, display name, object key, content type, size, checksum when available, kind, and creation time. The API always resolves artifacts by run ID plus artifact ID so one run cannot fetch another run's artifacts through the normal endpoint.

## Execution Pools

Execution pools are static Helm configuration in the current implementation. Capsulet chooses the execution pool for a run, and Kubernetes chooses the specific node inside that pool.

Each pool can define:

- description
- node selector
- tolerations
- resource requests and limits
- timeout seconds
- maximum concurrent jobs
- Kubernetes Job TTL after finish

Pool-level concurrency limits are represented in values today, but enforcement remains future work.

## Current Limitations

- No API authentication or user model.
- Dashboard integration is limited to runs, run detail, submission, cancellation, logs, and artifacts.
- No retention cleanup for object storage.
- No multi-file script bundles.
- No streaming logs.
- No Kubernetes Job reattachment after worker crash; expired running rows can be recovered and retried by the worker.
- No bundled PostgreSQL or MinIO chart dependency yet; local dependencies are started with Docker Compose or provided externally to the chart.
