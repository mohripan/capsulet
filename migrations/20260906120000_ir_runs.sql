-- Durable runs of verified-computation IR definitions.
--
-- A run's history is its events, and nothing else. Status, progress, budgets
-- spent, and outstanding effect claims are a fold over `ir_run_events`; they
-- are never a field somebody updated. That matters because recovery after a
-- crash has to reconstruct exactly what the dead worker knew, and the only way
-- to be sure it does is for both to compute it the same way from the same
-- source.
--
-- `ir_run_events` is therefore append-only in the database: UPDATE and DELETE
-- raise. `ir_runs` cannot be, because a lease is mutable by definition, so it
-- is append-only where it counts — the columns that give a run its identity
-- are frozen after insert, and a DELETE raises. What remains writable is the
-- lease, the epoch, the position counter, and a status projection whose
-- authority is still the log.
--
-- The status column is a cache. It exists so "which runs are stuck" is a query
-- rather than a scan of every log, and it is maintained by a trigger from the
-- event kind alone, mirroring `RunState::fold`. A test folds the log and
-- asserts the two agree; if the Rust ever transitions differently, that test
-- fails rather than the cache quietly diverging.

CREATE TABLE ir_runs (
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    id TEXT NOT NULL,
    -- The exact version being executed. A run can always point at the bytes it
    -- ran, which is what makes its certificate checkable later.
    definition_digest TEXT NOT NULL,
    -- Projected from the log by trigger. See the note above.
    status TEXT NOT NULL DEFAULT 'queued',
    -- The lease generation. Bumped on every new lease; every event names the
    -- epoch it was written under and one from a superseded lease is refused.
    epoch BIGINT NOT NULL DEFAULT 0,
    lease_owner TEXT,
    lease_expires_at TIMESTAMPTZ,
    heartbeat_at TIMESTAMPTZ,
    -- The position the next event will take. Held here so an append can take
    -- it under this row's lock, which is what stops two workers from both
    -- writing position n.
    next_position BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, project_id, id),
    FOREIGN KEY (tenant_id, project_id, definition_digest)
        REFERENCES ir_definition_versions(tenant_id, project_id, digest) ON DELETE RESTRICT,
    CONSTRAINT ir_runs_status_is_known CHECK (
        status IN ('queued', 'running', 'waiting', 'completed', 'failed', 'cancelled')
    ),
    CONSTRAINT ir_runs_epoch_is_sane CHECK (epoch >= 0),
    CONSTRAINT ir_runs_position_is_sane CHECK (next_position >= 0)
);

-- Leasing scans this: unfinished runs, oldest first.
CREATE INDEX ir_runs_leasable_idx
    ON ir_runs(status, lease_expires_at, created_at)
    WHERE status IN ('queued', 'running', 'waiting');

CREATE INDEX ir_runs_by_definition_idx
    ON ir_runs(tenant_id, project_id, definition_digest, created_at DESC);

CREATE TABLE ir_run_events (
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    -- Gapless from zero, taken under the run row's lock.
    position BIGINT NOT NULL,
    -- The event's short name, denormalised so a query can filter without
    -- parsing every payload.
    kind TEXT NOT NULL,
    payload JSONB NOT NULL,
    -- The lease generation that wrote this. Two workers cannot hold the same
    -- epoch, so an event is always attributable.
    epoch BIGINT NOT NULL,
    -- When the worker says it happened, as data. Distinct from `stored_at`,
    -- which is when the database saw it; conflating them would make a replay
    -- depend on how long a write took.
    recorded_at BIGINT NOT NULL,
    stored_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, project_id, run_id, position),
    FOREIGN KEY (tenant_id, project_id, run_id)
        REFERENCES ir_runs(tenant_id, project_id, id) ON DELETE RESTRICT,
    CONSTRAINT ir_run_events_position_is_sane CHECK (position >= 0),
    CONSTRAINT ir_run_events_epoch_is_sane CHECK (epoch >= 0)
);

CREATE INDEX ir_run_events_by_kind_idx
    ON ir_run_events(tenant_id, project_id, kind, stored_at DESC);

-- Refuses any change to what a run *is*, while leaving the lease writable.
--
-- A run that could be repointed at a different definition after the fact would
-- make every certificate it produced meaningless: the bytes it claims to have
-- executed would no longer be the bytes it executed.
CREATE FUNCTION capsulet_ir_runs_identity_is_frozen() RETURNS trigger AS $$
BEGIN
    IF NEW.tenant_id IS DISTINCT FROM OLD.tenant_id
       OR NEW.project_id IS DISTINCT FROM OLD.project_id
       OR NEW.id IS DISTINCT FROM OLD.id
       OR NEW.definition_digest IS DISTINCT FROM OLD.definition_digest
       OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
        RAISE EXCEPTION 'ir_runs identity is frozen after insert; the update was refused'
            USING ERRCODE = 'restrict_violation';
    END IF;
    -- A lease that never moves backwards is what fencing rests on.
    IF NEW.epoch < OLD.epoch THEN
        RAISE EXCEPTION 'ir_runs epoch cannot go backwards (% to %)', OLD.epoch, NEW.epoch
            USING ERRCODE = 'restrict_violation';
    END IF;
    IF NEW.next_position < OLD.next_position THEN
        RAISE EXCEPTION 'ir_runs log cannot be rewound (% to %)', OLD.next_position, NEW.next_position
            USING ERRCODE = 'restrict_violation';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER ir_runs_identity_is_frozen
    BEFORE UPDATE ON ir_runs
    FOR EACH ROW EXECUTE FUNCTION capsulet_ir_runs_identity_is_frozen();

CREATE TRIGGER ir_runs_are_not_deletable
    BEFORE DELETE ON ir_runs
    FOR EACH ROW EXECUTE FUNCTION capsulet_refuse_mutation();

CREATE TRIGGER ir_run_events_are_append_only
    BEFORE UPDATE OR DELETE ON ir_run_events
    FOR EACH ROW EXECUTE FUNCTION capsulet_refuse_mutation();

-- Advances the run's cached status from the event just appended.
--
-- The mapping mirrors `RunState::fold` exactly, and only these kinds move the
-- status; every other kind leaves it where it was.
CREATE FUNCTION capsulet_ir_run_project_status() RETURNS trigger AS $$
DECLARE
    projected TEXT;
BEGIN
    projected := CASE NEW.kind
        WHEN 'started' THEN 'running'
        WHEN 'node_started' THEN 'running'
        WHEN 'resumed' THEN 'running'
        WHEN 'suspended' THEN 'waiting'
        WHEN 'completed' THEN 'completed'
        WHEN 'failed' THEN 'failed'
        WHEN 'cancelled' THEN 'cancelled'
        ELSE NULL
    END;

    IF projected IS NOT NULL THEN
        UPDATE ir_runs
        SET status = projected, updated_at = now()
        WHERE tenant_id = NEW.tenant_id
          AND project_id = NEW.project_id
          AND id = NEW.run_id;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER ir_run_events_project_status
    AFTER INSERT ON ir_run_events
    FOR EACH ROW EXECUTE FUNCTION capsulet_ir_run_project_status();
