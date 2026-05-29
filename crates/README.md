# Backend Workspace

Capsulet's backend is a Rust workspace organized around a small domain core and thin service crates.

## Crates

- `capsulet-core`: domain model, application command/query shapes, and infrastructure ports
- `capsulet-api`: future HTTP control plane service
- `capsulet-worker`: future run leasing and Kubernetes Job coordination service
- `capsulet-scheduler`: future scheduled and delayed trigger scanner
- `capsulet-evaluator`: future automation condition evaluator
- `capsulet-runner`: future execution backend boundary
- `capsulet-cli`: future operator and developer CLI

## Architecture Direction

The current structure uses a DDD-style core with CQRS-friendly application boundaries:

- `domain`: aggregates, value objects, state transitions, and domain rules
- `application`: command and query shapes
- `ports`: traits that future infrastructure adapters will implement

Infrastructure dependencies such as PostgreSQL, Kubernetes, Kafka, and HTTP frameworks should not be added to `capsulet-core`.

## Checks

Run from the repository root:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Useful metadata check:

```sh
cargo metadata --no-deps --format-version 1
```
