# ADR 0001: Record Architecture Decisions

Status: Accepted

## Context

Capsulet is expected to grow into a Kubernetes-native automation platform with several architectural tradeoffs around Helm distribution, execution pools, automation evaluation, object storage, and eventing.

## Decision

Use Architecture Decision Records in `docs/adr/` to capture important decisions before they become implementation constraints.

## Consequences

- Major design choices have a stable historical record.
- Future contributors can understand why a decision was made.
- Reversing a decision should happen through a new ADR rather than editing history.

