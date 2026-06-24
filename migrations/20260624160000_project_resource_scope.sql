ALTER TABLE custom_trigger_plugins ADD COLUMN tenant_id text NOT NULL DEFAULT 'default';
ALTER TABLE custom_trigger_plugins ADD COLUMN project_id text NOT NULL DEFAULT 'default';

CREATE INDEX custom_trigger_plugins_tenant_project_updated_idx
    ON custom_trigger_plugins (tenant_id, project_id, updated_at DESC);

CREATE INDEX project_memberships_tenant_project_principal_idx
    ON project_memberships (tenant_id, project_id, principal_kind, principal_name);
