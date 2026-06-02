# ADR 0008: Axum for HTTP API

Status: Accepted

## Context

Sprint 002 needs a small HTTP control plane for health checks, manual job submission, and job-run queries. The framework should fit Rust async services, keep handler tests straightforward, and avoid unnecessary structure before the API surface stabilizes.

## Decision

Use Axum for `capsulet-api`.

The API crate exposes a testable router from `src/lib.rs`, while `src/main.rs` handles runtime configuration, PostgreSQL connection setup, migrations, and server startup.

## Consequences

- Route handlers can be tested without binding a TCP port.
- The API can share Tokio and Tower ecosystem tools.
- The API crate depends on infrastructure adapters, but `capsulet-core` stays framework-free.
- Future middleware for auth, tracing, request IDs, and metrics can be added through the Axum/Tower stack.
