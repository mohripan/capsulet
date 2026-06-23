ALTER TABLE workflow_runs
ADD COLUMN IF NOT EXISTS workflow_snapshot JSONB;
