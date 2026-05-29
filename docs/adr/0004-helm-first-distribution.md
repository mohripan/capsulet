# ADR 0004: Helm-First Distribution

Status: Accepted

## Context

Capsulet is intended to be an installable Kubernetes-native product, not only a source repository. The chart must install the control plane components, dashboard, RBAC, services, security defaults, and execution pool configuration.

## Decision

Treat the Helm chart as the primary product distribution.

The chart lives in `charts/capsulet` and starts with:

- `capsulet-api`
- `capsulet-worker`
- `capsulet-scheduler`
- `capsulet-evaluator`
- `capsulet-dashboard`
- service account and RBAC
- config map
- services for API and dashboard
- execution pool defaults
- chart test pod

## Consequences

- Chart values are part of the product API and should be reviewed carefully.
- Helm lint/template checks are required in CI.
- Local evaluation can start with bundled dependencies later, while production-shaped installs should support external PostgreSQL, object storage, and Kafka.
- Kubernetes-native concepts such as service accounts, security contexts, RBAC, and execution pools are first-class design surfaces.
