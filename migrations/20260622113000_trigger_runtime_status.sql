CREATE TABLE trigger_runtime_status (
    automation_id text NOT NULL,
    trigger_name text NOT NULL,
    last_error text,
    consecutive_failures integer NOT NULL DEFAULT 0,
    last_success_at timestamptz,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (automation_id, trigger_name),
    FOREIGN KEY (automation_id, trigger_name)
        REFERENCES automation_triggers(automation_id, name) ON DELETE CASCADE
);
