# Sprint 001: Foundation

## Sprint Goal

Establish a working foundation for Capsulet so the next sprint can start implementing the manual job runner without first fighting project setup.

By the end of this sprint, the repository should have:

- a buildable Rust workspace
- a runnable Next.js dashboard prototype
- a valid starter Helm chart
- basic CI for backend, frontend, and chart checks
- local development documentation
- a clear ADR trail for the first technical choices

## Sprint Length

Recommended: 1 week.

This sprint is intentionally infrastructure-heavy. Keep it small and concrete; do not start the real job execution backend until the foundation can build reliably.

## Sprint Theme

Foundation over features.

The output should be boring in the right way: commands work, directories make sense, CI runs, docs explain how to get started, and the codebase is ready for Sprint 002.

## Committed Scope

### 1. Rust Workspace Foundation

Create a real Rust workspace with placeholder service crates.

Crates:

- `crates/core`
- `crates/api`
- `crates/worker`
- `crates/scheduler`
- `crates/evaluator`
- `crates/runner`
- `crates/cli`

Expected work:

- add root `Cargo.toml`
- add `Cargo.toml` for each crate
- make `core` a library crate
- make service crates minimal binaries where appropriate
- add shared workspace dependency versions
- add basic `cargo fmt`, `cargo clippy`, and `cargo test` workflow
- add simple smoke tests where useful

Acceptance criteria:

- `cargo fmt --check` passes
- `cargo clippy --workspace --all-targets -- -D warnings` passes
- `cargo test --workspace` passes
- every crate has a clear one-line purpose in its `Cargo.toml` or README

### 2. Next.js Dashboard Foundation

Keep the current dashboard prototype and make it easier to run consistently.

Expected work:

- keep the multi-page mock dashboard
- add a dashboard README
- document Node.js and npm requirements
- document `npm install`, `npm run dev`, `npm run build`, and `npx tsc --noEmit`
- decide whether the current `next build` Windows hang needs a tracked issue or dependency adjustment
- keep mock data isolated from future API integration

Acceptance criteria:

- `npm install` works from `dashboard/`
- `npx tsc --noEmit` passes
- `npm run dev` serves the dashboard
- dashboard README explains all available mock routes

### 3. Helm Chart Skeleton

Create a valid starter Helm chart for the future product install.

Expected work:

- add `charts/capsulet/Chart.yaml`
- add `charts/capsulet/values.yaml`
- add `charts/capsulet/values.schema.json`
- add starter templates for service accounts, config, API, worker, scheduler, evaluator, dashboard, services, and tests
- include execution pool values in the initial values file
- keep templates simple and mostly placeholder, but valid

Acceptance criteria:

- `helm lint charts/capsulet` passes
- `helm template capsulet charts/capsulet` renders valid YAML
- chart values include image settings and execution pool defaults
- chart metadata matches the project name and Apache-2.0 license

### 4. Local Development Documentation

Write the minimum docs needed to onboard yourself back into the project quickly.

Expected docs:

- `docs/development.md`
- `docs/installation.md`
- `docs/helm-values.md` as an initial stub
- update `README.md` if commands change

Acceptance criteria:

- docs explain required tools
- docs explain how to run dashboard
- docs explain planned backend commands
- docs explain how to lint/render the Helm chart

### 5. CI Foundation

Add GitHub Actions skeletons that match the foundation commands.

Expected workflows:

- backend Rust checks
- dashboard checks
- Helm chart lint/template checks

Acceptance criteria:

- CI workflow files exist under `.github/workflows/`
- commands match local documentation
- workflows are allowed to be simple and grow later

### 6. Architecture Decision Records

Capture the first technical decisions as ADRs.

Recommended ADRs:

- Rust workspace and service crate layout
- Next.js dashboard choice
- Helm chart as product distribution
- object storage for scripts, logs, and artifacts
- Kafka as target event channel

Acceptance criteria:

- ADRs are concise
- each ADR has status, context, decision, and consequences
- ADRs do not duplicate the full architecture document

## Stretch Scope

Only do these after the committed scope is complete:

- add Dockerfiles for Rust services
- add dashboard Dockerfile
- add `docker-compose.yml` for local PostgreSQL, MinIO, and Kafka
- add `justfile`, `Makefile`, or `Taskfile.yml` for common commands
- add initial OpenAPI placeholder

## Explicit Non-Goals

- no real API endpoints
- no database schema
- no migrations
- no Kubernetes Job runner
- no object storage implementation
- no authentication
- no real automation evaluator
- no Kafka integration yet
- no production-ready dashboard data fetching

## Definition of Done

Sprint 001 is done when:

- Rust workspace builds and tests successfully
- dashboard can be run locally
- Helm chart lints and templates successfully
- CI exists for the same checks
- local development docs explain the workflow
- the architecture and roadmap still match the repository structure
- Sprint 002 can start on manual job submission and persistence

## Suggested Work Order

1. Create the Rust workspace and crate manifests.
2. Add minimal Rust code and tests so backend checks pass.
3. Add dashboard README and verify frontend commands.
4. Create Helm chart metadata, values, schema, and minimal templates.
5. Add development and installation docs.
6. Add CI workflows.
7. Add ADRs for decisions made during the sprint.
8. Review the sprint backlog and move unfinished work back to the product backlog.

## Sprint Review Checklist

- Can a fresh clone run the documented commands?
- Does the dashboard still communicate the product direction visually?
- Does the Helm chart look like the beginning of the product distribution?
- Are future backend services represented by crates with clear ownership?
- Is any decision hidden in code that should be captured as an ADR?
- Is Sprint 002 ready to focus on the manual job runner?

## Sprint 002 Preview

Sprint 002 should likely focus on the first manual job runner slice:

- API accepts a manual job submission
- job run and attempt domain types exist
- PostgreSQL schema starts
- worker leases a queued run
- runner abstraction is introduced
- Kubernetes runner can be stubbed before real cluster integration
