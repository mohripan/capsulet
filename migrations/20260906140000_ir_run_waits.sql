-- Durable waits.
--
-- A suspended run holds nothing anywhere. No timer thread, no callback
-- registration, no worker keeping a note — all of which a restart loses. What
-- it has instead is two columns and an inbox.
--
-- `wake_at_millis` is when a timer wait becomes due, projected from the
-- suspension event so the leasing query can skip a run that has nothing to do
-- yet. Milliseconds rather than a timestamp because the runtime compares it
-- against a time the caller supplies, and converting through the database's
-- clock would put a second, disagreeing clock in the path of a decision that is
-- supposed to be replayable.
--
-- `ir_run_signals` is the inbox. Everything that arrives from outside a run —
-- a webhook, a person's decision, a scheduler saying a timer elapsed — is
-- written here rather than into the run's log. Only the lease holder writes the
-- log, so there is exactly one writer and no race between an API request and a
-- worker. The worker matches the signal against what the run is actually
-- waiting for, and records where the resumption landed, which is what makes a
-- signal resume a run exactly once.

ALTER TABLE ir_runs ADD COLUMN wake_at_millis BIGINT;

-- Leasing scans this: runs that are due, oldest first.
DROP INDEX ir_runs_leasable_idx;
CREATE INDEX ir_runs_leasable_idx
    ON ir_runs(status, lease_expires_at, wake_at_millis, created_at)
    WHERE status IN ('queued', 'running', 'waiting');

CREATE TABLE ir_run_signals (
    id BIGSERIAL PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    -- 'event', 'human_decision', or 'timer_elapsed'.
    kind TEXT NOT NULL,
    -- The event name, or the obligation a decision answers.
    subject TEXT,
    -- Who delivered it, and what they were entitled to at that moment. Stored
    -- rather than resolved later: whether someone could open a gate is a fact
    -- about when they opened it.
    delivered_by TEXT NOT NULL,
    authorities JSONB NOT NULL DEFAULT '[]'::jsonb,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    delivered_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Where the resumption it caused landed in the run's log. NULL while the
    -- signal is still pending; set once, which is what "exactly once" means.
    consumed_at_position BIGINT,
    consumed_at TIMESTAMPTZ,
    outcome TEXT,
    FOREIGN KEY (tenant_id, project_id, run_id)
        REFERENCES ir_runs(tenant_id, project_id, id) ON DELETE RESTRICT,
    CONSTRAINT ir_run_signals_kind_is_known
        CHECK (kind IN ('event', 'human_decision', 'timer_elapsed')),
    CONSTRAINT ir_run_signals_outcome_is_known
        CHECK (outcome IS NULL OR outcome IN ('resumed', 'refused')),
    -- Consumed means resolved: a position or a reason, never neither.
    CONSTRAINT ir_run_signals_consumption_is_complete
        CHECK ((consumed_at IS NULL) = (outcome IS NULL))
);

CREATE INDEX ir_run_signals_pending_idx
    ON ir_run_signals(tenant_id, project_id, run_id, id)
    WHERE consumed_at IS NULL;

-- A signal is a record of something that happened outside the run, so it is
-- not editable after the fact. Consuming one is the single permitted change,
-- and it is permitted once.
CREATE FUNCTION capsulet_ir_signals_are_consumed_once() RETURNS trigger AS $$
BEGIN
    IF OLD.consumed_at IS NOT NULL THEN
        RAISE EXCEPTION 'signal % was already consumed at position %',
            OLD.id, OLD.consumed_at_position
            USING ERRCODE = 'restrict_violation';
    END IF;
    IF NEW.tenant_id IS DISTINCT FROM OLD.tenant_id
       OR NEW.project_id IS DISTINCT FROM OLD.project_id
       OR NEW.run_id IS DISTINCT FROM OLD.run_id
       OR NEW.kind IS DISTINCT FROM OLD.kind
       OR NEW.subject IS DISTINCT FROM OLD.subject
       OR NEW.delivered_by IS DISTINCT FROM OLD.delivered_by
       OR NEW.authorities IS DISTINCT FROM OLD.authorities
       OR NEW.payload IS DISTINCT FROM OLD.payload THEN
        RAISE EXCEPTION 'what a signal said is not editable; only consuming it is'
            USING ERRCODE = 'restrict_violation';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER ir_run_signals_are_consumed_once
    BEFORE UPDATE ON ir_run_signals
    FOR EACH ROW EXECUTE FUNCTION capsulet_ir_signals_are_consumed_once();

CREATE TRIGGER ir_run_signals_are_not_deletable
    BEFORE DELETE ON ir_run_signals
    FOR EACH ROW EXECUTE FUNCTION capsulet_refuse_mutation();

-- Projects the wake-up time out of the suspension event.
--
-- A suspension on anything other than a timer clears it: there is no time at
-- which a webhook becomes due.
CREATE OR REPLACE FUNCTION capsulet_ir_run_project_status() RETURNS trigger AS $$
DECLARE
    projected TEXT;
    wake BIGINT;
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

    IF projected IS NULL THEN
        RETURN NEW;
    END IF;

    IF NEW.kind = 'suspended' AND NEW.payload -> 'wait' ->> 'kind' = 'timer' THEN
        wake := (NEW.payload -> 'wait' ->> 'until')::bigint;
    END IF;

    UPDATE ir_runs
    SET status = projected,
        wake_at_millis = wake,
        updated_at = now()
    WHERE tenant_id = NEW.tenant_id
      AND project_id = NEW.project_id
      AND id = NEW.run_id;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
