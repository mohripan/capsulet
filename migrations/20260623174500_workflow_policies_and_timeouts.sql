ALTER TABLE workflow_step_dependencies
ADD COLUMN IF NOT EXISTS policy TEXT NOT NULL DEFAULT 'hard'
CHECK (policy IN ('hard', 'soft', 'always'));

ALTER TABLE workflow_steps
ADD COLUMN IF NOT EXISTS timeout_seconds BIGINT
CHECK (timeout_seconds IS NULL OR timeout_seconds > 0);

ALTER TABLE workflow_definitions
ADD COLUMN IF NOT EXISTS deadline_seconds BIGINT
CHECK (deadline_seconds IS NULL OR deadline_seconds > 0);

ALTER TABLE workflow_runs
ADD COLUMN IF NOT EXISTS deadline_at TIMESTAMPTZ;

ALTER TABLE job_runs
ADD COLUMN IF NOT EXISTS timeout_seconds BIGINT
CHECK (timeout_seconds IS NULL OR timeout_seconds > 0);

ALTER TABLE workflow_step_runs
ALTER COLUMN job_run_id DROP NOT NULL;

ALTER TABLE workflow_step_runs
DROP CONSTRAINT IF EXISTS workflow_step_runs_workflow_step_id_fkey;

ALTER TABLE workflow_step_runs
DROP CONSTRAINT IF EXISTS workflow_step_runs_status_check;

ALTER TABLE workflow_step_runs
ADD CONSTRAINT workflow_step_runs_status_check
CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled', 'timed_out', 'skipped'));

ALTER TABLE workflow_runs
DROP CONSTRAINT IF EXISTS workflow_runs_status_check;

ALTER TABLE workflow_runs
ADD CONSTRAINT workflow_runs_status_check
CHECK (status IN ('queued', 'running', 'removed', 'succeeded', 'failed', 'cancelled', 'timed_out'));
