-- The effect ledger.
--
-- Every claim a run has made, and how it resolved. The event log remains the
-- authority; this table is derived from it by trigger, the way `ir_runs.status`
-- is, and exists for the question the log cannot answer cheaply: *right now,
-- across every run in this installation, which protected effects are in flight
-- and how long have they been there?* Answering that from the log alone means
-- folding every log, which is exactly the shape of query that stops being run.
--
-- It is the same reason `assurance_obligations` exists alongside the
-- certificate that already contains them.
--
-- Rows are written only by the trigger below. Nothing else writes here, so the
-- ledger cannot say something the log does not, and a test folds the log and
-- compares.

CREATE TABLE ir_effect_claims (
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    effect_id TEXT NOT NULL,
    attempt INTEGER NOT NULL,
    -- The key handed to the far side, exactly as it was sent. A retry has to
    -- present this one rather than derive a new one.
    idempotency_key TEXT,
    -- 'claimed' means nobody knows yet. That is the state this whole milestone
    -- is about, so it is a value here rather than an absence.
    outcome TEXT NOT NULL DEFAULT 'claimed',
    receipt TEXT,
    -- How many times this attempt was claimed. More than one happens when the
    -- IR declares a key: the retry keeps the attempt so the key stays stable,
    -- and each try is still worth counting.
    claim_count INTEGER NOT NULL DEFAULT 1,
    -- The lease that made the most recent claim, and where it sits in the log.
    epoch BIGINT NOT NULL,
    position BIGINT NOT NULL,
    claimed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, project_id, run_id, node_id, effect_id, attempt),
    FOREIGN KEY (tenant_id, project_id, run_id)
        REFERENCES ir_runs(tenant_id, project_id, id) ON DELETE RESTRICT,
    CONSTRAINT ir_effect_claims_attempt_is_sane CHECK (attempt >= 0),
    CONSTRAINT ir_effect_claims_count_is_sane CHECK (claim_count >= 1),
    CONSTRAINT ir_effect_claims_outcome_is_known
        CHECK (outcome IN ('claimed', 'finalized', 'uncertain')),
    -- A receipt is what "it happened" means here, so the two travel together.
    CONSTRAINT ir_effect_claims_receipt_matches_outcome
        CHECK ((outcome = 'finalized') = (receipt IS NOT NULL))
);

-- The operator query this table exists for.
CREATE INDEX ir_effect_claims_outstanding_idx
    ON ir_effect_claims(outcome, claimed_at)
    WHERE outcome = 'claimed';

CREATE INDEX ir_effect_claims_by_run_idx
    ON ir_effect_claims(tenant_id, project_id, run_id, position);

-- Maintains the ledger from the log.
--
-- Reading the payload rather than taking the caller's word for it is the point:
-- there is one write path into a run, and everything else follows from it.
CREATE FUNCTION capsulet_ir_project_effect_claim() RETURNS trigger AS $$
DECLARE
    node TEXT := NEW.payload ->> 'node';
    effect TEXT := NEW.payload ->> 'effect';
    try INTEGER := (NEW.payload ->> 'attempt')::integer;
    existing TEXT;
BEGIN
    IF NEW.kind = 'effect_claimed' THEN
        SELECT outcome INTO existing
        FROM ir_effect_claims
        WHERE tenant_id = NEW.tenant_id AND project_id = NEW.project_id
          AND run_id = NEW.run_id AND node_id = node AND effect_id = effect
          AND attempt = try;

        -- Re-claiming an effect already known to have happened is the
        -- duplication this milestone exists to prevent, and it is refused here
        -- as well as decided against upstream.
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
                NEW.payload ->> 'key', NEW.epoch, NEW.position
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
            receipt = NEW.payload ->> 'receipt',
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

CREATE TRIGGER ir_run_events_project_effect_claims
    AFTER INSERT ON ir_run_events
    FOR EACH ROW EXECUTE FUNCTION capsulet_ir_project_effect_claim();
