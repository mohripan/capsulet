# ADR 0007: PostgreSQL and SQLx Persistence

Status: Accepted

## Context

Sprint 002 needs durable metadata for manual job runs. The persistence layer must support job definitions, queued runs, attempts, leasing, and later API/worker access without pulling database concerns into the domain core.

Capsulet also needs a migration approach that works locally, in CI, and eventually inside Helm-managed deployments.

## Decision

Use PostgreSQL as the durable metadata store and SQLx as the Rust database stack.

SQL migrations live in the repository-level `migrations/` directory and are embedded by the `capsulet-postgres` crate. The core crate exposes repository ports; `capsulet-postgres` implements those ports.

Local development uses Docker Compose with PostgreSQL 16.

## Consequences

- `capsulet-core` remains free of database dependencies.
- SQL remains explicit and reviewable.
- Migrations can be run by services on startup or by future deployment jobs.
- PostgreSQL is the source of truth for control-plane metadata, not script bundles, logs, or artifacts.
- Later service crates can share the same adapter instead of each owning database code.
