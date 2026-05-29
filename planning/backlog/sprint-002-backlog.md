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
| S2-API-001 | todo | Add API framework | API binary starts an HTTP server |
| S2-API-002 | todo | Add `GET /healthz` | Health endpoint returns success |
| S2-API-003 | todo | Add `POST /v1/jobs/runs` | Manual job submission creates queued run |
| S2-API-004 | todo | Add `GET /v1/jobs/runs` | API lists runs |
| S2-API-005 | todo | Add `GET /v1/jobs/runs/{id}` | API fetches one run |
| S2-API-006 | todo | Add API validation errors | Unknown job definition or pool returns clear error |
| S2-API-007 | todo | Add API tests | Success and failure paths are covered |

## Job Definitions

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S2-JOB-001 | todo | Add minimal job definition model | Model supports ID, runtime image, command, and bundle object key |
| S2-JOB-002 | todo | Seed `job_hello_python` | Manual testing has a known job definition |
| S2-JOB-003 | todo | Validate job definition references | Submissions cannot reference unknown definitions |

## Worker and Runner

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S2-WORKER-001 | todo | Add lease use case | Worker can lease one queued run |
| S2-WORKER-002 | todo | Add lease owner and expiry fields | Lease metadata is persisted |
| S2-WORKER-003 | todo | Prevent duplicate leases | Tests prove two workers cannot lease the same run |
| S2-WORKER-004 | todo | Add runner trait | Execution is behind a testable boundary |
| S2-WORKER-005 | todo | Add stub success runner | Leased run can become succeeded |
| S2-WORKER-006 | todo | Add stub failure runner | Leased run can become failed |

## CLI and Manual Testing

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S2-CLI-001 | todo | Add `capsulet submit` | CLI can submit to local API |
| S2-CLI-002 | todo | Add `capsulet runs` | CLI can list runs |
| S2-CLI-003 | todo | Add `capsulet run get` | CLI can fetch one run |
| S2-DOC-001 | todo | Add manual testing guide | Contributor can submit and inspect a run locally |
| S2-DOC-002 | done | Update development docs | Local database commands are documented; API commands remain covered by API tasks |

## Quality

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S2-QA-001 | done | Keep `cargo fmt --check` passing | Formatting passes |
| S2-QA-002 | done | Keep clippy passing | `cargo clippy --workspace --all-targets -- -D warnings` passes |
| S2-QA-003 | done | Keep workspace tests passing | `cargo test --workspace` passes |
| S2-QA-004 | doing | Add focused integration tests where practical | Repository behavior has Docker-backed coverage; API/worker coverage remains pending |

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

## ADR Candidates

Create ADRs only if the implementation forces durable choices:

- API framework selection
- PostgreSQL client and migration tooling. Done in ADR 0007.
- local development dependency strategy
- runner trait shape

## Sprint Risks

- Database setup can consume the sprint. Keep schema narrow.
- API framework choice can distract from the state path. Pick a boring option and move.
- CLI work is useful but secondary. Prefer API and worker correctness.
- Do not start Kubernetes execution in this sprint.
