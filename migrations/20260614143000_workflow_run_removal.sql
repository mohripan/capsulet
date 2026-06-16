ALTER TABLE workflow_runs
    DROP CONSTRAINT IF EXISTS workflow_runs_status_check;

ALTER TABLE workflow_runs
    ADD CONSTRAINT workflow_runs_status_check
    CHECK (status IN ('queued', 'running', 'removed', 'succeeded', 'failed', 'cancelled', 'timed_out'));
