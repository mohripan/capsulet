-- Domain packs a project installed (ADR 0022, #158).
--
-- One row per pack version, holding the whole pack as it arrived, so a
-- definition pinning it and a replay re-reading it see exactly these bytes.
-- Append-only: a version does not change, and an installed pack is part of the
-- record of every certificate decided under it.
CREATE TABLE domain_packs (
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    manifest_digest TEXT NOT NULL,
    signed_by TEXT NOT NULL,
    bundle JSONB NOT NULL,
    installed_by TEXT NOT NULL,
    installed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, project_id, name, version)
);

CREATE INDEX domain_packs_manifest_idx
    ON domain_packs(tenant_id, project_id, manifest_digest);

CREATE TRIGGER domain_packs_are_append_only
    BEFORE UPDATE OR DELETE ON domain_packs
    FOR EACH ROW EXECUTE FUNCTION capsulet_refuse_mutation();
