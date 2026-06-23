CREATE TABLE tenants (
    id text PRIMARY KEY,
    name text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE projects (
    id text PRIMARY KEY,
    tenant_id text NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, id)
);

INSERT INTO tenants (id, name)
VALUES ('default', 'Default Tenant')
ON CONFLICT (id) DO NOTHING;

INSERT INTO projects (id, tenant_id, name)
VALUES ('default', 'default', 'Default Project')
ON CONFLICT (id) DO NOTHING;

ALTER TABLE job_definitions ADD COLUMN tenant_id text NOT NULL DEFAULT 'default';
ALTER TABLE job_definitions ADD COLUMN project_id text NOT NULL DEFAULT 'default';
ALTER TABLE workflow_definitions ADD COLUMN tenant_id text NOT NULL DEFAULT 'default';
ALTER TABLE workflow_definitions ADD COLUMN project_id text NOT NULL DEFAULT 'default';
ALTER TABLE job_runs ADD COLUMN tenant_id text NOT NULL DEFAULT 'default';
ALTER TABLE job_runs ADD COLUMN project_id text NOT NULL DEFAULT 'default';
ALTER TABLE workflow_runs ADD COLUMN tenant_id text NOT NULL DEFAULT 'default';
ALTER TABLE workflow_runs ADD COLUMN project_id text NOT NULL DEFAULT 'default';
ALTER TABLE automations ADD COLUMN tenant_id text NOT NULL DEFAULT 'default';
ALTER TABLE automations ADD COLUMN project_id text NOT NULL DEFAULT 'default';
ALTER TABLE audit_events ADD COLUMN tenant_id text NOT NULL DEFAULT 'default';
ALTER TABLE audit_events ADD COLUMN project_id text NOT NULL DEFAULT 'default';

CREATE INDEX job_definitions_tenant_project_updated_idx ON job_definitions (tenant_id, project_id, updated_at DESC);
CREATE INDEX workflow_definitions_tenant_project_updated_idx ON workflow_definitions (tenant_id, project_id, updated_at DESC);
CREATE INDEX job_runs_tenant_project_created_idx ON job_runs (tenant_id, project_id, created_at DESC);
CREATE INDEX workflow_runs_tenant_project_created_idx ON workflow_runs (tenant_id, project_id, created_at DESC);
CREATE INDEX automations_tenant_project_updated_idx ON automations (tenant_id, project_id, updated_at DESC);
CREATE INDEX audit_events_tenant_project_created_idx ON audit_events (tenant_id, project_id, created_at DESC);

CREATE TABLE service_accounts (
    id text PRIMARY KEY,
    name text NOT NULL,
    tenant_id text NOT NULL DEFAULT 'default' REFERENCES tenants(id),
    project_id text NOT NULL DEFAULT 'default',
    role text NOT NULL CHECK (role IN ('viewer', 'operator', 'admin')),
    scopes text[] NOT NULL,
    token_hash bytea NOT NULL UNIQUE,
    expires_at timestamptz,
    revoked_at timestamptz,
    last_used_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX service_accounts_tenant_project_created_idx ON service_accounts (tenant_id, project_id, created_at DESC);
CREATE INDEX service_accounts_revoked_expires_idx ON service_accounts (revoked_at, expires_at);
