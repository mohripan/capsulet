ALTER TABLE job_runs
    ADD COLUMN heartbeat_at TIMESTAMPTZ;

CREATE INDEX job_runs_heartbeat_at_idx
    ON job_runs (heartbeat_at)
    WHERE status IN ('leased', 'running');
