CREATE TABLE job_artifacts (
    id TEXT PRIMARY KEY,
    job_run_id TEXT NOT NULL REFERENCES job_runs(id) ON DELETE CASCADE,
    job_attempt_id TEXT,
    name TEXT NOT NULL,
    object_key TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL CHECK (size_bytes >= 0),
    checksum_sha256 TEXT,
    kind TEXT NOT NULL CHECK (kind IN ('bundle', 'log', 'artifact')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (job_run_id, name, kind)
);

CREATE INDEX job_artifacts_job_run_id_idx
    ON job_artifacts(job_run_id);
