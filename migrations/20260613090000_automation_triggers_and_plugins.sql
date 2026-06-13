ALTER TABLE automations
    ADD COLUMN condition_tree JSONB NOT NULL DEFAULT '{}'::jsonb;

CREATE TABLE custom_trigger_plugins (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    runtime_image TEXT NOT NULL,
    command TEXT[] NOT NULL,
    config_schema JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE automation_triggers (
    id TEXT PRIMARY KEY,
    automation_id TEXT NOT NULL REFERENCES automations(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('manual', 'schedule', 'sql', 'custom')),
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    plugin_id TEXT REFERENCES custom_trigger_plugins(id),
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (automation_id, name)
);

CREATE INDEX automation_triggers_automation_id_idx
    ON automation_triggers(automation_id);
