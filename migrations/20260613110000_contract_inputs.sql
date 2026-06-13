ALTER TABLE automations
    ADD COLUMN IF NOT EXISTS job_input JSONB NOT NULL DEFAULT '{}'::jsonb;

ALTER TABLE workflow_runs
    ADD COLUMN IF NOT EXISTS input JSONB NOT NULL DEFAULT '{}'::jsonb;
