CREATE TABLE project_memberships (
    id text PRIMARY KEY,
    tenant_id text NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    project_id text NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    principal_kind text NOT NULL CHECK (principal_kind IN ('user', 'group', 'service_account')),
    principal_name text NOT NULL,
    role text NOT NULL CHECK (role IN ('project_viewer', 'project_operator', 'project_admin')),
    created_by text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, project_id, principal_kind, principal_name)
);

CREATE INDEX project_memberships_principal_idx
    ON project_memberships (tenant_id, principal_kind, principal_name);

CREATE INDEX project_memberships_project_idx
    ON project_memberships (tenant_id, project_id, role);
