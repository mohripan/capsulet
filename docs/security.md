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

Keycloak realm/client roles accepted by the API:

- `capsulet-viewer` or `viewer`
- `capsulet-operator` or `operator`
- `capsulet-admin` or `admin`

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
