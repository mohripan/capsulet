use capsulet_core::{
    AutomationId, WorkflowId, WorkflowRun, WorkflowRunId, WorkflowRunStatus, WorkflowStepRun,
};
use sqlx::Row;

use crate::{
    PostgresStore, PostgresStoreError,
    rows::{generated_store_id, row_to_workflow_run, row_to_workflow_step_run},
};
impl PostgresStore {
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

        Ok(WorkflowRun {
            id: run_id.clone(),
            workflow_id: workflow_id.clone(),
            automation_id: automation_id.cloned(),
            input_json: input_json.to_string(),
            status: WorkflowRunStatus::Queued,
            current_step_position: 0,
            created_at: row.try_get("created_at")?,
        })
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
        let rows = sqlx::query(
            r"
            SELECT id, workflow_id, automation_id, input::text AS input, status, current_step_position
            FROM workflow_runs
            WHERE status IN ('queued', 'running')
            ORDER BY created_at ASC
            LIMIT 50
            ",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut advanced = 0;
        for row in rows {
            let run = row_to_workflow_run(&row)?;
            match run.status {
                WorkflowRunStatus::Queued => {
                    if self.start_workflow_step(&run, 1).await? {
                        advanced += 1;
                    }
                }
                WorkflowRunStatus::Running => {
                    if self.advance_running_workflow(&run).await? {
                        advanced += 1;
                    }
                }
                _ => {}
            }
        }
        Ok(advanced)
    }

    async fn advance_running_workflow(
        &self,
        run: &WorkflowRun,
    ) -> Result<bool, PostgresStoreError> {
        let Some(step_run_row) = sqlx::query(
            r"
            SELECT wsr.id, wsr.job_run_id, jr.status
            FROM workflow_step_runs wsr
            JOIN job_runs jr ON jr.id = wsr.job_run_id
            WHERE wsr.workflow_run_id = $1 AND wsr.position = $2
            ",
        )
        .bind(run.id.as_str())
        .bind(run.current_step_position)
        .fetch_optional(&self.pool)
        .await?
        else {
            self.finish_workflow_run(run, WorkflowRunStatus::Failed)
                .await?;
            return Ok(true);
        };

        let job_status: String = step_run_row.try_get("status")?;
        match job_status.as_str() {
            "succeeded" => {
                sqlx::query(
                    "UPDATE workflow_step_runs SET status = 'succeeded', updated_at = now() WHERE id = $1",
                )
                .bind(step_run_row.try_get::<String, _>("id")?)
                .execute(&self.pool)
                .await?;
                let next_position = run.current_step_position + 1;
                if self
                    .workflow_step_exists(&run.workflow_id, next_position)
                    .await?
                {
                    self.start_workflow_step(run, next_position).await
                } else {
                    self.finish_workflow_run(run, WorkflowRunStatus::Succeeded)
                        .await?;
                    Ok(true)
                }
            }
            "failed" => {
                self.finish_workflow_run(run, WorkflowRunStatus::Failed)
                    .await?;
                Ok(true)
            }
            "cancelled" => {
                self.finish_workflow_run(run, WorkflowRunStatus::Cancelled)
                    .await?;
                Ok(true)
            }
            "timed_out" => {
                self.finish_workflow_run(run, WorkflowRunStatus::TimedOut)
                    .await?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn workflow_step_exists(
        &self,
        workflow_id: &WorkflowId,
        position: i32,
    ) -> Result<bool, PostgresStoreError> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM workflow_steps WHERE workflow_id = $1 AND position = $2)",
        )
        .bind(workflow_id.as_str())
        .bind(position)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }

    async fn start_workflow_step(
        &self,
        run: &WorkflowRun,
        position: i32,
    ) -> Result<bool, PostgresStoreError> {
        let Some(step_row) = sqlx::query(
            r"
            SELECT id, job_definition_id, execution_pool
            FROM workflow_steps
            WHERE workflow_id = $1 AND position = $2
            ",
        )
        .bind(run.workflow_id.as_str())
        .bind(position)
        .fetch_optional(&self.pool)
        .await?
        else {
            self.finish_workflow_run(run, WorkflowRunStatus::Failed)
                .await?;
            return Ok(true);
        };
        let step_id: String = step_row.try_get("id")?;
        let job_definition_id: String = step_row.try_get("job_definition_id")?;
        let execution_pool: String = step_row.try_get("execution_pool")?;
        let job_run_id = generated_store_id("run_workflow_step");
        let step_run_id = generated_store_id("workflow_step_run");
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r"
            INSERT INTO job_runs (id, job_definition_id, status, execution_pool, input, updated_at)
            VALUES ($1, $2, 'queued', $3, $4::jsonb, now())
            ",
        )
        .bind(&job_run_id)
        .bind(&job_definition_id)
        .bind(&execution_pool)
        .bind(run.input_json.as_str())
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            r"
            INSERT INTO workflow_step_runs (
                id, workflow_run_id, workflow_step_id, job_run_id, position, status, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, 'queued', now())
            ",
        )
        .bind(&step_run_id)
        .bind(run.id.as_str())
        .bind(&step_id)
        .bind(&job_run_id)
        .bind(position)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            r"
            UPDATE workflow_runs
            SET status = 'running', current_step_position = $2, updated_at = now()
            WHERE id = $1
            ",
        )
        .bind(run.id.as_str())
        .bind(position)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    async fn finish_workflow_run(
        &self,
        run: &WorkflowRun,
        status: WorkflowRunStatus,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            UPDATE workflow_runs
            SET status = $2, updated_at = now(), finished_at = now()
            WHERE id = $1
            ",
        )
        .bind(run.id.as_str())
        .bind(status.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
