CREATE TABLE memory_claim_conflicts (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    subject_id TEXT NOT NULL REFERENCES memory_entities(id) ON DELETE RESTRICT,
    canonical_entity_id TEXT REFERENCES memory_canonical_entities(id) ON DELETE RESTRICT,
    predicate TEXT NOT NULL,
    claim_ids TEXT[] NOT NULL,
    status TEXT NOT NULL,
    reason TEXT NOT NULL,
    preferred_claim_id TEXT REFERENCES memory_claims(id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (array_length(claim_ids, 1) >= 2),
    CHECK (status IN ('candidate', 'resolved', 'dismissed')),
    CHECK (status <> 'resolved' OR preferred_claim_id IS NOT NULL)
);

CREATE INDEX memory_claim_conflicts_scope_idx
    ON memory_claim_conflicts(tenant_id, project_id, status, updated_at DESC);

CREATE UNIQUE INDEX memory_claim_conflicts_candidate_unique
    ON memory_claim_conflicts(tenant_id, project_id, subject_id, predicate, claim_ids)
    WHERE status = 'candidate';
