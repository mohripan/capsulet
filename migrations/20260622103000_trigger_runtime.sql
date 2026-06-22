ALTER TABLE automation_triggers DROP CONSTRAINT automation_triggers_kind_check;
ALTER TABLE automation_triggers ADD CONSTRAINT automation_triggers_kind_check
    CHECK (kind IN ('manual', 'schedule', 'sql', 'webhook', 'custom'));

CREATE TABLE trigger_schedule_cursors (
    automation_id TEXT NOT NULL REFERENCES automations(id) ON DELETE CASCADE,
    trigger_name TEXT NOT NULL,
    next_fire_at TIMESTAMPTZ NOT NULL,
    last_fire_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (automation_id, trigger_name)
);
