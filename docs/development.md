# Development

This guide describes the Sprint 001 development workflow for Capsulet.

## Required Tools

- Rust 1.87 or newer
- Cargo
- Node.js 20.x
- npm 10.x
- Helm 3.18 or newer

Optional later tools:

- Docker
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
- `capsulet-api`: future HTTP control plane
- `capsulet-worker`: future job leasing and Kubernetes Job coordination
- `capsulet-scheduler`: future schedule and delay scanner
- `capsulet-evaluator`: future automation condition evaluator
- `capsulet-runner`: future execution backend boundary
- `capsulet-cli`: future CLI

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
- Service crates should stay thin until runtime behavior exists.
- Prefer ADRs for decisions that change architecture or operational behavior.
- Keep Helm as a first-class product distribution surface.

## Current Sprint

Sprint planning lives in `planning/`.

- Sprint plan: `planning/sprints/sprint-001-foundation.md`
- Sprint backlog: `planning/backlog/sprint-001-backlog.md`
