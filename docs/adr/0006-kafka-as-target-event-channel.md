# ADR 0006: Kafka as Target Event Channel

Status: Accepted

## Context

Capsulet's evaluator should not rely on PostgreSQL polling as the long-term event path. Trigger events, automation evaluation requests, run-created events, and lifecycle notifications need a durable event channel as the system grows.

## Decision

Use Kafka as the target production event channel.

PostgreSQL remains the source of truth for domain state. Kafka carries events between services. Consumers must be idempotent so replayed events do not create duplicate runs.

Early development may use an in-process or database-backed fallback until event contracts and service boundaries stabilize.

## Consequences

- Event contracts should be designed deliberately before Kafka integration starts.
- Local development may need a lightweight Kafka-compatible profile later.
- The evaluator can scale independently from API and scheduler workloads.
- Kafka integration is explicitly out of scope for Sprint 001.
