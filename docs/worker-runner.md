# Worker and Runner

The worker leases queued runs, executes them through a runner boundary, persists final state, and publishes bundles, large logs, and artifacts through object storage.

The worker promotes ready retries, recovers expired leases, leases one queued run, records an execution attempt by moving the run to `running`, executes through a runner boundary, and then stores a guarded final state.

Current runner implementations:

- `StubRunner::success()`: always marks the run as `succeeded`
- `StubRunner::failure()`: always marks the run as `failed`
- `ProcessRunner`: executes the configured command as a trusted local child process for development
- `KubernetesRunner`: creates a Kubernetes Job, waits for terminal status, supports cancellation, classifies timeouts, captures pod logs, and collects files published under `/capsulet/artifacts`

The runner boundary returns a `RunReport` with an outcome, logs, and collected artifacts. Outcomes are `succeeded`, `failed`, `timed_out`, or `cancelled`. Small logs are stored inline for the existing logs API. Logs larger than 64 KiB are also uploaded to object storage as `stdout.log`, with PostgreSQL storing the artifact metadata and object key.

Single-file Python script submissions are stored as bundle objects. Before execution, the worker reads the bundle and rewrites the run command to execute the script content.

## Run Locally

Start PostgreSQL:

```sh
docker compose up -d postgres
```

Start the API with example seeding:

```powershell
$env:CAPSULET_DATABASE_URL = "postgres://capsulet:capsulet@localhost:5432/capsulet"
$env:CAPSULET_API_ADDR = "127.0.0.1:8080"
$env:CAPSULET_EXECUTION_POOLS = "mini,large"
$env:CAPSULET_SEED_EXAMPLES = "true"
cargo run -p capsulet-api
```

Create a run:

```sh
curl -X POST http://127.0.0.1:8080/v1/jobs/runs \
  -H "content-type: application/json" \
  -d '{"job_definition_id":"job_hello_python","execution_pool":"mini"}'
```

Run one worker tick:

```powershell
$env:CAPSULET_DATABASE_URL = "postgres://capsulet:capsulet@localhost:5432/capsulet"
$env:CAPSULET_WORKER_ID = "worker-local"
$env:CAPSULET_STUB_RUNNER_RESULT = "success"
$env:CAPSULET_OBJECT_STORAGE_MODE = "filesystem"
$env:CAPSULET_OBJECT_STORAGE_PATH = ".capsulet-objects"
cargo run -p capsulet-worker
```

Fetch the run through the API. It should be `succeeded`.

For a failure path, set:

```powershell
$env:CAPSULET_STUB_RUNNER_RESULT = "failure"
```

## Kubernetes Runner

Run one Kubernetes-backed tick against your current kube context:

```powershell
$env:CAPSULET_DATABASE_URL = "postgres://capsulet:capsulet@localhost:5432/capsulet"
$env:CAPSULET_WORKER_ID = "worker-local"
$env:CAPSULET_RUNNER_MODE = "kubernetes"
$env:CAPSULET_EXECUTION_NAMESPACE = "capsulet"
cargo run -p capsulet-worker
```

For the Helm-installed minikube flow, see `docs/local-kubernetes-runner.md`.

The worker reads execution pools from one of these sources:

- `CAPSULET_EXECUTION_POOLS_YAML`
- `CAPSULET_EXECUTION_POOLS_FILE`
- built-in `mini` and `large` defaults

The Kubernetes runner applies the selected pool's resources, node selector, tolerations, timeout, and optional `ttlSecondsAfterFinished` cleanup setting to the created Job.

For artifact-producing jobs, write files to:

```text
/capsulet/artifacts
```

The Kubernetes runner wraps the job command, preserves the original exit status, and emits completed artifact files back to the worker for upload. Artifact names are normalized to their base file names; nested paths are not preserved.

## Object Storage

Filesystem storage is the local default:

```powershell
$env:CAPSULET_OBJECT_STORAGE_MODE = "filesystem"
$env:CAPSULET_OBJECT_STORAGE_PATH = ".capsulet-objects"
```

For MinIO or another S3-compatible endpoint:

```powershell
$env:CAPSULET_OBJECT_STORAGE_MODE = "s3"
$env:CAPSULET_OBJECT_STORAGE_BUCKET = "capsulet-artifacts"
$env:CAPSULET_OBJECT_STORAGE_ENDPOINT = "http://127.0.0.1:9000"
$env:CAPSULET_OBJECT_STORAGE_REGION = "us-east-1"
$env:CAPSULET_OBJECT_STORAGE_PATH_STYLE = "true"
$env:CAPSULET_OBJECT_STORAGE_ACCESS_KEY_ID = "capsulet"
$env:CAPSULET_OBJECT_STORAGE_SECRET_ACCESS_KEY = "capsuletpassword"
```

The API and worker must point at the same storage backend. Filesystem mode is useful for single-process local smoke tests; S3 mode is the expected multi-process and Kubernetes configuration.

Troubleshooting checks:

- `CAPSULET_OBJECT_STORAGE_BUCKET` must exist before the API or worker tries to write S3 objects.
- `CAPSULET_OBJECT_STORAGE_ENDPOINT` must be reachable from the API and worker process or pod. In minikube, use `host.minikube.internal` for host-local MinIO.
- Set `CAPSULET_OBJECT_STORAGE_PATH_STYLE=true` for MinIO.
- S3 credentials are read from `CAPSULET_OBJECT_STORAGE_ACCESS_KEY_ID` and `CAPSULET_OBJECT_STORAGE_SECRET_ACCESS_KEY`, or from the Helm credentials secret.
- If artifact list succeeds but download fails, the metadata row exists but the referenced object key could not be read from storage.

The evaluator removes expired terminal-run object bytes before deleting artifact metadata and inline logs. Cleanup is idempotent and configurable independently for run data, trigger events, and audit events. Multi-file source bundles and streaming object-backed logs remain future work; the dashboard lists and downloads completed artifacts.

## Cancellation, Timeout, and Retry

`POST /v1/jobs/runs/{id}/cancel` and `capsulet cancel <run-id>` mark non-terminal runs as `cancelled`.

For running Kubernetes Jobs, the runner checks cancellation while waiting for completion. When cancellation is observed, it deletes the run-derived Kubernetes Job and records the run as `cancelled`. Guarded persistence prevents an older worker result from overwriting a cancellation.

Timeouts are recorded as `timed_out` when the Kubernetes Job reports `DeadlineExceeded` or the runner wait deadline expires. Non-zero script exits remain `failed`.

Job definitions carry a minimal fixed-delay retry policy:

- `retry_max_attempts`
- `retry_delay_seconds`

Failed or timed-out runs move to `retry_scheduled` when attempts remain, then the worker promotes ready retries back to `queued`. Cancelled runs are terminal and are never retried.

## Lease Behavior

PostgreSQL leases the oldest queued run using `FOR UPDATE SKIP LOCKED`. This prevents two workers from receiving the same queued run concurrently.

The current lease metadata is:

- `lease_owner`
- `lease_expires_at`
- `heartbeat_at`
- `status = leased`

While a runner is active, the worker refreshes `heartbeat_at` and extends `lease_expires_at`. The heartbeat interval is derived from the configured lease duration and must be shorter than the lease.

At the beginning of each tick, the worker requeues expired `leased` runs. For an expired running Kubernetes attempt, a replacement worker atomically adopts the lease and reattaches to the deterministic run/attempt Job after validating its ownership labels. No new attempt or replacement Job is created. Non-reattachable runners requeue the run and retain at-least-once behavior.

## Health Endpoints

The worker starts a health listener at `CAPSULET_WORKER_HEALTH_ADDR` (default `0.0.0.0:8081`):

- `/livez` returns success while the health process is serving.
- `/readyz` and `/healthz` return success only when PostgreSQL responds to a ping.
