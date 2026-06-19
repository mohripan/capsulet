ALTER TABLE workflow_steps
    ADD CONSTRAINT workflow_steps_workflow_id_id_unique UNIQUE (workflow_id, id);

CREATE TABLE workflow_step_dependencies (
    workflow_id TEXT NOT NULL REFERENCES workflow_definitions(id) ON DELETE CASCADE,
    from_step_id TEXT NOT NULL,
    to_step_id TEXT NOT NULL,
    PRIMARY KEY (workflow_id, from_step_id, to_step_id),
    CHECK (from_step_id <> to_step_id),
    FOREIGN KEY (workflow_id, from_step_id)
        REFERENCES workflow_steps(workflow_id, id) ON DELETE CASCADE,
    FOREIGN KEY (workflow_id, to_step_id)
        REFERENCES workflow_steps(workflow_id, id) ON DELETE CASCADE
);

CREATE INDEX workflow_step_dependencies_from_idx
    ON workflow_step_dependencies (workflow_id, from_step_id);

CREATE INDEX workflow_step_dependencies_to_idx
    ON workflow_step_dependencies (workflow_id, to_step_id);

INSERT INTO workflow_step_dependencies (workflow_id, from_step_id, to_step_id)
SELECT workflow_id, previous_step_id, id
FROM (
    SELECT workflow_id, id,
           lag(id) OVER (PARTITION BY workflow_id ORDER BY position, id) AS previous_step_id
    FROM workflow_steps
) ordered_steps
WHERE previous_step_id IS NOT NULL;

ALTER TABLE workflow_step_runs
    ADD CONSTRAINT workflow_step_runs_run_step_unique
    UNIQUE (workflow_run_id, workflow_step_id);
