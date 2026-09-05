-- A claim can now resolve three ways, not two.
--
-- `abandoned` means the far side refused outright: the effect definitely did
-- not happen, and the run is free to route the node's failure like any other.
-- `uncertain` means nobody knows, and the run stops. Collapsing the two would
-- make every refusal look like the one case that has to stop everything, which
-- is how a system learns to ignore the case that matters.

ALTER TABLE ir_effect_claims
    DROP CONSTRAINT ir_effect_claims_outcome_is_known;

ALTER TABLE ir_effect_claims
    ADD CONSTRAINT ir_effect_claims_outcome_is_known
    CHECK (outcome IN ('claimed', 'finalized', 'abandoned', 'uncertain'));

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

    ELSIF NEW.kind IN ('effect_abandoned', 'effect_uncertain') THEN
        UPDATE ir_effect_claims
        SET outcome = CASE NEW.kind
                WHEN 'effect_abandoned' THEN 'abandoned'
                ELSE 'uncertain'
            END,
            resolved_at = now()
        WHERE tenant_id = NEW.tenant_id AND project_id = NEW.project_id
          AND run_id = NEW.run_id AND node_id = node AND effect_id = effect
          AND attempt = try;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
