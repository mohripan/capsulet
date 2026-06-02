CREATE TABLE job_run_logs (
    job_run_id TEXT PRIMARY KEY REFERENCES job_runs(id) ON DELETE CASCADE,
    log_text TEXT NOT NULL,
    captured_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
