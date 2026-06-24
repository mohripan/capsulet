# Hybrid Project IAM Design

## Goal

Capsulet will support one internal enterprise deployment shared by multiple departments or product teams. Keycloak remains the source of truth for login and platform administrators. Capsulet owns project membership, project roles, service-account ownership, and resource authorization.

## Identity Boundary

Keycloak authenticates human users and can grant the `capsulet-platform-admin` role. A platform admin can administer every Capsulet project and global platform setting. Non-platform users can sign in, but they receive no resource access until a Capsulet project admin or platform admin assigns them to one or more projects.

Capsulet stores project memberships for users, groups, and service accounts. The first implementation focuses on user and service-account memberships; group synchronization can be added after the membership contract is stable.

## Roles

- `project_viewer`: read project workflows, automations, job definitions, runs, logs, artifacts, and project-scoped audit events.
- `project_operator`: `project_viewer` plus run, cancel, retry, and resume operations.
- `project_admin`: `project_operator` plus create, update, and delete project resources, manage project service accounts, and manage project members.
- `platform_admin`: Keycloak-only global role with access to all projects and platform settings.

## Authorization Model

Every protected API operation checks both action permission and project scope:

```text
authenticated principal
  -> resolve platform role and project memberships
  -> determine target project from request or resource ownership
  -> check required permission for that project
  -> execute storage query filtered by tenant_id + project_id
```

Dashboard filtering is not a security boundary. The API and persistence layer must prevent cross-project reads, writes, operations, log reads, artifact downloads, service-account access, and audit access.

## Data Model

Capsulet already has `tenants`, `projects`, and `tenant_id` / `project_id` ownership columns on core resources. The IAM hardening adds a `project_memberships` table with principal kind, principal name, tenant, project, role, creator, and timestamps. Service accounts remain project-scoped by default.

All new resources are created inside an explicit active project. Existing installs continue to use the `default` tenant and `default` project until more projects are created.

## API Surface

New and expanded API behavior:

- `/v1/auth/me` returns platform role plus authorized project memberships.
- `/v1/projects` lists projects visible to the caller.
- `/v1/projects/{project_id}` returns project details when visible.
- `/v1/projects/{project_id}/members` lets project admins and platform admins manage memberships.
- Service-account APIs are scoped to the caller's authorized projects.
- Existing workflow, automation, job-definition, run, log, artifact, and audit APIs enforce project ownership.

## Dashboard UX

The dashboard gets a project switcher in the main shell. All pages use the active project by default. Platform admins can view all projects and create projects. Project admins can manage members and service accounts only for their projects.

## Error Handling

Cross-project reads should generally return `404` for resource-specific endpoints to avoid leaking existence. List endpoints return only authorized resources. Mutations against a known but unauthorized project return `403`.

## Testing

Required test coverage:

- OIDC platform-admin claim maps to global platform access.
- Non-platform users receive no project access without membership.
- Project members see only their projects.
- Cross-project workflow, automation, job, run, log, artifact, audit, and service-account access is blocked.
- Dashboard E2E verifies project switcher filtering and membership management.
