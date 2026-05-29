# Sprint 002: Manual Job Runner

## Sprint Goal

Build the first real backend slice: a manual job submission flow that creates durable job run state, can be queried, and can be leased by a worker using a stub runner.

By the end of this sprint, Capsulet should support the first internal end-to-end path:

1. API receives a manual job submission.
2. The request is validated.
3. A job run is persisted.
4. The run can be listed and fetched.
5. A worker can lease a queued run.
6. A stub runner can mark the run succeeded or failed.

This sprint should prove the backend architecture without trying to execute real Kubernetes Jobs yet.

## Sprint Length

Recommended: 1 week.

## Sprint Theme

Durable state before real execution.

The goal is not to run user code yet. The goal is to make the job state machine real, persistence-backed, testable, and accessible through early API/CLI surfaces.

## Committed Scope

### 1. Persistence Foundation

Add PostgreSQL as the first durable infrastructure dependency.

Expected work:

- choose and add a Rust database stack
- add database configuration shape
- add initial migration structure
- create tables for job definitions, job runs, and job attempts
- add local development instructions for PostgreSQL
- decide whether to use Docker Compose for local PostgreSQL in this sprint

Recommended implementation:

- use `sqlx` for compile-time checked queries later
- start with runtime-checked queries if offline setup would slow the sprint
- keep persistence adapters outside `capsulet-core`

Acceptance criteria:

- migration files exist
- job run and attempt tables exist
- tests can run against a local or test database, or repository-level tests use an in-memory adapter until database test infrastructure is ready
- persistence code does not leak into `capsulet-core`

### 2. Manual Job Submission API

Create the first API surface for manual jobs.

Expected endpoints:

- `POST /v1/jobs/runs`
- `GET /v1/jobs/runs`
- `GET /v1/jobs/runs/{id}`
- `GET /healthz`

The first request shape can be minimal:

```json
{
  "jobDefinitionId": "job_hello_python",
  "executionPool": "mini",
  "input": {
    "name": "Ripan"
  }
}
```

Acceptance criteria:

- API can create a queued job run
- API can list job runs
- API can fetch one job run
- invalid execution pool or missing job definition returns a clear error
- API tests cover success and validation failures

### 3. Job Definition Seed

Add enough job definition support to submit a run without building the full YAML authoring system.

Expected work:

- define a minimal job definition model
- seed or insert a development job definition such as `job_hello_python`
- store script bundle object key placeholder, not actual script content
- keep input schema simple

Acceptance criteria:

- manual submission references a known job definition
- unknown job definition is rejected
- job definitions are persisted or seeded predictably for local development

### 4. Worker Leasing

Implement the first worker-side use case: lease queued work.

Expected behavior:

- worker asks for the next queued run
- repository transitions `queued -> leased`
- lease records owner and expiry
- worker can transition `leased -> running`
- worker can mark final state through stub execution

Acceptance criteria:

- two workers cannot lease the same run in tests
- expired lease behavior is designed, even if full recovery waits until Sprint 003
- state transitions still use domain rules from `capsulet-core`

### 5. Stub Runner

Add a runner abstraction and a fake runner implementation.

Expected work:

- define runner trait or application port
- implement stub runner that returns success
- support a forced failure path for tests
- do not create Kubernetes Jobs yet

Acceptance criteria:

- worker can execute a leased run with stub runner
- success path marks the run `succeeded`
- failure path marks the run `failed`
- tests cover both paths

### 6. CLI Smoke Commands

Add the first CLI commands for manual testing.

Target commands:

```sh
capsulet submit --job job_hello_python --pool mini --input '{"name":"Ripan"}'
capsulet runs
capsulet run get <run-id>
```

Acceptance criteria:

- CLI can call the local API
- CLI output is readable
- CLI handles API errors clearly

If CLI work threatens the sprint goal, keep CLI as stretch and use `curl` examples instead.

### 7. Documentation

Document the manual testing flow.

Expected docs:

- update `docs/development.md`
- add `docs/manual-job-runner.md`
- document local database setup
- document API examples with `curl`
- document CLI examples if implemented

Acceptance criteria:

- a contributor can start local dependencies, run API/worker, submit a job, and inspect state
- limitations are clearly documented

## Stretch Scope

Only do these after the committed scope is complete:

- dashboard reads real run list from API
- Docker Compose for PostgreSQL, MinIO, and Kafka
- OpenAPI document generation
- job input schema validation
- object storage placeholder adapter
- basic run logs table or object reference model

## Explicit Non-Goals

- no Kubernetes Job execution
- no real script bundle upload
- no object storage integration
- no Kafka integration
- no scheduled automations
- no webhook triggers
- no authentication
- no dashboard backend integration unless all core backend work is done
- no production-grade migration strategy yet

## Definition of Done

Sprint 002 is done when:

- API can create and query manual job runs
- job run state is persisted
- worker can lease and complete a run through a stub runner
- core domain rules remain covered by tests
- persistence and API behavior have focused tests
- local manual testing instructions exist
- Sprint 003 can start on real execution or object storage without reworking the state model

## Suggested Work Order

1. Add persistence crate or infrastructure module.
2. Add migrations for job definitions, job runs, and attempts.
3. Implement repository adapter for job runs.
4. Add API framework and health endpoint.
5. Implement manual submission endpoint.
6. Implement list/get run endpoints.
7. Implement worker leasing use case.
8. Add stub runner and final state updates.
9. Add CLI smoke commands or curl-only manual docs.
10. Update docs and sprint backlog.

## Sprint Review Checklist

- Can a manual run be submitted locally?
- Is the run actually durable?
- Can the run be fetched after API restart?
- Can the worker lease only one copy of a queued run?
- Are invalid state transitions still rejected?
- Is Kubernetes execution still cleanly isolated behind a runner boundary?
- Did any implementation decision need an ADR?

## Sprint 003 Preview

Sprint 003 should likely focus on one of these paths:

- real Kubernetes Job runner with kind/minikube
- object storage for script bundles and artifacts
- dashboard integration with real run APIs

Choose based on what Sprint 002 reveals.
