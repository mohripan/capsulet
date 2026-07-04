CREATE TABLE ingestion_connectors (
    id text PRIMARY KEY,
    tenant_id text NOT NULL,
    project_id text NOT NULL,
    name text NOT NULL,
    kind text NOT NULL,
    config jsonb NOT NULL,
    enabled boolean NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX ingestion_connectors_scope_idx
    ON ingestion_connectors (tenant_id, project_id, updated_at DESC);

CREATE TABLE ingestion_runs (
    id text PRIMARY KEY,
    tenant_id text NOT NULL,
    project_id text NOT NULL,
    connector_id text NOT NULL REFERENCES ingestion_connectors(id) ON DELETE CASCADE,
    status text NOT NULL,
    error text,
    source_count integer NOT NULL DEFAULT 0,
    evidence_count integer NOT NULL DEFAULT 0,
    entity_count integer NOT NULL DEFAULT 0,
    claim_count integer NOT NULL DEFAULT 0,
    event_count integer NOT NULL DEFAULT 0,
    relationship_count integer NOT NULL DEFAULT 0,
    started_at timestamptz NOT NULL DEFAULT now(),
    finished_at timestamptz,
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX ingestion_runs_scope_idx
    ON ingestion_runs (tenant_id, project_id, started_at DESC);

CREATE INDEX ingestion_runs_connector_idx
    ON ingestion_runs (connector_id, started_at DESC);

CREATE TABLE ingestion_run_outputs (
    run_id text NOT NULL REFERENCES ingestion_runs(id) ON DELETE CASCADE,
    kind text NOT NULL,
    memory_id text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (run_id, kind, memory_id)
);
