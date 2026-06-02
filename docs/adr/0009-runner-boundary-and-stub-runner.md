# ADR 0009: Runner Boundary and Stub Runner

Status: Accepted

## Context

Sprint 002 needs worker behavior before Kubernetes Job execution is ready. The worker should be able to lease queued runs, update durable state, and exercise success/failure transitions without coupling the use case to Kubernetes.

## Decision

Create a `capsulet-runner` crate that owns the `Runner` trait and deterministic stub runners.

The worker crate depends on the runner boundary and executes one leased run through a generic runner. Kubernetes Job execution will later become another runner implementation.

## Consequences

- Worker state transitions can be tested without Kubernetes.
- The future Kubernetes runner has a clear integration point.
- Sprint 002 can prove the durable queue path before introducing cluster execution complexity.
- Stub runners are development tools only; they are not the production execution backend.
