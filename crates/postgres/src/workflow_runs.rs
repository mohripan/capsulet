use std::collections::BTreeSet;

use capsulet_core::{
    AutomationId, WorkflowGraph, WorkflowId, WorkflowRun, WorkflowRunId, WorkflowRunStatus,
    WorkflowStepId, WorkflowStepRun,
};
use sqlx::Row;

use crate::{
    PostgresStore, PostgresStoreError,
    rows::{generated_store_id, row_to_workflow_run, row_to_workflow_step_run},
};
impl PostgresStore {
    /// Returns whether a workflow has an execution that can still consume its definition.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the lookup fails.
    pub async fn workflow_has_active_runs(
        &self,
        workflow_id: &WorkflowId,
    ) -> Result<bool, PostgresStoreError> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM workflow_runs WHERE workflow_id = $1 AND status IN ('queued', 'running'))",
        )
        .bind(workflow_id.as_str())
        .fetch_one(&self.pool)
        .await?)
    }

    /// Returns whether a job definition belongs to a workflow with an active execution.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the lookup fails.
    pub async fn job_definition_has_active_workflow_runs(
        &self,
        job_definition_id: &capsulet_core::JobDefinitionId,
    ) -> Result<bool, PostgresStoreError> {
        Ok(sqlx::query_scalar(
            r"
            SELECT EXISTS(
                SELECT 1
                FROM workflow_steps ws
                JOIN workflow_runs wr ON wr.workflow_id = ws.workflow_id
                WHERE ws.job_definition_id = $1
                  AND wr.status IN ('queued', 'running')
            )
            ",
        )
        .bind(job_definition_id.as_str())
        .fetch_one(&self.pool)
        .await?)
    }

    /// Returns true when any workflow step references the job definition.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the lookup fails.
    pub async fn job_definition_is_used_by_workflows(
        &self,
        job_definition_id: &capsulet_core::JobDefinitionId,
    ) -> Result<bool, PostgresStoreError> {
        Ok(sqlx::query_scalar(
            r"
            SELECT EXISTS(
                SELECT 1
                FROM workflow_steps
                WHERE job_definition_id = $1
            )
            ",
        )
        .bind(job_definition_id.as_str())
        .fetch_one(&self.pool)
        .await?)
    }

    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn create_workflow_run(
        &self,
        workflow_id: &WorkflowId,
        automation_id: Option<&AutomationId>,
        run_id: &WorkflowRunId,
        input_json: &str,
    ) -> Result<WorkflowRun, PostgresStoreError> {
        let row = sqlx::query(
            r"
            INSERT INTO workflow_runs (
                id, workflow_id, automation_id, input, status, current_step_position, updated_at
            )
            VALUES ($1, $2, $3, $4::jsonb, 'queued', 0, now())
            RETURNING created_at::text AS created_at
            ",
        )
        .bind(run_id.as_str())
        .bind(workflow_id.as_str())
        .bind(automation_id.map(AutomationId::as_str))
        .bind(input_json)
        .fetch_one(&self.pool)
        .await?;

        Ok(WorkflowRun::new(
            run_id.clone(),
            workflow_id.clone(),
            automation_id.cloned(),
            input_json,
            WorkflowRunStatus::Queued,
            0,
            row.try_get::<String, _>("created_at")?,
        ))
    }

    /// Lists workflow runs.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn list_workflow_runs(
        &self,
        limit: i64,
    ) -> Result<Vec<WorkflowRun>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, workflow_id, automation_id, input::text AS input, status, current_step_position, created_at::text AS created_at
            FROM workflow_runs
            ORDER BY created_at DESC
            LIMIT $1
            ",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(row_to_workflow_run).collect()
    }

    /// Finds one workflow run.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn find_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
    ) -> Result<Option<WorkflowRun>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, workflow_id, automation_id, input::text AS input, status, current_step_position, created_at::text AS created_at
            FROM workflow_runs
            WHERE id = $1
            ",
        )
        .bind(workflow_run_id.as_str())
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(row_to_workflow_run).transpose()
    }

    /// Marks a queued workflow run as removed when no step has started.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup or persistence fails.
    pub async fn remove_queued_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
    ) -> Result<Option<WorkflowRun>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            UPDATE workflow_runs
            SET status = 'removed', updated_at = now(), finished_at = now()
            WHERE id = $1
              AND status = 'queued'
              AND NOT EXISTS (
                  SELECT 1 FROM workflow_step_runs WHERE workflow_run_id = workflow_runs.id
              )
            RETURNING id, workflow_id, automation_id, input::text AS input, status, current_step_position, created_at::text AS created_at
            ",
        )
        .bind(workflow_run_id.as_str())
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            return row_to_workflow_run(&row).map(Some);
        }

        self.find_workflow_run(workflow_run_id).await
    }

    /// Cancels a running workflow run and its active job run.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup or persistence fails.
    pub async fn cancel_running_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
    ) -> Result<Option<WorkflowRun>, PostgresStoreError> {
        let mut tx = self.pool.begin().await?;
        let active = sqlx::query(
            r"
            SELECT wsr.id, wsr.job_run_id
            FROM workflow_runs wr
            JOIN workflow_step_runs wsr
              ON wsr.workflow_run_id = wr.id
            WHERE wr.id = $1
              AND wr.status = 'running'
              AND wsr.status IN ('queued', 'running')
            FOR UPDATE
            ",
        )
        .bind(workflow_run_id.as_str())
        .fetch_all(&mut *tx)
        .await?;

        if active.is_empty() {
            tx.rollback().await?;
            return self.find_workflow_run(workflow_run_id).await;
        }

        for step in &active {
            sqlx::query(
                r"
            UPDATE job_runs
            SET status = 'cancelled',
                lease_expires_at = NULL,
                heartbeat_at = NULL,
                retry_ready_at = NULL,
                updated_at = now()
            WHERE id = $1
              AND status IN ('queued', 'leased', 'running', 'retry_scheduled')
            ",
            )
            .bind(step.try_get::<String, _>("job_run_id")?)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                "UPDATE workflow_step_runs SET status = 'cancelled', updated_at = now() WHERE id = $1",
            )
            .bind(step.try_get::<String, _>("id")?)
            .execute(&mut *tx)
            .await?;
        }

        let row = sqlx::query(
            r"
            UPDATE workflow_runs
            SET status = 'cancelled', updated_at = now(), finished_at = now()
            WHERE id = $1
              AND status = 'running'
            RETURNING id, workflow_id, automation_id, input::text AS input, status, current_step_position, created_at::text AS created_at
            ",
        )
        .bind(workflow_run_id.as_str())
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;

        row.as_ref().map(row_to_workflow_run).transpose()
    }

    /// Lists step runs for one workflow run.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn list_workflow_step_runs(
        &self,
        workflow_run_id: &WorkflowRunId,
    ) -> Result<Vec<WorkflowStepRun>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, workflow_run_id, workflow_step_id, job_run_id, position, status
            FROM workflow_step_runs
            WHERE workflow_run_id = $1
            ORDER BY position ASC
            ",
        )
        .bind(workflow_run_id.as_str())
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(row_to_workflow_step_run).collect()
    }

    /// Resumes a failed workflow from its persisted successful step checkpoints.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup or persistence fails.
    pub async fn resume_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
    ) -> Result<Option<WorkflowRun>, PostgresStoreError> {
        let mut tx = self.pool.begin().await?;
        let status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM workflow_runs WHERE id = $1 FOR UPDATE",
        )
        .bind(workflow_run_id.as_str())
        .fetch_optional(&mut *tx)
        .await?;

        let Some(status) = status else {
            tx.rollback().await?;
            return Ok(None);
        };
        if !matches!(status.as_str(), "failed" | "timed_out") {
            tx.rollback().await?;
            return self.find_workflow_run(workflow_run_id).await;
        }

        let discarded_job_run_ids = sqlx::query_scalar::<_, String>(
            r"
            DELETE FROM workflow_step_runs
            WHERE workflow_run_id = $1
              AND status <> 'succeeded'
            RETURNING job_run_id
            ",
        )
        .bind(workflow_run_id.as_str())
        .fetch_all(&mut *tx)
        .await?;

        if !discarded_job_run_ids.is_empty() {
            sqlx::query("DELETE FROM job_runs WHERE id = ANY($1)")
                .bind(&discarded_job_run_ids)
                .execute(&mut *tx)
                .await?;
        }

        sqlx::query(
            r"
            UPDATE workflow_runs
            SET status = 'running',
                current_step_position = COALESCE((
                    SELECT max(position)
                    FROM workflow_step_runs
                    WHERE workflow_run_id = $1 AND status = 'succeeded'
                ), 0),
                finished_at = NULL,
                updated_at = now()
            WHERE id = $1
            ",
        )
        .bind(workflow_run_id.as_str())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        self.find_workflow_run(workflow_run_id).await
    }

    /// Creates workflow runs for enabled interval automations whose fire time is due.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup or persistence fails.
    pub async fn trigger_due_interval_automations(&self) -> Result<u64, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, workflow_id, interval_seconds
            FROM automations
            WHERE status = 'enabled'
              AND trigger_kind = 'interval'
              AND interval_seconds IS NOT NULL
              AND next_fire_at IS NOT NULL
              AND next_fire_at <= now()
            ORDER BY next_fire_at ASC
            LIMIT 25
            ",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut triggered = 0;
        for row in rows {
            let automation_id: String = row.try_get("id")?;
            let workflow_id: String = row.try_get("workflow_id")?;
            let interval_seconds: i32 = row.try_get("interval_seconds")?;
            let workflow_run_id = generated_store_id("workflow_run");
            let mut tx = self.pool.begin().await?;
            sqlx::query(
                r"
                INSERT INTO workflow_runs (
                    id, workflow_id, automation_id, input, status, current_step_position, updated_at
                )
                VALUES ($1, $2, $3, '{}'::jsonb, 'queued', 0, now())
                ",
            )
            .bind(&workflow_run_id)
            .bind(&workflow_id)
            .bind(&automation_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                r"
                UPDATE automations
                SET last_triggered_at = now(),
                    next_fire_at = now() + ($2 * interval '1 second'),
                    updated_at = now()
                WHERE id = $1
                ",
            )
            .bind(&automation_id)
            .bind(interval_seconds)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            triggered += 1;
        }

        Ok(triggered)
    }

    /// Advances queued and running workflow runs by creating or inspecting job runs.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup or persistence fails.
    pub async fn advance_workflow_runs(&self) -> Result<u64, PostgresStoreError> {
        let run_ids = sqlx::query_scalar::<_, String>(
            r"
            SELECT id
            FROM workflow_runs
            WHERE status IN ('queued', 'running')
            ORDER BY created_at ASC
            LIMIT 50
            ",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut advanced = 0;
        for run_id in run_ids {
            if self.reconcile_workflow_run(&run_id).await? {
                advanced += 1;
            }
        }
        Ok(advanced)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "the scheduler state transition stays in one database transaction"
    )]
    async fn reconcile_workflow_run(&self, run_id: &str) -> Result<bool, PostgresStoreError> {
        let mut tx = self.pool.begin().await?;
        let acquired: bool =
            sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtextextended($1, 0))")
                .bind(run_id)
                .fetch_one(&mut *tx)
                .await?;
        if !acquired {
            tx.rollback().await?;
            return Ok(false);
        }

        let Some(run_row) = sqlx::query(
            r"
            SELECT id, workflow_id, automation_id, input::text AS input, status,
                   current_step_position, created_at::text AS created_at
            FROM workflow_runs
            WHERE id = $1 AND status IN ('queued', 'running')
            FOR UPDATE
            ",
        )
        .bind(run_id)
        .fetch_optional(&mut *tx)
        .await?
        else {
            tx.rollback().await?;
            return Ok(false);
        };
        let run = row_to_workflow_run(&run_row)?;
        let workflow = self
            .find_workflow(run.workflow_id())
            .await?
            .ok_or_else(|| {
                PostgresStoreError::InvalidPersistedValue(format!(
                    "workflow {} not found",
                    run.workflow_id()
                ))
            })?;
        let graph = WorkflowGraph::new(workflow.id(), workflow.steps(), workflow.dependencies())?;

        sqlx::query(
            r"
            UPDATE workflow_step_runs wsr
            SET status = CASE
                    WHEN jr.status = 'succeeded' THEN 'succeeded'
                    WHEN jr.status = 'failed' THEN 'failed'
                    WHEN jr.status = 'cancelled' THEN 'cancelled'
                    WHEN jr.status = 'timed_out' THEN 'timed_out'
                    WHEN jr.status IN ('leased', 'running') THEN 'running'
                    ELSE 'queued'
                END,
                updated_at = now()
            FROM job_runs jr
            WHERE wsr.workflow_run_id = $1
              AND jr.id = wsr.job_run_id
              AND wsr.status IS DISTINCT FROM CASE
                    WHEN jr.status = 'succeeded' THEN 'succeeded'
                    WHEN jr.status = 'failed' THEN 'failed'
                    WHEN jr.status = 'cancelled' THEN 'cancelled'
                    WHEN jr.status = 'timed_out' THEN 'timed_out'
                    WHEN jr.status IN ('leased', 'running') THEN 'running'
                    ELSE 'queued'
                END
            ",
        )
        .bind(run_id)
        .execute(&mut *tx)
        .await?;

        let states = sqlx::query(
            r"
            SELECT workflow_step_id, status
            FROM workflow_step_runs
            WHERE workflow_run_id = $1
            ",
        )
        .bind(run_id)
        .fetch_all(&mut *tx)
        .await?;

        let mut started = BTreeSet::new();
        let mut succeeded = BTreeSet::new();
        let mut active = 0_usize;
        let mut failed = false;
        for state in &states {
            let step_id = WorkflowStepId::new(state.try_get::<String, _>("workflow_step_id")?)
                .map_err(PostgresStoreError::InvalidPersistedValue)?;
            started.insert(step_id.clone());
            match state.try_get::<String, _>("status")?.as_str() {
                "succeeded" => {
                    succeeded.insert(step_id);
                }
                "queued" | "running" => active += 1,
                "failed" | "cancelled" | "timed_out" => failed = true,
                _ => {}
            }
        }

        if succeeded.len() == workflow.steps().len() {
            Self::finish_workflow_run_in(&mut tx, run_id, WorkflowRunStatus::Succeeded).await?;
            tx.commit().await?;
            return Ok(true);
        }

        let ready = graph.ready_steps(&started, &succeeded);
        let ready_count = ready.len();
        let progress_position = workflow
            .steps()
            .iter()
            .filter(|step| started.contains(step.id()))
            .map(capsulet_core::WorkflowStep::position)
            .chain(
                ready
                    .iter()
                    .copied()
                    .map(capsulet_core::WorkflowStep::position),
            )
            .max()
            .unwrap_or(0);
        for step in ready {
            let job_run_id = generated_store_id("run_workflow_step");
            let step_run_id = generated_store_id("workflow_step_run");
            sqlx::query(
                "INSERT INTO job_runs (id, job_definition_id, status, execution_pool, input, updated_at) VALUES ($1, $2, 'queued', $3, $4::jsonb, now())",
            )
            .bind(&job_run_id)
            .bind(step.job_definition_id().as_str())
            .bind(step.execution_pool().as_str())
            .bind(run.input_json())
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO workflow_step_runs (id, workflow_run_id, workflow_step_id, job_run_id, position, status, updated_at) VALUES ($1, $2, $3, $4, $5, 'queued', now())",
            )
            .bind(&step_run_id)
            .bind(run_id)
            .bind(step.id().as_str())
            .bind(&job_run_id)
            .bind(step.position())
            .execute(&mut *tx)
            .await?;
        }

        if failed && active == 0 && ready_count == 0 {
            Self::finish_workflow_run_in(&mut tx, run_id, WorkflowRunStatus::Failed).await?;
        } else if ready_count > 0 || run.status() == WorkflowRunStatus::Queued {
            sqlx::query(
                "UPDATE workflow_runs SET status = 'running', current_step_position = GREATEST(current_step_position, $2), updated_at = now() WHERE id = $1",
            )
            .bind(run_id)
            .bind(progress_position)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(ready_count > 0 || failed)
    }

    async fn finish_workflow_run_in(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        run_id: &str,
        status: WorkflowRunStatus,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query("UPDATE workflow_runs SET status = $2, updated_at = now(), finished_at = now() WHERE id = $1")
            .bind(run_id)
            .bind(status.to_string())
            .execute(&mut **tx)
            .await?;
        Ok(())
    }
}
