# Security

Capsulet has two authentication paths:

- OIDC JWT bearer tokens from Keycloak for human dashboard users.
- Service bearer tokens from `CAPSULET_API_TOKENS` for CLI, automation, and bootstrap use.

The dashboard does not ask users to paste API tokens. Local compose provides:

- Keycloak: `http://localhost:18080`, realm `capsulet`
- temporary dashboard admin: username `admin`, password `admin`

Change or disable the temporary admin before any shared environment.

## Authorization model

Roles are ordered:

- `viewer`: read-only API/dashboard access
- `operator`: run/cancel/operate workflows and jobs
- `admin`: create/update/delete definitions, automations, plugins, and security settings

Service tokens can be narrowed with explicit scopes. When `scopes` is omitted,
Capsulet grants the default scope set for the configured role to preserve
backward compatibility. A scoped token can also carry `expires_at_unix`:

```json
[
  {
    "name": "ci-runner",
    "role": "operator",
    "token": "replace-with-32-plus-random-bytes",
    "scopes": ["jobs:run", "jobs:read"],
    "expires_at_unix": 4102444800
  }
]
```

For production, create database-backed service accounts from the Security page
or API:

- `GET /v1/service-accounts`
- `POST /v1/service-accounts`
- `POST /v1/service-accounts/{id}/revoke`

The API returns the token only once at creation time. The database stores only a
SHA-256 token hash, tracks `last_used_at`, supports `expires_at_unix`, and uses
`revoked_at` for rotation. `CAPSULET_API_TOKENS` should remain a small bootstrap
set, preferably limited to break-glass administrators.

Supported scope families are:

- `auth:read`
- `jobs:read`, `jobs:run`, `jobs:cancel`, `jobs:write`
- `workflows:read`, `workflows:operate`, `workflows:write`
- `automations:read`, `automations:operate`, `automations:write`
- `audit:read`
- `system:read`, `system:write`
- `resource:*` wildcard scopes, plus `*` for administrators

Keycloak realm/client roles accepted by the API:

- `capsulet-platform-admin`: full platform administration across all projects
- `capsulet-viewer` or `viewer`
- `capsulet-operator` or `operator`
- `capsulet-admin` or `admin`

`capsulet-admin` and `admin` remain accepted as compatibility aliases. New
deployments should grant `capsulet-platform-admin` for global administrators.

## Project IAM

Capsulet uses a hybrid IAM model for internal enterprise deployments:

- Keycloak owns login, SSO, MFA, and platform-admin assignment.
- Capsulet owns project membership and project-level roles after login.
- A signed-in user can belong to multiple Capsulet projects at once.
- Project memberships are stored in `project_memberships`.

Project roles:

- `project_viewer`: read project resources, runs, logs, artifacts, and audit events
- `project_operator`: viewer plus run/cancel/resume operations
- `project_admin`: operator plus resource, service-account, and member management

`GET /v1/auth/me` returns the caller's `platform_admin` flag and
`project_memberships`. `GET /v1/projects` returns only projects visible to the
caller. The dashboard project switcher is a convenience control; backend APIs
must continue to enforce project scope on resource access.

## Runtime isolation controls

The Kubernetes runner uses:

- non-root execution containers
- `allowPrivilegeEscalation: false`
- dropped Linux capabilities
- read-only root filesystem with bounded writable volumes
- disabled service-account token automount for execution pods
- active deadlines
- deterministic job ownership labels for reattachment

Recommended production settings:

- use namespace-per-pool for untrusted workloads
- configure image allowlists per execution pool
- use digest-pinned images for sensitive pools
- apply default-deny NetworkPolicy and explicit egress presets
- separate Capsulet control-plane namespace from execution namespaces
- use RuntimeClass sandboxing when available

## Egress presets

Suggested policies:

- `none`: no external egress; allow only DNS if needed
- `cluster`: cluster service CIDRs only
- `internet`: HTTPS egress only through an audited proxy
- `custom`: explicit CIDR/FQDN policy maintained by platform operators

## Threat model summary

Capsulet protects:

- API operations through authentication, RBAC, and audit events
- workflow/job state through guarded database transitions
- Kubernetes job ownership through deterministic labels and reattachment checks
- untrusted code boundaries through pod security defaults

Capsulet does not by itself protect against:

- malicious images allowed by an administrator
- cluster-admin users bypassing Kubernetes policy
- compromised object storage credentials
- unrestricted egress policies
- side channels between workloads on the same node

Production deployments should pair Capsulet with Kubernetes Pod Security Admission, NetworkPolicy enforcement, image admission policy, secret rotation, and centralized audit logging.

Example ValidatingAdmissionPolicy manifests live in `ops/admission/`. The digest
image policy is shipped in audit mode by default because local development often
uses tag-based images; production clusters should switch it to `Deny` after all
execution images are digest pinned.

## Tenancy

The persistence schema includes `tenants`, `projects`, `project_memberships`,
and `tenant_id` / `project_id` ownership columns on core resources. Existing
installs migrate to the `default` tenant/project. Principals now carry
tenant/project context and project memberships. Continue to use one tenant per
cluster for strict isolation until every resource-specific query is covered by
project-scope tests.
