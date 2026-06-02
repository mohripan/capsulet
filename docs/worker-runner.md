# Worker and Runner

Sprint 004 includes the first controllable Kubernetes-backed worker execution path.

The worker promotes ready retries, recovers expired leases, leases one queued run, records an execution attempt by moving the run to `running`, executes through a runner boundary, and then stores a guarded final state.

Current runner implementation:

- `StubRunner::success()`: always marks the run as `succeeded`
- `StubRunner::failure()`: always marks the run as `failed`
- `KubernetesRunner`: creates a Kubernetes Job, waits for terminal status, supports cancellation, classifies timeouts, and captures bounded pod logs

The runner boundary returns a `RunReport` with an outcome and optional bounded logs. Outcomes are `succeeded`, `failed`, `timed_out`, or `cancelled`. Logs are stored through a generic log repository boundary. PostgreSQL is still only the bounded local implementation; object storage remains the preferred backend for larger logs and artifacts.

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
- `status = leased`

At the beginning of each tick, the worker requeues expired `leased` and `running` runs. This recovers rows left behind by worker crashes or rollouts. Reattaching to already-created running Kubernetes Jobs remains a later reconciliation task, so recovery may create a replacement Job after the lease expires.
