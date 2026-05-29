CREATE TABLE job_definitions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    runtime_image TEXT NOT NULL,
    command TEXT[] NOT NULL,
    bundle_object_key TEXT NOT NULL,
    input_schema JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE job_runs (
    id TEXT PRIMARY KEY,
    job_definition_id TEXT NOT NULL REFERENCES job_definitions(id),
    status TEXT NOT NULL CHECK (
        status IN (
            'queued',
            'leased',
            'running',
            'succeeded',
            'failed',
            'cancelled',
            'timed_out',
            'retry_scheduled'
        )
    ),
    execution_pool TEXT NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    lease_owner TEXT,
    lease_expires_at TIMESTAMPTZ,
    input JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX job_runs_status_created_at_idx ON job_runs(status, created_at);
CREATE INDEX job_runs_job_definition_id_idx ON job_runs(job_definition_id);
CREATE INDEX job_runs_lease_expires_at_idx ON job_runs(lease_expires_at) WHERE lease_expires_at IS NOT NULL;

CREATE TABLE job_attempts (
    id TEXT PRIMARY KEY,
    job_run_id TEXT NOT NULL REFERENCES job_runs(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK (
        status IN (
            'leased',
            'running',
            'succeeded',
            'failed',
            'cancelled',
            'timed_out'
        )
    ),
    kubernetes_job_name TEXT,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX job_attempts_job_run_id_idx ON job_attempts(job_run_id);
