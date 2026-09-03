-- Verified-computation IR definitions and platform certificates.
--
-- Everything here is append-only, and it is append-only in the database rather
-- than only in Rust. A certificate whose contents can be edited afterwards
-- proves nothing: the whole point is that the record of what was checked
-- survives the wish that it had said something else. Triggers below raise on
-- UPDATE and DELETE, so a mistaken migration, a console session, or a future
-- repository method cannot quietly rewrite history.
--
-- Triggers rather than rules, for two reasons. A `DO INSTEAD NOTHING` rule
-- makes a mutation silently succeed-as-no-op, which tells a caller nothing;
-- raising says plainly that the write was refused. And PostgreSQL refuses
-- `INSERT ... ON CONFLICT` on any table carrying an INSERT or UPDATE rule,
-- which would have cost us the idempotent registration that content addressing
-- exists to give.
--
-- Definitions are content-addressed. Two authors who write the same definition
-- produce the same canonical bytes and therefore the same digest, so
-- registering it twice is idempotent rather than a conflict.

CREATE TABLE ir_definitions (
    id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Ownership is part of identity, not a filter applied later: the same
    -- definition id in two projects is two definitions.
    PRIMARY KEY (tenant_id, project_id, id)
);

CREATE TABLE ir_definition_versions (
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    definition_id TEXT NOT NULL,
    -- The digest of the canonical bytes. Identity, not metadata.
    digest TEXT NOT NULL,
    version TEXT NOT NULL,
    schema_version TEXT NOT NULL,
    -- The exact bytes that were digested, so a reader never has to re-encode
    -- and hope the encoder has not changed.
    canonical_bytes TEXT NOT NULL,
    -- The admission record, proving this version passed structural admission
    -- before it was stored at all.
    admission JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, project_id, digest),
    FOREIGN KEY (tenant_id, project_id, definition_id)
        REFERENCES ir_definitions(tenant_id, project_id, id) ON DELETE RESTRICT
);

CREATE INDEX ir_definition_versions_by_definition_idx
    ON ir_definition_versions(tenant_id, project_id, definition_id, created_at DESC);

-- Evidence metadata. The bytes live in object storage under the digest, so a
-- large log does not turn the metadata store into a blob store.
CREATE TABLE assurance_evidence (
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    digest TEXT NOT NULL,
    media_type TEXT NOT NULL,
    byte_length BIGINT NOT NULL,
    object_key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, project_id, digest),
    CONSTRAINT assurance_evidence_length_is_sane CHECK (byte_length >= 0)
);

CREATE TABLE assurance_certificates (
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    id TEXT NOT NULL,
    -- The definition this certificate is about, by digest.
    definition_digest TEXT NOT NULL,
    schema_version TEXT NOT NULL,
    -- Both dimensions, stored separately, because execution status and
    -- assurance verdict are independent and collapsing them here would push the
    -- confusion into every reader.
    verdict TEXT NOT NULL,
    mode TEXT NOT NULL,
    policy_version TEXT NOT NULL,
    kernel_version TEXT NOT NULL,
    replay_digest TEXT NOT NULL,
    canonical_bytes TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, project_id, id),
    CONSTRAINT assurance_certificates_verdict_is_known
        CHECK (verdict IN ('unverified', 'accepted', 'conditional', 'rejected')),
    CONSTRAINT assurance_certificates_mode_is_known
        CHECK (mode IN ('observe', 'verify', 'enforce'))
);

CREATE INDEX assurance_certificates_scope_idx
    ON assurance_certificates(tenant_id, project_id, created_at DESC);

CREATE INDEX assurance_certificates_verdict_idx
    ON assurance_certificates(tenant_id, project_id, verdict, created_at DESC);

-- Obligations are projected out of the certificate for querying. The
-- certificate's own bytes remain the authority; this table exists so "what is
-- still outstanding, and how old is it" is a query rather than a scan.
CREATE TABLE assurance_obligations (
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    certificate_id TEXT NOT NULL,
    obligation_id TEXT NOT NULL,
    contract TEXT NOT NULL,
    state TEXT NOT NULL,
    owner TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, project_id, certificate_id, obligation_id),
    FOREIGN KEY (tenant_id, project_id, certificate_id)
        REFERENCES assurance_certificates(tenant_id, project_id, id) ON DELETE RESTRICT,
    CONSTRAINT assurance_obligations_state_is_known
        CHECK (state IN ('discharged', 'assumed', 'waived', 'residual', 'failed'))
);

CREATE INDEX assurance_obligations_outstanding_idx
    ON assurance_obligations(tenant_id, project_id, state, created_at);

-- Append-only, enforced here rather than trusted to callers.
--
-- A row-level BEFORE trigger, so a statement that matches nothing is not an
-- error while a statement that would actually change a stored record is.
CREATE FUNCTION capsulet_refuse_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'relation % is append-only; % was refused', TG_TABLE_NAME, TG_OP
        USING ERRCODE = 'restrict_violation';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER ir_definitions_are_append_only
    BEFORE UPDATE OR DELETE ON ir_definitions
    FOR EACH ROW EXECUTE FUNCTION capsulet_refuse_mutation();

CREATE TRIGGER ir_definition_versions_are_append_only
    BEFORE UPDATE OR DELETE ON ir_definition_versions
    FOR EACH ROW EXECUTE FUNCTION capsulet_refuse_mutation();

CREATE TRIGGER assurance_evidence_is_append_only
    BEFORE UPDATE OR DELETE ON assurance_evidence
    FOR EACH ROW EXECUTE FUNCTION capsulet_refuse_mutation();

CREATE TRIGGER assurance_certificates_are_append_only
    BEFORE UPDATE OR DELETE ON assurance_certificates
    FOR EACH ROW EXECUTE FUNCTION capsulet_refuse_mutation();

CREATE TRIGGER assurance_obligations_are_append_only
    BEFORE UPDATE OR DELETE ON assurance_obligations
    FOR EACH ROW EXECUTE FUNCTION capsulet_refuse_mutation();
