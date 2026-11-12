-- A person's decision on a reading the kernel could not make (ADR 0021, #156).
--
-- One review per reading, bound to it by a digest over the residual and the
-- certificate's replay digest, and scoped to the certificate's own project by the
-- foreign key rather than by the code above it. Append-only: a decision is
-- something somebody can be held to, and rewriting one would rewrite whose
-- judgement an answer rests on.

ALTER TABLE reasoning_certificates
    ADD CONSTRAINT reasoning_certificates_scope_key UNIQUE (tenant_id, project_id, id);

CREATE TABLE reasoning_reading_reviews (
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    certificate_id TEXT NOT NULL,
    reading TEXT NOT NULL,
    decision TEXT NOT NULL,
    reviewer TEXT NOT NULL,
    authority TEXT NOT NULL,
    rationale TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, project_id, certificate_id, reading),
    FOREIGN KEY (tenant_id, project_id, certificate_id)
        REFERENCES reasoning_certificates(tenant_id, project_id, id) ON DELETE RESTRICT,
    CONSTRAINT reasoning_reading_reviews_decision_is_known
        CHECK (decision IN ('holds', 'does_not_hold')),
    CONSTRAINT reasoning_reading_reviews_say_why
        CHECK (btrim(rationale) <> '')
);

CREATE TRIGGER reasoning_reading_reviews_are_append_only
    BEFORE UPDATE OR DELETE ON reasoning_reading_reviews
    FOR EACH ROW EXECUTE FUNCTION capsulet_refuse_mutation();
