# Sprint 002 Backlog

This is the working backlog for Sprint 002: Manual Job Runner.

Status legend:

- `todo`: not started
- `doing`: in progress
- `blocked`: waiting on a decision or prerequisite
- `done`: complete

## Persistence

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S2-DB-001 | done | Choose and add PostgreSQL Rust stack | Database approach is documented and dependencies are added |
| S2-DB-002 | done | Add migration structure | Migrations can be run locally |
| S2-DB-003 | done | Create job definition table | Known job definitions can be stored |
| S2-DB-004 | done | Create job run table | Queued runs can be persisted and queried |
| S2-DB-005 | done | Create job attempt table | Attempts can be recorded against runs |
| S2-DB-006 | done | Add job run repository adapter | Adapter implements the core repository boundary |

## API

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S2-API-001 | done | Add API framework | API binary starts an HTTP server |
| S2-API-002 | done | Add `GET /healthz` | Health endpoint returns success |
| S2-API-003 | done | Add `POST /v1/jobs/runs` | Manual job submission creates queued run |
| S2-API-004 | done | Add `GET /v1/jobs/runs` | API lists runs |
| S2-API-005 | done | Add `GET /v1/jobs/runs/{id}` | API fetches one run |
| S2-API-006 | done | Add API validation errors | Unknown job definition or pool returns clear error |
| S2-API-007 | done | Add API tests | Success and failure paths are covered |

## Job Definitions

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S2-JOB-001 | done | Add minimal job definition model | Model supports ID, runtime image, command, and bundle object key |
| S2-JOB-002 | done | Seed `job_hello_python` | Manual testing has a known job definition |
| S2-JOB-003 | done | Validate job definition references | Submissions cannot reference unknown definitions |

## Worker and Runner

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S2-WORKER-001 | done | Add lease use case | Worker can lease one queued run |
| S2-WORKER-002 | done | Add lease owner and expiry fields | Lease metadata is persisted |
| S2-WORKER-003 | done | Prevent duplicate leases | Tests prove two workers cannot lease the same run |
| S2-WORKER-004 | done | Add runner trait | Execution is behind a testable boundary |
| S2-WORKER-005 | done | Add stub success runner | Leased run can become succeeded |
| S2-WORKER-006 | done | Add stub failure runner | Leased run can become failed |

## CLI and Manual Testing

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S2-CLI-001 | done | Add `capsulet submit` | CLI can submit to local API |
| S2-CLI-002 | done | Add `capsulet runs` | CLI can list runs |
| S2-CLI-003 | done | Add `capsulet run get` | CLI can fetch one run |
| S2-DOC-001 | done | Add manual testing guide | Contributor can submit and inspect a run locally |
| S2-DOC-002 | done | Update development docs | Local database commands are documented; API commands remain covered by API tasks |

## Quality

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S2-QA-001 | done | Keep `cargo fmt --check` passing | Formatting passes |
| S2-QA-002 | done | Keep clippy passing | `cargo clippy --workspace --all-targets -- -D warnings` passes |
| S2-QA-003 | done | Keep workspace tests passing | `cargo test --workspace` passes |
| S2-QA-004 | done | Add focused integration tests where practical | Repository has Docker-backed coverage; API/worker have use-case coverage and smoke verification |

## Completed Notes

Persistence foundation completed:

- Added `compose.yaml` with local PostgreSQL 16.
- Added repository-level SQL migration structure in `migrations/`.
- Added initial schema for `job_definitions`, `job_runs`, and `job_attempts`.
- Added `capsulet-postgres` as the SQLx-backed persistence adapter crate.
- Updated `capsulet-core` repository port to support async persistence.
- Added job run save, find, list, and queued-run lease operations in the PostgreSQL adapter.
- Added Docker-backed repository test coverage for migrations and persisted job runs.
- Added persistence documentation in `docs/persistence.md`.
- Added ADR 0007 for PostgreSQL and SQLx.
- Verified `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `cargo test -p capsulet-postgres` against local PostgreSQL.

API foundation completed:

- Added Axum as the API framework.
- Split `capsulet-api` into a testable router library and runtime binary.
- Added `GET /healthz`.
- Added `POST /v1/jobs/runs` for manual job-run creation.
- Added `GET /v1/jobs/runs` for job-run listing.
- Added `GET /v1/jobs/runs/{id}` for single run lookup.
- Added validation errors for invalid identifiers, unknown job definitions, unknown execution pools, and missing runs.
- Added API unit tests with an in-memory fake store.
- Added API documentation in `docs/api.md`.
- Added ADR 0008 for Axum.
- Verified the API manually against Docker PostgreSQL with health, create, list, and fetch requests.

Job definition and worker foundation completed:

- Added the `JobDefinition` domain model with runtime image, command, bundle object key, and input schema.
- Added built-in `job_hello_python` definition for local/manual testing.
- Added optional API startup seeding with `CAPSULET_SEED_EXAMPLES=true`.
- Added the `capsulet-runner` trait boundary and deterministic success/failure stub runners.
- Added the `capsulet-worker` lease-and-run use case.
- Added a worker binary that executes one queued run per invocation.
- Added PostgreSQL duplicate-lease coverage using `FOR UPDATE SKIP LOCKED`.
- Added worker tests for empty queue, stub success, and stub failure paths.
- Added worker/runner documentation in `docs/worker-runner.md`.
- Added ADR 0009 for the runner boundary.
- Verified the API plus worker path manually against Docker PostgreSQL: create queued run, execute worker success tick, fetch succeeded run with one attempt.

CLI foundation completed:

- Added `capsulet submit` for manual run submission against the local API.
- Added `capsulet runs` for listing runs with a configurable limit.
- Added `capsulet run get` for fetching one run by ID.
- Added `CAPSULET_API_URL` and `--api-url` support for pointing the CLI at a local API.
- Added focused CLI parsing, URL-building, and output-formatting tests.

## ADR Candidates

Create ADRs only if the implementation forces durable choices:

- API framework selection. Done in ADR 0008.
- PostgreSQL client and migration tooling. Done in ADR 0007.
- local development dependency strategy
- runner trait shape. Done in ADR 0009.

## Sprint Risks

- Database setup can consume the sprint. Keep schema narrow.
- API framework choice can distract from the state path. Pick a boring option and move.
- CLI work is useful but secondary. Prefer API and worker correctness.
- Do not start Kubernetes execution in this sprint.
