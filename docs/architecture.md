# Architecture

Capsulet is a Kubernetes-native automation platform for authoring reusable Python jobs, composing them into simple workflows, triggering them manually or on an interval, and inspecting durable execution state, logs, and artifacts.

## Components

- API: accepts job definition, workflow, automation, and run submissions; exposes run status, logs, cancellation, and artifact download endpoints.
- Worker: leases queued runs, recovers expired leases, executes one or more worker ticks, and persists final run state.
- Scheduler: creates due interval automation runs and advances workflow runs from one step to the next as underlying job runs finish.
- Runner: owns execution backend behavior. The current production-shaped runner creates Kubernetes Jobs; the stub runner is used for local tests and smoke checks.
- PostgreSQL: stores control-plane metadata, including job definitions, workflows, automations, workflow runs, job runs, attempts, inline log previews, and artifact metadata.
- Object storage: stores script bundles, large logs, and artifact bytes through the shared `capsulet-storage` boundary.
- CLI: talks to the API for submit, status, logs, cancellation, and artifact commands.
- Dashboard: browser UI for job definition authoring, workflow authoring, automation triggers, live run listing, submission, run detail, cancellation, logs, and artifacts. Settings and security surfaces are still placeholders.

## Runtime Flow

1. A user creates a reusable job definition from the dashboard or API. For Python jobs, the API stores the script as `bundles/job-definitions/<job-definition-id>/main.py`.
2. A user can submit that job definition directly, or compose it into a linear workflow.
3. A user creates an automation for the workflow. Manual automations run when triggered; interval automations create runs when the scheduler observes that `next_fire_at` is due.
4. PostgreSQL stores the queued workflow run and its step-run state.
5. The scheduler starts the next workflow step by creating a normal queued job run for the step's job definition and execution pool.
6. The worker recovers expired leases, leases queued job runs, and records execution attempts.
7. If the run uses a script bundle, the worker reads the bundle from object storage and rewrites the command to execute the script content.
8. The runner executes the job. In Kubernetes mode, it creates a Kubernetes Job using the selected execution pool resources, node selector, tolerations, timeout, and cleanup TTL.
9. The worker stores a bounded inline log preview. Logs larger than 64 KiB are also uploaded to object storage as `logs/<run-id>/stdout.log`.
10. Files published under `/capsulet/artifacts` are uploaded to object storage as run artifacts.
11. The worker marks the job run `succeeded`, `failed`, `timed_out`, `cancelled`, or `retry_scheduled`.
12. The scheduler advances or finishes the workflow run based on the terminal state of each step's job run.

## Dashboard Flow

The dashboard uses a same-origin Next.js proxy route at `/api/capsulet/...`. Browser code calls that route, and the proxy forwards requests to the upstream Capsulet API configured by `CAPSULET_DASHBOARD_API_URL`.

This keeps local browser use simple and avoids CORS requirements for the current unauthenticated alpha dashboard.

## Storage Boundaries

PostgreSQL is the source of truth for metadata and state transitions. It does not store script bundle bytes, large log bytes, or artifact bytes.

Object storage keys are scoped by their owning resource:

- `bundles/job-definitions/<job-definition-id>/main.py`
- `bundles/<run-id>/main.py`
- `logs/<run-id>/stdout.log`
- `artifacts/<run-id>/<name>`

Artifact metadata in PostgreSQL includes the run ID, optional attempt ID, artifact ID, display name, object key, content type, size, checksum when available, kind, and creation time. The API always resolves artifacts by run ID plus artifact ID so one run cannot fetch another run's artifacts through the normal endpoint.

## Execution Pools

Execution pools are static configuration in the current implementation. The API exposes configured pool names to the dashboard, Capsulet chooses the execution pool for a run, and Kubernetes chooses the specific node inside that pool.

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
- Workflow authoring is intentionally linear: steps execute in position order, with no branches or condition tree yet.
- Automation triggers are manual and fixed interval only.
- Dashboard authoring covers create/list flows; edit/delete controls remain limited.
- No retention cleanup for object storage.
- No multi-file script bundles.
- No streaming logs.
- No Kubernetes Job reattachment after worker crash; expired running rows can be recovered and retried by the worker.
- No bundled PostgreSQL or MinIO chart dependency yet; local dependencies are started with Docker Compose or provided externally to the chart.
