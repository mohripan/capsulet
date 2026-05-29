# ADR 0002: Rust Workspace and Core Boundaries

Status: Accepted

## Context

Capsulet will have multiple backend responsibilities: API, worker, scheduler, evaluator, runner, and CLI. These components should stay independently deployable over time while sharing the same product language and domain rules.

The project also needs a foundation that is useful now without prematurely introducing database clients, Kubernetes clients, Kafka clients, or web frameworks.

## Decision

Use a Cargo workspace with one shared domain/application crate and thin service crates:

- `capsulet-core`: domain model, application command/query shapes, and infrastructure ports
- `capsulet-api`: future HTTP control plane
- `capsulet-worker`: future job leasing and execution coordinator
- `capsulet-scheduler`: future scheduled and delayed trigger scanner
- `capsulet-evaluator`: future automation condition evaluator
- `capsulet-runner`: future execution backend boundary
- `capsulet-cli`: future command-line client

The first backend foundation follows these boundaries:

- domain concepts live in `capsulet-core::domain`
- command/query shapes live in `capsulet-core::application`
- infrastructure traits live in `capsulet-core::ports`
- service crates stay thin until real runtime concerns exist

This gives Capsulet a DDD-style core and a CQRS-friendly application boundary without adding framework complexity before the first manual job runner exists.

## Consequences

- Domain rules can be tested without infrastructure.
- Service crates can evolve independently.
- The API, worker, scheduler, and evaluator share one consistent state machine and automation language.
- Future PostgreSQL, Kubernetes, Kafka, and HTTP implementations can depend on core ports instead of leaking into the domain.
- The project avoids a premature hexagonal architecture explosion while preserving the boundary needed for it later.
