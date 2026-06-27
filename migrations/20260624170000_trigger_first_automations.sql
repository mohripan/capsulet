DROP INDEX IF EXISTS automations_due_idx;

ALTER TABLE automations
    DROP COLUMN IF EXISTS trigger_kind,
    DROP COLUMN IF EXISTS interval_seconds;

CREATE INDEX automations_due_idx
    ON automations(next_fire_at)
    WHERE status = 'enabled' AND next_fire_at IS NOT NULL;
