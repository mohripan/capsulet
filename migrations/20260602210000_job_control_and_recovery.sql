ALTER TABLE job_definitions
    ADD COLUMN retry_max_attempts INTEGER NOT NULL DEFAULT 1 CHECK (retry_max_attempts > 0),
    ADD COLUMN retry_delay_seconds INTEGER NOT NULL DEFAULT 0 CHECK (retry_delay_seconds >= 0);

ALTER TABLE job_runs
    ADD COLUMN retry_ready_at TIMESTAMPTZ;

CREATE INDEX job_runs_retry_ready_at_idx
    ON job_runs(retry_ready_at)
    WHERE status = 'retry_scheduled';
