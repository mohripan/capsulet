-- Certificates: the kernel's decision on one proposal.
--
-- Stored so that an answer can be audited after the fact: what was asked, which
-- pinned evidence set it was answered from, what the kernel discharged, and
-- which readings it refused to make. The replay digest ties a certificate to
-- the exact proposal that produced it.

CREATE TABLE reasoning_certificates (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    question TEXT NOT NULL,
    verdict TEXT NOT NULL,
    goal JSONB NOT NULL,
    discharged JSONB NOT NULL DEFAULT '[]'::jsonb,
    residuals JSONB NOT NULL DEFAULT '[]'::jsonb,
    errors JSONB NOT NULL DEFAULT '[]'::jsonb,
    -- The alphabet the proposal was generated against, so a later reviewer can
    -- tell whether a rejection was the model's fault or retrieval's.
    alphabet_digest TEXT NOT NULL,
    replay_digest TEXT NOT NULL,
    model TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT reasoning_certificates_verdict_is_known
        CHECK (verdict IN ('accepted', 'conditional', 'rejected'))
);

CREATE INDEX reasoning_certificates_scope_idx
    ON reasoning_certificates(tenant_id, project_id, created_at DESC);

CREATE INDEX reasoning_certificates_verdict_idx
    ON reasoning_certificates(tenant_id, project_id, verdict, created_at DESC);
