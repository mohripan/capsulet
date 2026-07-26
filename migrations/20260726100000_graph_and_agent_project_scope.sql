ALTER TABLE graph_definitions ADD COLUMN tenant_id text NOT NULL DEFAULT 'default';
ALTER TABLE graph_definitions ADD COLUMN project_id text NOT NULL DEFAULT 'default';

ALTER TABLE agent_definitions ADD COLUMN tenant_id text NOT NULL DEFAULT 'default';
ALTER TABLE agent_definitions ADD COLUMN project_id text NOT NULL DEFAULT 'default';

ALTER TABLE agent_runs ADD COLUMN tenant_id text NOT NULL DEFAULT 'default';
ALTER TABLE agent_runs ADD COLUMN project_id text NOT NULL DEFAULT 'default';

CREATE INDEX graph_definitions_tenant_project_updated_idx
    ON graph_definitions (tenant_id, project_id, updated_at DESC);

CREATE INDEX agent_definitions_tenant_project_updated_idx
    ON agent_definitions (tenant_id, project_id, updated_at DESC);

CREATE INDEX agent_runs_tenant_project_updated_idx
    ON agent_runs (tenant_id, project_id, updated_at DESC);
