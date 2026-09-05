-- The event payload became externally tagged.
--
-- `RunEvent` used to serialize as `{"event": "iteration_finished", ...}` and now
-- serializes as `{"iteration_finished": {...}}`. The reason is not cosmetic:
-- serde reads an internally-tagged enum by buffering the object first, and that
-- buffer cannot hold a 128-bit integer — which is what a loop's progress measure
-- is. The old shape could be written and not read back, so a run with a progress
-- measure was unrecoverable the moment it restarted.
--
-- The variant key equals the `kind` column, so `payload -> kind` is the
-- variant's body and the triggers below reach through it.
--
-- No data migration: the shape changed before any run existed outside a test
-- database, and a row in the old shape would fail to fold rather than fold
-- wrongly, which is the failure mode to prefer.

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

    IF NEW.kind = 'suspended'
       AND NEW.payload -> NEW.kind -> 'wait' ->> 'kind' = 'timer' THEN
        wake := (NEW.payload -> NEW.kind -> 'wait' ->> 'until')::bigint;
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

CREATE OR REPLACE FUNCTION capsulet_ir_project_effect_claim() RETURNS trigger AS $$
DECLARE
    body JSONB := NEW.payload -> NEW.kind;
    node TEXT := body ->> 'node';
    effect TEXT := body ->> 'effect';
    try INTEGER := (body ->> 'attempt')::integer;
    existing TEXT;
BEGIN
    IF NEW.kind = 'effect_claimed' THEN
        SELECT outcome INTO existing
        FROM ir_effect_claims
        WHERE tenant_id = NEW.tenant_id AND project_id = NEW.project_id
          AND run_id = NEW.run_id AND node_id = node AND effect_id = effect
          AND attempt = try;

        IF existing = 'finalized' THEN
            RAISE EXCEPTION
                'effect %/% attempt % is already finalized; claiming it again was refused',
                node, effect, try
                USING ERRCODE = 'restrict_violation';
        END IF;

        IF existing IS NULL THEN
            INSERT INTO ir_effect_claims (
                tenant_id, project_id, run_id, node_id, effect_id, attempt,
                idempotency_key, epoch, position
            )
            VALUES (
                NEW.tenant_id, NEW.project_id, NEW.run_id, node, effect, try,
                body ->> 'key', NEW.epoch, NEW.position
            );
        ELSE
            UPDATE ir_effect_claims
            SET claim_count = claim_count + 1,
                outcome = 'claimed',
                epoch = NEW.epoch,
                position = NEW.position,
                resolved_at = NULL
            WHERE tenant_id = NEW.tenant_id AND project_id = NEW.project_id
              AND run_id = NEW.run_id AND node_id = node AND effect_id = effect
              AND attempt = try;
        END IF;

    ELSIF NEW.kind = 'effect_finalized' THEN
        UPDATE ir_effect_claims
        SET outcome = 'finalized',
            receipt = body ->> 'receipt',
            resolved_at = now()
        WHERE tenant_id = NEW.tenant_id AND project_id = NEW.project_id
          AND run_id = NEW.run_id AND node_id = node AND effect_id = effect
          AND attempt = try;

    ELSIF NEW.kind = 'effect_uncertain' THEN
        UPDATE ir_effect_claims
        SET outcome = 'uncertain',
            resolved_at = now()
        WHERE tenant_id = NEW.tenant_id AND project_id = NEW.project_id
          AND run_id = NEW.run_id AND node_id = node AND effect_id = effect
          AND attempt = try;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
