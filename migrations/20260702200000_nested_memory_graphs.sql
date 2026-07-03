CREATE TABLE memory_subgraphs (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    parent_subgraph_id TEXT REFERENCES memory_subgraphs(id) ON DELETE RESTRICT,
    name TEXT NOT NULL,
    description TEXT,
    owner_kind TEXT,
    owner_id TEXT,
    contract_id TEXT REFERENCES memory_contracts(id) ON DELETE RESTRICT,
    summary_claim_id TEXT REFERENCES memory_claims(id) ON DELETE RESTRICT,
    permissions JSONB,
    status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (status IN ('draft', 'active', 'archived')),
    CHECK (
        status <> 'active'
        OR (
            owner_kind IS NOT NULL
            AND owner_id IS NOT NULL
            AND contract_id IS NOT NULL
            AND summary_claim_id IS NOT NULL
            AND permissions IS NOT NULL
        )
    )
);

CREATE TABLE memory_subgraph_members (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    subgraph_id TEXT NOT NULL REFERENCES memory_subgraphs(id) ON DELETE CASCADE,
    member_kind TEXT NOT NULL,
    member_id TEXT NOT NULL,
    role TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (subgraph_id, member_kind, member_id, role)
);

CREATE TABLE memory_canonical_entities (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    display_name TEXT NOT NULL,
    aliases TEXT[] NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE memory_entity_resolutions (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    subgraph_id TEXT NOT NULL REFERENCES memory_subgraphs(id) ON DELETE CASCADE,
    entity_id TEXT NOT NULL REFERENCES memory_entities(id) ON DELETE RESTRICT,
    canonical_entity_id TEXT NOT NULL REFERENCES memory_canonical_entities(id) ON DELETE RESTRICT,
    confidence DOUBLE PRECISION NOT NULL,
    status TEXT NOT NULL,
    evidence_ids TEXT[] NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (confidence >= 0.0 AND confidence <= 1.0),
    CHECK (status IN ('candidate', 'confirmed', 'rejected')),
    CHECK (status <> 'confirmed' OR array_length(evidence_ids, 1) IS NOT NULL)
);

CREATE UNIQUE INDEX memory_entity_resolutions_confirmed_unique
    ON memory_entity_resolutions(subgraph_id, entity_id)
    WHERE status = 'confirmed';

CREATE TABLE memory_subgraph_edges (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    edge_type TEXT NOT NULL,
    from_subgraph_id TEXT NOT NULL REFERENCES memory_subgraphs(id) ON DELETE CASCADE,
    to_subgraph_id TEXT NOT NULL REFERENCES memory_subgraphs(id) ON DELETE CASCADE,
    from_member_kind TEXT NOT NULL,
    from_member_id TEXT NOT NULL,
    to_member_kind TEXT NOT NULL,
    to_member_id TEXT NOT NULL,
    claim_ids TEXT[] NOT NULL DEFAULT '{}',
    evidence_ids TEXT[] NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (from_subgraph_id <> to_subgraph_id),
    CHECK (array_length(claim_ids, 1) IS NOT NULL OR array_length(evidence_ids, 1) IS NOT NULL)
);

CREATE TABLE memory_summary_traces (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    subgraph_id TEXT NOT NULL REFERENCES memory_subgraphs(id) ON DELETE CASCADE,
    summary_claim_id TEXT NOT NULL REFERENCES memory_claims(id) ON DELETE RESTRICT,
    inner_claim_ids TEXT[] NOT NULL DEFAULT '{}',
    evidence_ids TEXT[] NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (array_length(inner_claim_ids, 1) IS NOT NULL OR array_length(evidence_ids, 1) IS NOT NULL)
);

CREATE TABLE memory_entity_graph_attachments (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    canonical_entity_id TEXT NOT NULL REFERENCES memory_canonical_entities(id) ON DELETE CASCADE,
    subgraph_id TEXT NOT NULL REFERENCES memory_subgraphs(id) ON DELETE CASCADE,
    attachment_type TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (attachment_type IN ('primary', 'supporting', 'historical'))
);

CREATE UNIQUE INDEX memory_entity_graph_attachments_primary_unique
    ON memory_entity_graph_attachments(tenant_id, project_id, canonical_entity_id)
    WHERE attachment_type = 'primary';

CREATE INDEX memory_subgraphs_scope_idx ON memory_subgraphs(tenant_id, project_id, updated_at DESC);
CREATE INDEX memory_subgraph_members_subgraph_idx ON memory_subgraph_members(subgraph_id, role);
CREATE INDEX memory_canonical_entities_scope_idx ON memory_canonical_entities(tenant_id, project_id, updated_at DESC);
CREATE INDEX memory_entity_resolutions_entity_idx ON memory_entity_resolutions(entity_id, status);
CREATE INDEX memory_subgraph_edges_scope_idx ON memory_subgraph_edges(tenant_id, project_id, updated_at DESC);
CREATE INDEX memory_summary_traces_subgraph_idx ON memory_summary_traces(subgraph_id, summary_claim_id);
