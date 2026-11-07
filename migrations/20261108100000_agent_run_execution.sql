-- Agent runs execute on the durable IR path.
--
-- An agent run stays the product record and links to the IR run that does the
-- work. Status and results are read from that run's log; nothing here is a
-- second answer to where a run is. Rows from before this migration have no link
-- and keep reporting what they always reported.

ALTER TABLE agent_runs ADD COLUMN ir_run_id TEXT;
ALTER TABLE agent_runs ADD COLUMN definition_digest TEXT;

ALTER TABLE agent_runs
    ADD CONSTRAINT agent_runs_ir_run_fkey
    FOREIGN KEY (tenant_id, project_id, ir_run_id)
    REFERENCES ir_runs (tenant_id, project_id, id)
    ON DELETE RESTRICT;

CREATE UNIQUE INDEX agent_runs_ir_run_idx
    ON agent_runs (tenant_id, project_id, ir_run_id)
    WHERE ir_run_id IS NOT NULL;

-- What a graph node does: a prompt template, a validator's rule, the job it
-- enqueues. Strings only, so it survives into a digest unchanged.
ALTER TABLE graph_nodes ADD COLUMN config JSONB NOT NULL DEFAULT '{}'::jsonb;

-- The bytes behind the digests a run's log carries.
--
-- Scoped to a project even though the key is a content digest. Two projects that
-- produced identical bytes each hold their own row, so holding a digest is never
-- a way to read what another project stored.
CREATE TABLE ir_run_values (
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    digest TEXT NOT NULL,
    content TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, project_id, digest),
    CONSTRAINT ir_run_values_size_is_bounded CHECK (size_bytes BETWEEN 0 AND 16777216)
);

-- Cancellation arrives through the inbox like every other outside signal.
ALTER TABLE ir_run_signals DROP CONSTRAINT ir_run_signals_kind_is_known;
ALTER TABLE ir_run_signals ADD CONSTRAINT ir_run_signals_kind_is_known
    CHECK (kind IN ('event', 'human_decision', 'timer_elapsed', 'cancel'));
