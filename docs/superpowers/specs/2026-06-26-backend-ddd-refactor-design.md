# Backend DDD Refactor Design

## Context

Capsulet is a Rust workspace with an existing separation between domain, PostgreSQL persistence, API, worker, scheduler, evaluator, storage, runner, and CLI crates. The current architecture already points toward clean architecture: `capsulet-core` is dependency-light and contains domain types, state transitions, workflow graph validation, and some repository/application contracts.

The backend still has several large mixed-responsibility modules. The biggest hotspots are `crates/api/src/http.rs`, `crates/runner/src/lib.rs`, and `crates/worker/src/worker.rs`. These files combine transport, orchestration, adapter logic, persistence calls, and execution details. The project has not been released yet, so internal Rust APIs can change freely if endpoint behavior, database behavior, and deployment behavior remain verified.

## Goals

- Introduce a dedicated `capsulet-application` crate for use cases, application services, and ports.
- Make `capsulet-core` a stricter domain crate that owns entities, value objects, invariants, status transitions, graph validation, and domain errors.
- Keep infrastructure concerns out of `capsulet-core` and `capsulet-application` except through explicit ports.
- Make API, worker, scheduler, and evaluator crates thinner runtime or transport adapters.
- Preserve existing HTTP behavior, database schema, deployment configuration, and product workflows during the refactor.
- Improve testability by moving business orchestration behind application service APIs with focused tests.

## Non-Goals

- Do not split every bounded context into its own crate in this pass.
- Do not redesign the PostgreSQL schema or migrations.
- Do not change public REST endpoint paths or response semantics as part of the refactor.
- Do not replace Axum, SQLx, Tokio, Kubernetes, Docker Compose, or Helm.
- Do not pursue a pure DDD implementation when a simpler clean architecture boundary is clearer.

## Architecture

The new dependency direction is:

```text
capsulet-core
    ^
    |
capsulet-application
    ^
    |
adapters and runtimes:
  capsulet-postgres
  capsulet-storage
  capsulet-api
  capsulet-worker
  capsulet-scheduler
  capsulet-evaluator
  capsulet-runner
  capsulet-cli
```

`capsulet-core` contains pure domain concepts. It must not contain async repository traits, SQL-facing records, HTTP models, runner implementations, or process runtime logic.

`capsulet-application` contains use cases and ports. It depends on `capsulet-core` and defines the interfaces that infrastructure implements. It is the place for command/query objects, application services, orchestration errors, and transaction-shaped operations.

Infrastructure and runtime crates depend on `capsulet-application` and implement ports or call services. They can also depend on `capsulet-core` for domain types when useful, but business decisions belong in application services rather than adapters.

## Application Modules

`capsulet-application` starts with these modules:

- `jobs`: job definition create/update/list/delete, manual run creation, run listing/detail, cancellation, logs, artifacts, retry decisions, and worker finalization-facing operations.
- `workflows`: workflow definition create/update/list/delete, dependency graph orchestration, topology read models, workflow run cancellation, removal, resume, logs, and step run projections.
- `automations`: automation creation/update/list/delete, trigger/plugin validation, manual triggering, trigger condition handling, and compatibility handling for legacy interval/manual fields.
- `identity`: service accounts, projects, memberships, role/scope-facing operations, and audit-facing ownership checks.
- `execution`: worker lease-and-run orchestration, heartbeat coordination, script bundle materialization, upstream artifact loading, captured log persistence, artifact metadata persistence, and final outcome handling.

The crate can introduce `evaluation` or `retention` in a later phase if evaluator logic proves to be business orchestration rather than runtime polling.

## Component Ownership

`capsulet-core` owns nouns and rules:

- typed IDs;
- domain entities and value objects;
- job and workflow status transitions;
- workflow graph validation;
- condition expressions;
- execution pool value types;
- retry policy values;
- domain-specific parse/validation errors.

`capsulet-application` owns verbs and workflows:

- create job definition;
- queue manual run;
- finish worker attempt;
- resume workflow;
- trigger automation;
- create service account;
- map domain decisions into application errors.

`capsulet-postgres` owns SQL:

- SQLx pools and queries;
- row structs;
- migrations;
- row-to-domain and row-to-application mapping;
- application port implementations.

`capsulet-api` owns HTTP:

- route wiring;
- middleware and request context;
- authentication and authorization adapters;
- JSON request/response models;
- status-code and error response mapping;
- SSE response formatting.

`capsulet-worker` owns process runtime:

- environment/config parsing;
- selecting runner implementation;
- polling loop and health listener;
- calling `application::execution` services.

`capsulet-runner` owns execution mechanics:

- runner contracts used by worker/application execution;
- stub runner;
- local process runner;
- WASI Python runner;
- Kubernetes Job runner;
- Kubernetes Job rendering;
- execution artifact collection.

`capsulet-scheduler` and `capsulet-evaluator` own polling/runtime loops and delegate durable decisions to application services.

## Migration Plan

1. Create `crates/application` as `capsulet-application`.
2. Move existing command/query types and repository ports from `capsulet-core::application` and `capsulet-core::ports` into `capsulet-application`.
3. Update workspace manifests and imports so current behavior compiles through the new crate.
4. Extract worker lease/finalize orchestration from `crates/worker/src/worker.rs` into `capsulet-application::execution`, with tests around retry, cancellation, missing artifacts, log persistence, and lease loss.
5. Move API orchestration for jobs, workflows, automations, identity, logs, and artifacts into application services while leaving handlers responsible for HTTP extraction and response mapping.
6. Split `crates/api/src/http.rs` into route modules after use-case calls are isolated.
7. Split `crates/runner/src/lib.rs` into modules such as `contract`, `pools`, `stub`, `process`, `wasm_python`, and `kubernetes`, preserving public re-exports needed by callers.
8. Thin scheduler and evaluator by moving durable business decisions into application services where they are not already persistence-specific.
9. Run full verification after each major phase before starting the next one.

## Testing Strategy

Use test-first changes for newly extracted service APIs. The first tests lock behavior that is easiest to break during extraction:

- manual run command conversion;
- job retry/finalization decisions;
- worker handling of cancellation, lost leases, missing job definitions, missing script bundles, duplicate upstream artifacts, and large logs;
- workflow graph validation through the application service boundary;
- API handler behavior through existing fake-store tests after orchestration moves;
- runner module behavior through existing runner tests after file split.

Existing tests remain regression coverage. New application service tests prefer fake port implementations and real domain objects over mocks that only assert call counts.

## Verification

The expected verification sequence is:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --locked
```

For PostgreSQL-backed integration tests:

```powershell
docker compose up -d postgres minio minio-init
$env:CAPSULET_TEST_DATABASE_URL = "postgres://capsulet:capsulet@localhost:5432/capsulet"
cargo test -p capsulet-postgres --locked
```

For local product smoke:

```powershell
docker compose up --build -d
docker compose ps
```

For deployment shape:

```sh
helm lint charts/capsulet
helm template capsulet charts/capsulet
```

Minikube or kind validation runs after the Rust workspace and Compose stack are green, using the documented local Kubernetes runner flow. It verifies at least one Kubernetes-backed job run, logs, artifacts, cancellation, and worker reattachment where feasible.

## Risks

- Moving ports out of `capsulet-core` will cause broad import churn. Handle this mechanically first, before deeper behavior extraction.
- API tests may need fake implementations updated as ports move into `capsulet-application`.
- Worker orchestration is concurrency-sensitive. Heartbeat and lease behavior must stay covered while being moved.
- Runner splitting can break public re-exports used by worker tests and runtime configuration.
- A single giant refactor would be hard to debug. Keep each phase compiling and tested before continuing.

## Acceptance Criteria

- `capsulet-application` exists and owns application services and ports.
- `capsulet-core` no longer owns repository ports or application use-case orchestration.
- `capsulet-api`, `capsulet-worker`, `capsulet-scheduler`, and `capsulet-evaluator` delegate business decisions to application services where applicable.
- `crates/api/src/http.rs`, `crates/runner/src/lib.rs`, and `crates/worker/src/worker.rs` are reduced into smaller, purpose-specific modules.
- Existing behavior is preserved by tests and smoke checks.
- Formatting, clippy, workspace tests, PostgreSQL tests, Compose smoke, and Helm rendering all pass or have documented environmental blockers.
