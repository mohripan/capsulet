-- Which runs hold which stored values.
--
-- Values are stored once per project for each distinct content, so the same bytes
-- (an empty string, a common validator result, the same question asked twice) are
-- held by many runs at once. Erasing one run's payloads therefore cannot simply
-- erase the bytes its log names: that would take them from every other run that
-- holds them, including one still running. A row here says "this run holds this
-- value"; erasing a run removes its rows, and a value's bytes go once no run holds
-- them any more.

CREATE TABLE ir_run_value_refs (
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    digest TEXT NOT NULL,
    run_id TEXT NOT NULL,
    PRIMARY KEY (tenant_id, project_id, digest, run_id),
    FOREIGN KEY (tenant_id, project_id, run_id)
        REFERENCES ir_runs (tenant_id, project_id, id) ON DELETE RESTRICT
);

CREATE INDEX ir_run_value_refs_run_idx ON ir_run_value_refs (tenant_id, project_id, run_id);

-- Runs from before this table name their values in their logs: what they were
-- given, and what each node produced. Values reached only through another value
-- were never named there, and get no row.
INSERT INTO ir_run_value_refs (tenant_id, project_id, digest, run_id)
SELECT DISTINCT events.tenant_id, events.project_id, named.value, events.run_id
FROM ir_run_events events
CROSS JOIN LATERAL jsonb_each_text(
    CASE events.kind
        WHEN 'inputs_bound' THEN events.payload -> 'inputs_bound' -> 'inputs'
        WHEN 'node_finished' THEN events.payload -> 'node_finished' -> 'outputs'
        WHEN 'completed' THEN events.payload -> 'completed' -> 'outputs'
    END
) AS named
WHERE events.kind IN ('inputs_bound', 'node_finished', 'completed')
  AND EXISTS (
      SELECT 1 FROM ir_run_values held
      WHERE held.tenant_id = events.tenant_id
        AND held.project_id = events.project_id
        AND held.digest = named.value
  )
  -- A run already erased holds nothing.
  AND NOT EXISTS (
      SELECT 1 FROM agent_runs erased
      WHERE erased.id = events.run_id
        AND erased.tenant_id = events.tenant_id
        AND erased.project_id = events.project_id
        AND erased.payloads_erased_at IS NOT NULL
  )
ON CONFLICT DO NOTHING;
