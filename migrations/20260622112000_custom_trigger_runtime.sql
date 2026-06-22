CREATE TABLE custom_trigger_runtime (
    automation_id text NOT NULL,
    trigger_name text NOT NULL,
    next_poll_at timestamptz NOT NULL DEFAULT now(),
    lease_owner text,
    lease_expires_at timestamptz,
    last_error text,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (automation_id, trigger_name),
    FOREIGN KEY (automation_id, trigger_name)
        REFERENCES automation_triggers(automation_id, name) ON DELETE CASCADE
);

CREATE INDEX custom_trigger_runtime_due_idx
    ON custom_trigger_runtime(next_poll_at)
    WHERE lease_owner IS NULL;
