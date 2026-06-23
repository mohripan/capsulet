# ADR 0011: Tenancy Model

## Status

Accepted

## Context

Enterprise buyers need a clear isolation model. Capsulet now has tenant/project schema foundations and principals carry tenant/project context, but not every data-access query is tenant-filtered yet.

## Decision

Capsulet remains single-tenant per cluster for strict isolation in this release. Enterprise isolation is provided by deploying one Capsulet control plane per tenant, backed by tenant-specific PostgreSQL, object storage, Kubernetes namespace, secrets, and identity configuration.

The database schema includes `tenants`, `projects`, and ownership columns on core resources. Existing rows migrate to the `default` tenant/project. Service-account tokens are tenant/project-scoped at creation time.

## Consequences

- All API users within one deployment can see the deployment's shared resources according to their role until row-level filtering is completed.
- Procurement and operations documentation must state this explicitly.
- Completing in-database multi-tenancy requires row-level filtering in every API/store query, tenant-aware object key prefixes, per-tenant quotas, and dashboard tenant switching.
