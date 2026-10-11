-- Historical writers omitted scope and inherited the default project. Retain those
-- records, but do not present their guessed ownership as verified project data.
ALTER TABLE audit_events ADD COLUMN scope_verified boolean NOT NULL DEFAULT false;

CREATE INDEX audit_events_verified_scope_created_idx
    ON audit_events (tenant_id, project_id, created_at DESC, id DESC)
    WHERE scope_verified;
