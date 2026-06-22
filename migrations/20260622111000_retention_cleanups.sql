CREATE TABLE retention_cleanups (
    job_run_id text PRIMARY KEY REFERENCES job_runs(id) ON DELETE CASCADE,
    cleaned_at timestamptz NOT NULL DEFAULT now()
);
