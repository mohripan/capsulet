CREATE TABLE workflow_definitions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL CHECK (status IN ('draft', 'enabled', 'disabled')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE workflow_steps (
    id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL REFERENCES workflow_definitions(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position > 0),
    name TEXT NOT NULL,
    job_definition_id TEXT NOT NULL REFERENCES job_definitions(id),
    execution_pool TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (workflow_id, position)
);

CREATE TABLE automations (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    workflow_id TEXT NOT NULL REFERENCES workflow_definitions(id),
    status TEXT NOT NULL CHECK (status IN ('enabled', 'disabled')),
    trigger_kind TEXT NOT NULL CHECK (trigger_kind IN ('manual', 'interval')),
    interval_seconds INTEGER CHECK (interval_seconds IS NULL OR interval_seconds > 0),
    next_fire_at TIMESTAMPTZ,
    last_triggered_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX automations_due_idx
    ON automations(next_fire_at)
    WHERE status = 'enabled' AND trigger_kind = 'interval';

CREATE TABLE workflow_runs (
    id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL REFERENCES workflow_definitions(id),
    automation_id TEXT REFERENCES automations(id),
    status TEXT NOT NULL CHECK (
        status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled', 'timed_out')
    ),
    current_step_position INTEGER NOT NULL DEFAULT 0 CHECK (current_step_position >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ
);

CREATE INDEX workflow_runs_status_created_at_idx ON workflow_runs(status, created_at);

CREATE TABLE workflow_step_runs (
    id TEXT PRIMARY KEY,
    workflow_run_id TEXT NOT NULL REFERENCES workflow_runs(id) ON DELETE CASCADE,
    workflow_step_id TEXT NOT NULL REFERENCES workflow_steps(id),
    job_run_id TEXT NOT NULL REFERENCES job_runs(id),
    position INTEGER NOT NULL CHECK (position > 0),
    status TEXT NOT NULL CHECK (
        status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled', 'timed_out')
    ),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (workflow_run_id, position)
);
