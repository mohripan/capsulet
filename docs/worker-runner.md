# Worker and Runner

Sprint 002 includes the first worker execution path.

The worker leases one queued run, records an execution attempt by moving the run to `running`, executes through a runner boundary, and then stores a terminal state.

Current runner implementation:

- `StubRunner::success()`: always marks the run as `succeeded`
- `StubRunner::failure()`: always marks the run as `failed`

Kubernetes Job execution is intentionally out of scope for Sprint 002.

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

## Lease Behavior

PostgreSQL leases the oldest queued run using `FOR UPDATE SKIP LOCKED`. This prevents two workers from receiving the same queued run concurrently.

The current lease metadata is:

- `lease_owner`
- `lease_expires_at`
- `status = leased`

Lease expiry recovery is planned for a later worker hardening slice.
