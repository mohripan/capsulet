<!-- capsulet-claims: CAP-PRODUCT-001, CAP-JOB-001, CAP-JOB-002, CAP-JOB-003, CAP-JOB-004, CAP-JOB-005, CAP-JOB-006, CAP-JOB-007, CAP-JOB-008, CAP-JOB-009, CAP-WORKFLOW-001, CAP-IAM-001, CAP-IAM-002, CAP-IAM-003, CAP-IAM-004, CAP-IAM-005, CAP-IAM-006, CAP-IAM-007, CAP-IAM-009, CAP-PERSISTENCE-001, CAP-DASHBOARD-001, CAP-SDK-001, CAP-OBSERVABILITY-001, CAP-HELM-001 -->
# Development

This guide covers development of Capsulet's three layers: the implemented compatibility workflow
engine, the experimental agent platform (including governed memory), and the correctness plane.
Only the deterministic kernel slice of the correctness plane is implemented today; the unified IR
and runtime-wide enforcement remain planned.

## Required Tools

- Rust 1.96.0 (installed automatically by `rustup` from `rust-toolchain.toml`)
- Cargo
- Node.js 20 or newer, with npm 10 (24 for the `claims-evidence` gate)
- Python 3.12 or newer
- Docker 27 or newer, with Compose v2
- Helm 3.18 or newer
- Kind 0.30 or newer and kubectl, for the `kind` gate

Check the current machine without changing it:

```sh
cargo run -p capsulet-xtask --locked -- verify doctor
```

Run deterministic service-free verification with `verify --profile fast`, or the release-equivalent
suite with `verify --profile full`. The executable gate graph and supported-tool matrix are in
[`docs/contracts/verification-matrix.md`](contracts/verification-matrix.md).

If Windows reserves the default PostgreSQL host port, choose another before the stack starts.
Containers still reach each other on their own ports:

```powershell
$env:CAPSULET_POSTGRES_HOST_PORT = "56432"
cargo run -p capsulet-xtask -- stack up
```

Every host port has such a variable; `compose.yaml` lists them.

## Changing public contracts

Run the complete local/CI contract gate from the repository root. The PowerShell entry point is a
compatibility delegate to the Rust verifier:

```powershell
pwsh ./scripts/check-contracts.ps1
```

The direct cross-platform form is:

```sh
cargo run -p capsulet-xtask --locked -- verify --gate claims --gate api-contracts --gate sdk
```

Use the following workflow for contract changes:

1. Product claim: edit `docs/contracts/product-claims.json`, add the claim marker to every declared
   public surface, and attach executable test evidence for implemented capabilities/guarantees.
   Test evidence names the verifier gate that runs it and that gate's exact command, as
   `verify --list --format json` spells it (PowerShell as `pwsh`, a working directory as
   `cd <dir> && `). Then run `cargo run -p capsulet-xtask --locked -- claims collect`, which builds
   and lists each cited command's tests and rewrites `docs/contracts/test-manifest.json`; a test
   that is not collected, or is collected but ignored or skipped, is refused.
   Add an accepted ADR when the claim establishes a normative product or compatibility decision.
   Regenerate `docs/contracts/product-claims.md` with
   `pwsh ./scripts/render-product-claims.ps1 -OutputPath docs/contracts/product-claims.md`.
2. Endpoint or wire schema: declare the operation in `crates/api/src/endpoint_contract.rs`, naming
   the Rust type the handler reads, the one it writes and, with `parameters = Type`, the query or
   headers it reads, never a schema name or a parameter list. Register its handler in
   `crates/api/src/http/internal.rs` with `.operation("operationId", handler)`: the method, the path
   and whether it needs authentication come from the contract, and the router refuses to start
   with an operation unregistered. Derive `utoipa::ToSchema` on the body types and
   `utoipa::IntoParams` on the parameter type; `#[schema(as = Name)]` keeps a published component name
   that differs from the Rust one. The document collects each type's schema and every schema it
   refers to, and refuses two types that claim one name. Only shapes with no serde type in this
   crate, such as non-JSON bodies or the kernel's certificate, are written out, as types in
   `crates/api/src/wire_schemas.rs`, and each is checked against real payloads in
   `crates/api/src/openapi.rs`. In tests every response is checked against the contract of
   the route that produced it, failures included; a crate whose tests drive the router turns
   that on with `capsulet-api`'s `contract-conformance` feature as a dev-dependency, and
   `CAPSULET_CONFORMANCE_TRACE=<file>` records which operations and statuses a run reached.
   Preserve stable `operationId` values unless the versioning policy
   authorizes a break. Run `cargo run -p capsulet-api --bin export-openapi` and never hand-edit the
   generated artifact.
3. Status or verdict: update the Rust source enum first, then
   `docs/contracts/lifecycle-mapping.json`, transitions, owners, and target mappings. Execution
   status and assurance verdict remain separate vocabularies.
4. Policy or decision: update the normative file under `docs/contracts/`, index it from the contract
   README, and create/supersede an ADR where the decision policy requires one.
5. SDK/dashboard call: add the transport operation to `CLIENT_OPERATIONS` or
   `DASHBOARD_OPERATIONS`, keep ergonomic authoring separate from wire types, and run
   `pwsh ./scripts/check-sdk-contracts.ps1`.

The unified gate validates deterministic generated files, route/OpenAPI equality, schema and
security metadata, claim markers/evidence, lifecycle semantics, accepted ADR status, and client
operation maps. The pull-request template records migration, authorization, stability, SDK, and
documentation impact explicitly.

## Backend

Run from the repository root:

```sh
cargo run -p capsulet-xtask --locked -- verify --gate format --gate lint --gate unit
```

These are rustfmt, Clippy with every warning an error, and the workspace's tests;
`verify --list` shows each gate's exact command. The backend workspace lives in `crates/`, and every
crate inherits its version, license and repository from the workspace (see the
[release process](contracts/release-process.md)):

- `capsulet-core`: typed execution graph, agent, workflow compatibility, and execution domain model
- `capsulet-application`: application services, agent runtime use cases, and ports
- `capsulet-ir`, `capsulet-ir-adapters`, `capsulet-runtime`: the verified-computation IR, the
  translation into it, and its execution runtime
- `capsulet-kernel`: the deterministic correctness kernel
- `capsulet-proposer`: untrusted proposers, including a local Ollama adapter
- `capsulet-replay`: offline certificate replay
- `capsulet-postgres`: PostgreSQL persistence adapter and embedded migrations
- `capsulet-storage`: filesystem and S3-compatible object storage adapter
- `capsulet-api`: HTTP control plane for graphs, agents, runs, jobs, and compatibility workflows
- `capsulet-worker`: job leasing and runner coordination
- `capsulet-graph-worker`: durable execution of agent runs and IR runs
- `capsulet-scheduler`: interval automation polling and compatibility workflow-DAG reconciliation
- `capsulet-evaluator`: durable cron, SQL, webhook, and custom-trigger evaluation plus retention cleanup
- `capsulet-runner`: execution boundary with stub, trusted local-process, WASI Python and Kubernetes Job runners
- `capsulet-observability`: tracing and metrics
- `capsulet-cli`: operator and developer CLI for the HTTP API
- `capsulet-xtask`: the verifier, the stack fixture, and the release and supply-chain checks

## Local Docker Compose Stack

Capsulet uses PostgreSQL as the durable metadata store and object storage for script bundles, large logs, and artifacts. For a full local product check, start the Compose stack from the repository root:

```sh
cargo run -p capsulet-xtask -- stack up
```

This generates the stack's secrets, builds the images, waits for every health check, and seeds a
tenant with two projects and an identity per role. Nothing it uses is checked in; see
[test identities](development/test-identities.md). `cargo run -p capsulet-xtask -- stack down`
removes the stack, its volumes and its secrets. It starts:

- PostgreSQL on `localhost:55432`
- SeaweedFS, the S3 object store, on `localhost:8333`
- Keycloak on `localhost:18080` with realm `capsulet`
- Capsulet API on `localhost:8080`
- Capsulet dashboard on `localhost:3000`
- Capsulet worker, running each job's Python in its own container (process runner)
- Capsulet graph worker, which executes agent runs and IR runs
- Capsulet scheduler and evaluator in continuous polling mode
- Mailpit SMTP on `localhost:1025` and its web UI on `localhost:8025`, for the send-email example
- Prometheus on `localhost:9090` and Grafana on `localhost:3001`

Every image it builds is labelled with this checkout's release version and commit, and once it is
healthy the API and the dashboard must report the version `release.toml` names.

Open the live dashboard:

```text
http://127.0.0.1:3000/runs
```

Sign in to the dashboard as the temporary local admin, whose username and password are under
`dashboard_login` in `.capsulet/stack/capsulet-local/fixture.json`. Keycloak's realm user `admin`
also gets a password generated for the stack.

All `/v1` API calls require `Authorization: Bearer <token>` unless authentication is explicitly
disabled for an isolated test. Each identity's token is in the same file. The smoke scripts and
Playwright read them from it, so they never need one passed in.

The Compose worker uses `CAPSULET_RUNNER_MODE=process`, running each job's Python in the worker container, so the full API/dashboard/job/artifact flow can be checked without a Kubernetes cluster. Use the Helm/minikube path for Kubernetes Job execution.

The Compose database URL, with the password generated for the stack, is in
`.capsulet/stack/capsulet-local/database-url`.

Compose stores script bundles, large logs and artifacts in SeaweedFS, through the same S3 adapter a
deployment uses. SeaweedFS runs in its single-process `mini` mode, creates the `capsulet-artifacts`
bucket on start, admits only the stack's generated key, and is reachable from the host on
`127.0.0.1:8333`. It replaced MinIO, whose community images stopped being published on 2025-10-23
and whose repository is no longer maintained.

To start only the data services for manual local backend work:

```sh
cargo run -p capsulet-xtask -- stack up postgres
```

Its URL, with the password generated for it, is in `.capsulet/stack/capsulet-local/database-url`.

The persistence crate embeds migrations from `migrations/` with SQLx. To run the PostgreSQL-backed tests against the local database:

```sh
CAPSULET_TEST_DATABASE_URL=$(cat .capsulet/stack/capsulet-local/database-url) \
  cargo test -p capsulet-postgres
```

On PowerShell:

```powershell
$env:CAPSULET_TEST_DATABASE_URL = Get-Content .capsulet/stack/capsulet-local/database-url
cargo test -p capsulet-postgres
```

The URL names a server, not the database the tests write to. Each database-backed test creates a
database of its own on that server, named after its binary and the test with a hash on the end
(for example `t_agent_runs_a_run_started_over_http_is_executed_by_the_worker_…`). It drops and
recreates that database the first time the test asks for it. No two tests share tables, so a worker
in one test cannot lease a run another test queued, and fixture ids stay stable (`tenant_1`,
`run_2`). Whatever a failing test left behind stays there to inspect until that test runs again. The role in the URL must be allowed
to create databases; the Compose `capsulet` role and the verifier's disposable server both are.
Without the variable, a database-backed test fails and says how to run it rather than passing.
Run through the verifier, the `postgres`, `migrations` and `chaos` gates also fail when a test
binary ran no tests, or ignored or filtered out any. Each run records every binary's counts in
`.capsulet/verify/<run>/test-counts.json`, and in the job summary on CI.

Remove the stack, its volumes and its secrets when you are done:

```sh
cargo run -p capsulet-xtask -- stack down
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
$env:CAPSULET_DATABASE_URL = Get-Content .capsulet/stack/capsulet-local/database-url
$env:CAPSULET_API_ADDR = "127.0.0.1:8080"
$env:CAPSULET_EXECUTION_POOLS = "mini,large"
$env:CAPSULET_SEED_EXAMPLES = "true"
$env:CAPSULET_OBJECT_STORAGE_MODE = "filesystem"
$env:CAPSULET_OBJECT_STORAGE_PATH = ".capsulet-objects"
$env:CAPSULET_API_TOKENS = '[{"name":"local-admin","role":"admin","token":"replace-me-replace-me-replace-me-1234"}]'
cargo run -p capsulet-api
```

Important local API surfaces:

- `GET /healthz`
- `POST /v1/graphs`
- `GET /v1/graphs`
- `POST /v1/agents`
- `GET /v1/agents`
- `POST /v1/agents/{id}/runs`
- `GET /v1/agent-runs`
- `POST /v1/jobs/runs`
- `GET /v1/jobs/runs`
- `GET /v1/jobs/runs/{id}`
- `GET /v1/jobs/runs/{id}/logs`
- `POST /v1/jobs/runs/{id}/cancel`
- `GET /v1/jobs/runs/{id}/artifacts`
- `GET /v1/jobs/runs/{id}/artifacts/{artifact_id}`

See `docs/api.md` for request examples.
The machine-readable OpenAPI document is available at `GET /openapi.json`.

## CLI

The CLI talks to the HTTP API. Start the API first, then run:

```powershell
$env:CAPSULET_API_URL = "http://127.0.0.1:8080"
$env:CAPSULET_API_TOKEN = "replace-me"
cargo run -p capsulet-cli -- submit job_hello_python --pool mini
cargo run -p capsulet-cli -- runs
cargo run -p capsulet-cli -- run get run_123
cargo run -p capsulet-cli -- status run_123
cargo run -p capsulet-cli -- logs run_123
cargo run -p capsulet-cli -- artifacts list run_123
cargo run -p capsulet-cli -- completions powershell > capsulet.ps1
```

You can also pass the API URL per command:

```sh
cargo run -p capsulet-cli -- --api-url http://127.0.0.1:8080 runs --limit 25
```

## Worker

The worker can execute a single tick or poll continuously with the stub, local-process, or Kubernetes Job runner. `CAPSULET_WORKER_LOOP=true` enables polling; `CAPSULET_WORKER_POLL_SECONDS` controls the idle delay.

Run a success tick:

```powershell
$env:CAPSULET_DATABASE_URL = Get-Content .capsulet/stack/capsulet-local/database-url
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
npm ci
npm run dev
```

The `dashboard` gate runs ESLint and the production build, and the `sdk` gate the dashboard's
transport tests and `tsc`; `dashboard-e2e` runs Playwright against a stack of its own:

```sh
cargo run -p capsulet-xtask --locked -- verify --gate dashboard --gate sdk --gate dashboard-e2e
```

Point the dashboard at a local API:

```powershell
$env:CAPSULET_DASHBOARD_API_URL = "http://127.0.0.1:8080"
npm run dev
```

The dashboard uses the live API for authored jobs, workflows, automations, runs, artifacts, identity, and audit events. See `dashboard/README.md`.

## Helm Chart

Run from the repository root:

```sh
cargo run -p capsulet-xtask --locked -- verify --gate helm
```

That lints and renders the chart, and `release check-chart` then renders it twice. With the
defaults and with each component's digest set, it holds every workload to:

- the image `release.toml` names;
- a non-root, seccomp-confined pod whose containers cannot escalate privileges and hold no
  capabilities;
- read-only root filesystems for Capsulet's own containers;
- service-account tokens only where a pod uses the Kubernetes API;
- readiness and liveness probes;
- only the secrets the chart renders or its values name.

The `kind` gate then installs the chart on a Kind cluster of its own and exercises it there:

```sh
cargo run -p capsulet-xtask --locked -- verify --gate helm --gate kind
```

It builds every image from the checkout and loads it into a cluster named for the verifier run,
with a kubeconfig of its own, so your kubeconfig and your other clusters are never touched. It
generates the test credentials, validates every manifest against the API server before installing,
runs `helm test` and an in-cluster smoke, and uninstalls. Whatever happens, it deletes that cluster.
[CI](development/ci.md#the-kind-gate) describes each step, how CI runs the gates, and how to
reproduce a failed job.

See [local Kubernetes](local-kubernetes-runner.md) to install it on minikube.

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
