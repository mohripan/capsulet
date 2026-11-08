-- What a run was given and produced can be erased; the record that it ran cannot.
--
-- A run's log holds digests, never the bytes behind them, so it stays whole: it is
-- what a decision replays from and what a certificate names. The bytes live in
-- `ir_run_values`, and erasing one keeps its row as a tombstone. The digest still
-- names it, and a later read can say "erased" instead of "missing", which are
-- different statements about what happened.
--
-- Producing the same bytes again later stores them again: an erasure is about what
-- was held, not a ban on a string.

ALTER TABLE ir_run_values ALTER COLUMN content DROP NOT NULL;
ALTER TABLE ir_run_values ADD COLUMN erased_at TIMESTAMPTZ;
ALTER TABLE ir_run_values ADD COLUMN erased_by TEXT;

ALTER TABLE ir_run_values
    ADD CONSTRAINT ir_run_values_erasure_is_whole
    CHECK (
        (content IS NULL) = (erased_at IS NOT NULL)
        AND (erased_at IS NULL) = (erased_by IS NULL)
    );

-- The agent run row also kept a copy of the state it was started with.
ALTER TABLE agent_runs ADD COLUMN payloads_erased_at TIMESTAMPTZ;
ALTER TABLE agent_runs ADD COLUMN payloads_erased_by TEXT;

ALTER TABLE agent_runs
    ADD CONSTRAINT agent_runs_erasure_is_whole
    CHECK ((payloads_erased_at IS NULL) = (payloads_erased_by IS NULL));
