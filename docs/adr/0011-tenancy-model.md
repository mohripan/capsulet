# ADR 0011: Tenancy Model

## Status

Accepted

## Context

Enterprise buyers need a clear isolation model. Capsulet now has tenant/project schema foundations and principals carry tenant/project context. Internal enterprise deployments also need one control plane shared by many departments without requiring project owners to manage Keycloak.

## Decision

Capsulet remains single-tenant per cluster for strict tenant isolation. Inside that tenant, departments and product teams are represented as Capsulet projects.

Keycloak owns login, SSO/MFA, and platform-admin assignment through `capsulet-platform-admin`. Capsulet owns project membership, project roles, service-account ownership, and resource authorization after login.

The database schema includes `tenants`, `projects`, `project_memberships`, and ownership columns on core resources. Existing rows migrate to the `default` tenant/project. Service-account tokens are tenant/project-scoped at creation time.

## Consequences

- Platform administrators are assigned outside Capsulet through Keycloak.
- Project administrators manage project memberships inside Capsulet.
- The dashboard exposes visible projects through a project switcher, but API authorization remains the security boundary.
- Completing hard multi-project isolation requires project-scope coverage on every API/store query, tenant-aware object key prefixes, and per-project quotas.
