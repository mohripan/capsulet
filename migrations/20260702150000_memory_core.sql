CREATE TABLE memory_sources (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    uri TEXT,
    title TEXT NOT NULL,
    authority TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE memory_evidence (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    source_id TEXT NOT NULL REFERENCES memory_sources(id) ON DELETE RESTRICT,
    locator TEXT NOT NULL,
    excerpt TEXT NOT NULL,
    observed_at TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE memory_entities (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    name TEXT NOT NULL,
    aliases TEXT[] NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE memory_claims (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    subject_id TEXT NOT NULL REFERENCES memory_entities(id) ON DELETE RESTRICT,
    predicate TEXT NOT NULL,
    object TEXT NOT NULL,
    evidence_ids TEXT[] NOT NULL,
    confidence DOUBLE PRECISION NOT NULL,
    authority TEXT NOT NULL,
    status TEXT NOT NULL,
    observed_at TEXT NOT NULL,
    valid_from TEXT,
    valid_until TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (confidence >= 0.0 AND confidence <= 1.0),
    CHECK (array_length(evidence_ids, 1) IS NOT NULL)
);

CREATE TABLE memory_events (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    occurred_at TEXT NOT NULL,
    entity_ids TEXT[] NOT NULL DEFAULT '{}',
    evidence_ids TEXT[] NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (array_length(evidence_ids, 1) IS NOT NULL)
);

CREATE TABLE memory_relationships (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    relationship_type TEXT NOT NULL,
    from_entity_id TEXT NOT NULL REFERENCES memory_entities(id) ON DELETE RESTRICT,
    to_entity_id TEXT NOT NULL REFERENCES memory_entities(id) ON DELETE RESTRICT,
    evidence_ids TEXT[] NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (array_length(evidence_ids, 1) IS NOT NULL)
);

CREATE INDEX memory_sources_scope_idx ON memory_sources(tenant_id, project_id, updated_at DESC);
CREATE INDEX memory_evidence_scope_idx ON memory_evidence(tenant_id, project_id, updated_at DESC);
CREATE INDEX memory_entities_scope_idx ON memory_entities(tenant_id, project_id, updated_at DESC);
CREATE INDEX memory_claims_scope_idx ON memory_claims(tenant_id, project_id, updated_at DESC);
CREATE INDEX memory_claims_subject_predicate_idx ON memory_claims(subject_id, predicate, status);
CREATE INDEX memory_events_scope_idx ON memory_events(tenant_id, project_id, updated_at DESC);
CREATE INDEX memory_relationships_scope_idx ON memory_relationships(tenant_id, project_id, updated_at DESC);
