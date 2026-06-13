# Development

This guide describes the current development workflow for Capsulet.

## Required Tools

- Rust 1.87 or newer
- Cargo
- Node.js 20.x
- npm 10.x
- Helm 3.18 or newer
- Docker

Optional later tools:

- kind or minikube
- kubectl

## Backend

Run from the repository root:

```sh
cargo metadata --no-deps --format-version 1
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The backend workspace lives in `crates/`.

Current crates:

- `capsulet-core`: domain model, command/query shapes, and infrastructure ports
- `capsulet-postgres`: PostgreSQL persistence adapter and embedded migrations
- `capsulet-storage`: filesystem and S3-compatible object storage adapter
- `capsulet-api`: HTTP control plane
- `capsulet-worker`: job leasing and runner coordination
- `capsulet-scheduler`: future schedule and delay scanner
- `capsulet-evaluator`: future automation condition evaluator
- `capsulet-runner`: execution backend boundary with stub and Kubernetes Job runners
- `capsulet-cli`: operator and developer CLI for the HTTP API

## Local Docker Compose Stack

Capsulet uses PostgreSQL as the durable metadata store and object storage for script bundles, large logs, and artifacts. For a full local product check, start the Compose stack from the repository root:

```sh
docker compose up --build
```

This starts:

- PostgreSQL on `localhost:5432`
- MinIO on `localhost:9000`
- MinIO console on `localhost:9001`
- Capsulet API on `localhost:8080`
- Capsulet dashboard on `localhost:3000`
- Capsulet worker in continuous stub-runner mode

Open the live dashboard:

```text
http://127.0.0.1:3000/runs
```

The Compose worker uses `CAPSULET_RUNNER_MODE=stub` so the full API/dashboard/job/artifact flow can be checked without a Kubernetes cluster. Use the Helm/minikube path for Kubernetes Job execution.

The Compose database URL is:

```sh
postgres://capsulet:capsulet@localhost:5432/capsulet
```

The Compose MinIO credentials are:

```text
endpoint: http://127.0.0.1:9000
bucket: capsulet-artifacts
access key: capsulet
secret key: capsuletpassword
```

The `minio-init` Compose service creates the `capsulet-artifacts` bucket automatically.

To start only the data services for manual local backend work:

```sh
docker compose up -d postgres minio minio-init
```

The persistence crate embeds migrations from `migrations/` with SQLx. To run the PostgreSQL-backed tests against the local database:

```sh
CAPSULET_TEST_DATABASE_URL=postgres://capsulet:capsulet@localhost:5432/capsulet \
  cargo test -p capsulet-postgres
```

On PowerShell:

```powershell
$env:CAPSULET_TEST_DATABASE_URL = "postgres://capsulet:capsulet@localhost:5432/capsulet"
cargo test -p capsulet-postgres
```

Stop the local database when you are done:

```sh
docker compose down
```

Use timestamped SQL migration files in `migrations/`:

```text
migrations/YYYYMMDDHHMMSS_description.sql
```

Migrations should be append-only after they are shared. Add a new migration instead of editing an existing migration that another developer may already have applied.

## API

The API uses Axum and connects to PostgreSQL through `capsulet-postgres`.

Run locally:

```powershell
$env:CAPSULET_DATABASE_URL = "postgres://capsulet:capsulet@localhost:5432/capsulet"
$env:CAPSULET_API_ADDR = "127.0.0.1:8080"
$env:CAPSULET_EXECUTION_POOLS = "mini,large"
$env:CAPSULET_SEED_EXAMPLES = "true"
$env:CAPSULET_OBJECT_STORAGE_MODE = "filesystem"
$env:CAPSULET_OBJECT_STORAGE_PATH = ".capsulet-objects"
cargo run -p capsulet-api
```

Available job-run endpoints:

- `GET /healthz`
- `POST /v1/jobs/runs`
- `GET /v1/jobs/runs`
- `GET /v1/jobs/runs/{id}`
- `GET /v1/jobs/runs/{id}/logs`
- `POST /v1/jobs/runs/{id}/cancel`
- `GET /v1/jobs/runs/{id}/artifacts`
- `GET /v1/jobs/runs/{id}/artifacts/{artifact_id}`

See `docs/api.md` for request examples.

## CLI

The CLI talks to the HTTP API. Start the API first, then run:

```powershell
$env:CAPSULET_API_URL = "http://127.0.0.1:8080"
cargo run -p capsulet-cli -- submit job_hello_python --pool mini
cargo run -p capsulet-cli -- runs
cargo run -p capsulet-cli -- run get run_123
cargo run -p capsulet-cli -- status run_123
cargo run -p capsulet-cli -- logs run_123
cargo run -p capsulet-cli -- artifacts list run_123
```

You can also pass the API URL per command:

```sh
cargo run -p capsulet-cli -- --api-url http://127.0.0.1:8080 runs --limit 25
```

## Worker

The worker can execute one queued run per process invocation through a stub runner, or run continuously through the Kubernetes Job runner.

Run a success tick:

```powershell
$env:CAPSULET_DATABASE_URL = "postgres://capsulet:capsulet@localhost:5432/capsulet"
$env:CAPSULET_WORKER_ID = "worker-local"
$env:CAPSULET_STUB_RUNNER_RESULT = "success"
cargo run -p capsulet-worker
```

Run one Kubernetes-backed worker tick against the current kube context:

```powershell
$env:CAPSULET_RUNNER_MODE = "kubernetes"
$env:CAPSULET_EXECUTION_NAMESPACE = "capsulet"
cargo run -p capsulet-worker
```

For the Helm/minikube path, see `docs/local-kubernetes-runner.md`.

Run a failure tick:

```powershell
$env:CAPSULET_STUB_RUNNER_RESULT = "failure"
cargo run -p capsulet-worker
```

See `docs/worker-runner.md` for the manual flow.

## Dashboard

Run from `dashboard/`:

```sh
npm install
npm run dev
npm test
npx tsc --noEmit
npm run build
```

Point the dashboard at a local API:

```powershell
$env:CAPSULET_DASHBOARD_API_URL = "http://127.0.0.1:8080"
npm run dev
```

The `/runs` and `/runs/{id}` pages use the live API. Other product-shaped pages still use mock data. See `dashboard/README.md`.

## Helm Chart

Run from the repository root:

```sh
helm lint charts/capsulet
helm template capsulet charts/capsulet
```

The chart can install the API and worker runtime path when local images, a PostgreSQL secret, and object storage settings are provided. See `docs/local-kubernetes-runner.md`.

## Architecture Rules

- Keep domain logic in `capsulet-core`.
- Do not add database, Kubernetes, Kafka, or HTTP framework dependencies to `capsulet-core`.
- Put infrastructure adapters such as PostgreSQL in separate crates that implement `capsulet-core` ports.
- Service crates should stay thin until runtime behavior exists.
- Prefer ADRs for decisions that change architecture or operational behavior.
- Keep Helm as a first-class product distribution surface.

## Current Sprint

Sprint planning lives in `planning/`.

- Current sprint plan: `planning/sprints/sprint-010-mvp-hardening-and-alpha-readiness.md`
- Current sprint backlog: `planning/backlog/sprint-010-backlog.md`
