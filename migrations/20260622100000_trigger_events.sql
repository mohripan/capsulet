CREATE TABLE trigger_events (
    id TEXT PRIMARY KEY,
    automation_id TEXT NOT NULL REFERENCES automations(id) ON DELETE CASCADE,
    trigger_name TEXT NOT NULL,
    correlation_key TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'leased', 'evaluated', 'failed')),
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    lease_owner TEXT,
    lease_expires_at TIMESTAMPTZ,
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    last_error TEXT,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (automation_id, trigger_name, idempotency_key)
);

CREATE INDEX trigger_events_claim_idx
    ON trigger_events (status, available_at, occurred_at)
    WHERE status IN ('pending', 'leased');
CREATE INDEX trigger_events_correlation_idx
    ON trigger_events (automation_id, correlation_key, occurred_at);

CREATE TABLE trigger_evaluations (
    automation_id TEXT NOT NULL REFERENCES automations(id) ON DELETE CASCADE,
    correlation_key TEXT NOT NULL,
    workflow_run_id TEXT NOT NULL REFERENCES workflow_runs(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (automation_id, correlation_key)
);
