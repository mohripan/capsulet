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
- `capsulet-api`: future HTTP control plane
- `capsulet-worker`: job leasing and runner coordination
- `capsulet-scheduler`: future schedule and delay scanner
- `capsulet-evaluator`: future automation condition evaluator
- `capsulet-runner`: execution backend boundary and stub runners
- `capsulet-cli`: operator and developer CLI for the HTTP API

## Local PostgreSQL

Sprint 002 uses PostgreSQL as the durable metadata store. Start the local database from the repository root:

```sh
docker compose up -d postgres
```

The development database URL is:

```sh
postgres://capsulet:capsulet@localhost:5432/capsulet
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
cargo run -p capsulet-api
```

Available Sprint 002 endpoints:

- `GET /healthz`
- `POST /v1/jobs/runs`
- `GET /v1/jobs/runs`
- `GET /v1/jobs/runs/{id}`

See `docs/api.md` for request examples.

## CLI

The CLI talks to the HTTP API. Start the API first, then run:

```powershell
$env:CAPSULET_API_URL = "http://127.0.0.1:8080"
cargo run -p capsulet-cli -- submit job_hello_python --pool mini
cargo run -p capsulet-cli -- runs
cargo run -p capsulet-cli -- run get run_123
```

You can also pass the API URL per command:

```sh
cargo run -p capsulet-cli -- --api-url http://127.0.0.1:8080 runs --limit 25
```

## Worker

The worker currently executes one queued run per process invocation through a stub runner.

Run a success tick:

```powershell
$env:CAPSULET_DATABASE_URL = "postgres://capsulet:capsulet@localhost:5432/capsulet"
$env:CAPSULET_WORKER_ID = "worker-local"
$env:CAPSULET_STUB_RUNNER_RESULT = "success"
cargo run -p capsulet-worker
```

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
npx tsc --noEmit
```

The dashboard is currently a mock frontend. See `dashboard/README.md` for routes and the current build caveat.

## Helm Chart

Run from the repository root:

```sh
helm lint charts/capsulet
helm template capsulet charts/capsulet
```

The chart is a skeleton for the future installable product. It renders the API, worker, scheduler, evaluator, dashboard, RBAC, service account, config, services, execution pool config, and a test pod.

## Architecture Rules

- Keep domain logic in `capsulet-core`.
- Do not add database, Kubernetes, Kafka, or HTTP framework dependencies to `capsulet-core`.
- Put infrastructure adapters such as PostgreSQL in separate crates that implement `capsulet-core` ports.
- Service crates should stay thin until runtime behavior exists.
- Prefer ADRs for decisions that change architecture or operational behavior.
- Keep Helm as a first-class product distribution surface.

## Current Sprint

Sprint planning lives in `planning/`.

- Sprint plan: `planning/sprints/sprint-001-foundation.md`
- Sprint backlog: `planning/backlog/sprint-002-backlog.md`
