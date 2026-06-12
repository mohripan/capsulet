//! `PostgreSQL` persistence adapter for Capsulet.

use async_trait::async_trait;
use capsulet_core::{
    ArtifactId, ArtifactObjectKind, Automation, AutomationId, AutomationStatus, AutomationTrigger,
    AutomationTriggerKind, CustomTriggerPlugin, ExecutionPoolName, JobArtifact,
    JobArtifactRepository, JobAttemptId, JobDefinition, JobDefinitionId, JobRun, JobRunId,
    JobRunLog, JobRunLogRepository, JobRunRepository, JobRunStatus, RetryPolicy, TriggerKind,
    TriggerName, WorkflowDefinition, WorkflowId, WorkflowRun, WorkflowRunId, WorkflowRunStatus,
    WorkflowStatus, WorkflowStep, WorkflowStepId, WorkflowStepRun, WorkflowStepRunId,
};
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

/// `PostgreSQL`-backed store for Capsulet persistence.
#[derive(Debug, Clone)]
pub struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    /// Connects to `PostgreSQL`.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the connection pool cannot be
    /// created.
    pub async fn connect(database_url: &str) -> Result<Self, PostgresStoreError> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;

        Ok(Self { pool })
    }

    /// Creates a store from an existing pool.
    #[must_use]
    pub const fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Returns the underlying `PostgreSQL` pool.
    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Runs embedded `SQLx` migrations.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when a migration fails.
    pub async fn migrate(&self) -> Result<(), PostgresStoreError> {
        MIGRATOR.run(&self.pool).await?;
        Ok(())
    }

    /// Inserts or updates a job definition.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn upsert_job_definition(
        &self,
        definition: &JobDefinition,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO job_definitions (
                id,
                name,
                runtime_image,
                command,
                bundle_object_key,
                input_schema,
                retry_max_attempts,
                retry_delay_seconds,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6::jsonb, $7, $8, now())
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                runtime_image = EXCLUDED.runtime_image,
                command = EXCLUDED.command,
                bundle_object_key = EXCLUDED.bundle_object_key,
                input_schema = EXCLUDED.input_schema,
                retry_max_attempts = EXCLUDED.retry_max_attempts,
                retry_delay_seconds = EXCLUDED.retry_delay_seconds,
                updated_at = now()
            ",
        )
        .bind(definition.id.as_str())
        .bind(&definition.name)
        .bind(&definition.runtime_image)
        .bind(&definition.command)
        .bind(&definition.bundle_object_key)
        .bind(&definition.input_schema)
        .bind(
            i32::try_from(definition.retry_max_attempts)
                .map_err(|_| PostgresStoreError::AttemptOverflow)?,
        )
        .bind(i32::try_from(definition.retry_delay_seconds).map_err(|_| {
            PostgresStoreError::InvalidPersistedValue("retry delay is too large".into())
        })?)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Checks whether a job definition exists.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails.
    pub async fn job_definition_exists(
        &self,
        id: &JobDefinitionId,
    ) -> Result<bool, PostgresStoreError> {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM job_definitions WHERE id = $1)")
                .bind(id.as_str())
                .fetch_one(&self.pool)
                .await?;

        Ok(exists)
    }

    /// Finds a job definition by id.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are
    /// invalid.
    pub async fn find_job_definition(
        &self,
        id: &JobDefinitionId,
    ) -> Result<Option<JobDefinition>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id,
                   name,
                   runtime_image,
                   command,
                   bundle_object_key,
                   input_schema::text,
                   retry_max_attempts,
                   retry_delay_seconds
            FROM job_definitions
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(row_to_job_definition).transpose()
    }

    /// Lists job definitions ordered by most recently updated.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are
    /// invalid.
    pub async fn list_job_definitions(
        &self,
        limit: i64,
    ) -> Result<Vec<JobDefinition>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id,
                   name,
                   runtime_image,
                   command,
                   bundle_object_key,
                   input_schema::text,
                   retry_max_attempts,
                   retry_delay_seconds
            FROM job_definitions
            ORDER BY updated_at DESC, id ASC
            LIMIT $1
            ",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(row_to_job_definition).collect()
    }

    /// Deletes a job definition when it exists.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when deletion fails.
    pub async fn delete_job_definition(
        &self,
        id: &JobDefinitionId,
    ) -> Result<bool, PostgresStoreError> {
        let result = sqlx::query("DELETE FROM job_definitions WHERE id = $1")
            .bind(id.as_str())
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Inserts or updates a workflow definition and its ordered steps.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn upsert_workflow(
        &self,
        workflow: &WorkflowDefinition,
    ) -> Result<(), PostgresStoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r"
            INSERT INTO workflow_definitions (id, name, description, status, updated_at)
            VALUES ($1, $2, $3, $4, now())
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                status = EXCLUDED.status,
                updated_at = now()
            ",
        )
        .bind(workflow.id.as_str())
        .bind(&workflow.name)
        .bind(&workflow.description)
        .bind(workflow.status.to_string())
        .execute(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM workflow_steps WHERE workflow_id = $1")
            .bind(workflow.id.as_str())
            .execute(&mut *tx)
            .await?;

        for step in &workflow.steps {
            sqlx::query(
                r"
                INSERT INTO workflow_steps (
                    id, workflow_id, position, name, job_definition_id, execution_pool, updated_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, now())
                ",
            )
            .bind(step.id.as_str())
            .bind(workflow.id.as_str())
            .bind(step.position)
            .bind(&step.name)
            .bind(step.job_definition_id.as_str())
            .bind(step.execution_pool.as_str())
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Lists workflow definitions with their ordered steps.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn list_workflows(
        &self,
        limit: i64,
    ) -> Result<Vec<WorkflowDefinition>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, name, description, status
            FROM workflow_definitions
            ORDER BY updated_at DESC, id ASC
            LIMIT $1
            ",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut workflows = Vec::with_capacity(rows.len());
        for row in rows {
            workflows.push(self.workflow_from_row(&row).await?);
        }
        Ok(workflows)
    }

    /// Finds one workflow definition with its ordered steps.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn find_workflow(
        &self,
        id: &WorkflowId,
    ) -> Result<Option<WorkflowDefinition>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, name, description, status
            FROM workflow_definitions
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => Ok(Some(self.workflow_from_row(&row).await?)),
            None => Ok(None),
        }
    }

    async fn workflow_from_row(
        &self,
        row: &sqlx::postgres::PgRow,
    ) -> Result<WorkflowDefinition, PostgresStoreError> {
        let id: String = row.try_get("id")?;
        let workflow_id = WorkflowId::new(id).map_err(PostgresStoreError::InvalidPersistedValue)?;
        let step_rows = sqlx::query(
            r"
            SELECT id, workflow_id, position, name, job_definition_id, execution_pool
            FROM workflow_steps
            WHERE workflow_id = $1
            ORDER BY position ASC
            ",
        )
        .bind(workflow_id.as_str())
        .fetch_all(&self.pool)
        .await?;

        Ok(WorkflowDefinition {
            id: workflow_id,
            name: row.try_get("name")?,
            description: row.try_get("description")?,
            status: parse_workflow_status(row.try_get::<String, _>("status")?.as_str())?,
            steps: step_rows
                .iter()
                .map(row_to_workflow_step)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    /// Inserts or updates an automation.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn upsert_automation(
        &self,
        automation: &Automation,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO automations (
                id, name, description, workflow_id, status, trigger_kind,
                interval_seconds, next_fire_at, updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7,
                CASE WHEN $6 = 'interval' THEN now() ELSE NULL END,
                now()
            )
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                workflow_id = EXCLUDED.workflow_id,
                status = EXCLUDED.status,
                trigger_kind = EXCLUDED.trigger_kind,
                interval_seconds = EXCLUDED.interval_seconds,
                next_fire_at = COALESCE(automations.next_fire_at, EXCLUDED.next_fire_at),
                updated_at = now()
            ",
        )
        .bind(automation.id.as_str())
        .bind(&automation.name)
        .bind(&automation.description)
        .bind(automation.workflow_id.as_str())
        .bind(automation.status.to_string())
        .bind(automation.trigger_kind.to_string())
        .bind(automation.interval_seconds)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Lists automations.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn list_automations(
        &self,
        limit: i64,
    ) -> Result<Vec<Automation>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, name, description, workflow_id, status, trigger_kind, interval_seconds
            FROM automations
            ORDER BY updated_at DESC, id ASC
            LIMIT $1
            ",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(row_to_automation).collect()
    }

    /// Finds one automation.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn find_automation(
        &self,
        id: &AutomationId,
    ) -> Result<Option<Automation>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, name, description, workflow_id, status, trigger_kind, interval_seconds
            FROM automations
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(row_to_automation).transpose()
    }

    /// Replaces an automation trigger graph and its condition tree.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn replace_automation_triggers(
        &self,
        automation_id: &AutomationId,
        triggers: &[AutomationTrigger],
        condition_json: &str,
    ) -> Result<(), PostgresStoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r"
            UPDATE automations
            SET condition_tree = $2::jsonb,
                updated_at = now()
            WHERE id = $1
            ",
        )
        .bind(automation_id.as_str())
        .bind(condition_json)
        .execute(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM automation_triggers WHERE automation_id = $1")
            .bind(automation_id.as_str())
            .execute(&mut *tx)
            .await?;

        for trigger in triggers {
            sqlx::query(
                r"
                INSERT INTO automation_triggers (
                    id, automation_id, name, kind, config, plugin_id, enabled, updated_at
                )
                VALUES ($1, $2, $3, $4, $5::jsonb, $6, $7, now())
                ",
            )
            .bind(format!(
                "{}_{}",
                automation_id.as_str(),
                trigger.name.as_str()
            ))
            .bind(automation_id.as_str())
            .bind(trigger.name.as_str())
            .bind(trigger.kind.to_string())
            .bind(&trigger.config_json)
            .bind(trigger.plugin_id.as_deref())
            .bind(trigger.enabled)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Lists trigger definitions and the stored condition tree for one automation.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn list_automation_triggers(
        &self,
        automation_id: &AutomationId,
    ) -> Result<(Vec<AutomationTrigger>, String), PostgresStoreError> {
        let condition_json: Option<String> =
            sqlx::query_scalar("SELECT condition_tree::text FROM automations WHERE id = $1")
                .bind(automation_id.as_str())
                .fetch_optional(&self.pool)
                .await?;
        let rows = sqlx::query(
            r"
            SELECT automation_id, name, kind, config::text, plugin_id, enabled
            FROM automation_triggers
            WHERE automation_id = $1
            ORDER BY name ASC
            ",
        )
        .bind(automation_id.as_str())
        .fetch_all(&self.pool)
        .await?;

        Ok((
            rows.iter()
                .map(row_to_automation_trigger)
                .collect::<Result<Vec<_>, _>>()?,
            condition_json.unwrap_or_else(|| "{}".to_string()),
        ))
    }

    /// Inserts or updates a custom trigger plugin registry entry.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn upsert_custom_trigger_plugin(
        &self,
        plugin: &CustomTriggerPlugin,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO custom_trigger_plugins (
                id, name, description, runtime_image, command, config_schema, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6::jsonb, now())
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                runtime_image = EXCLUDED.runtime_image,
                command = EXCLUDED.command,
                config_schema = EXCLUDED.config_schema,
                updated_at = now()
            ",
        )
        .bind(&plugin.id)
        .bind(&plugin.name)
        .bind(&plugin.description)
        .bind(&plugin.runtime_image)
        .bind(&plugin.command)
        .bind(&plugin.config_schema_json)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Lists custom trigger plugins.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails.
    pub async fn list_custom_trigger_plugins(
        &self,
        limit: i64,
    ) -> Result<Vec<CustomTriggerPlugin>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, name, description, runtime_image, command, config_schema::text
            FROM custom_trigger_plugins
            ORDER BY updated_at DESC, id ASC
            LIMIT $1
            ",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(row_to_custom_trigger_plugin).collect()
    }

    /// Finds a custom trigger plugin by id.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails.
    pub async fn find_custom_trigger_plugin(
        &self,
        id: &str,
    ) -> Result<Option<CustomTriggerPlugin>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, name, description, runtime_image, command, config_schema::text
            FROM custom_trigger_plugins
            WHERE id = $1
            ",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(row_to_custom_trigger_plugin).transpose()
    }

    /// Creates a queued workflow run.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn create_workflow_run(
        &self,
        workflow_id: &WorkflowId,
        automation_id: Option<&AutomationId>,
        run_id: &WorkflowRunId,
    ) -> Result<WorkflowRun, PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO workflow_runs (
                id, workflow_id, automation_id, status, current_step_position, updated_at
            )
            VALUES ($1, $2, $3, 'queued', 0, now())
            ",
        )
        .bind(run_id.as_str())
        .bind(workflow_id.as_str())
        .bind(automation_id.map(AutomationId::as_str))
        .execute(&self.pool)
        .await?;

        Ok(WorkflowRun {
            id: run_id.clone(),
            workflow_id: workflow_id.clone(),
            automation_id: automation_id.cloned(),
            status: WorkflowRunStatus::Queued,
            current_step_position: 0,
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
            SELECT id, workflow_id, automation_id, status, current_step_position
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
                    id, workflow_id, automation_id, status, current_step_position, updated_at
                )
                VALUES ($1, $2, $3, 'queued', 0, now())
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
            SELECT id, workflow_id, automation_id, status, current_step_position
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
            INSERT INTO job_runs (id, job_definition_id, status, execution_pool, updated_at)
            VALUES ($1, $2, 'queued', $3, now())
            ",
        )
        .bind(&job_run_id)
        .bind(&job_definition_id)
        .bind(&execution_pool)
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

    /// Inserts the built-in hello Python definition for local testing.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn seed_hello_python_job_definition(&self) -> Result<(), PostgresStoreError> {
        self.seed_example_job_definitions().await
    }

    /// Inserts built-in example definitions for local testing.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn seed_example_job_definitions(&self) -> Result<(), PostgresStoreError> {
        for definition in [
            JobDefinition::hello_python(),
            JobDefinition::sleep_python(),
            JobDefinition::fail_python(),
            JobDefinition::timeout_python(),
            JobDefinition::artifact_python(),
        ] {
            self.upsert_job_definition(&definition).await?;
        }
        Ok(())
    }

    /// Lists job runs ordered by creation time, newest first.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails.
    pub async fn list_job_runs(&self, limit: i64) -> Result<Vec<JobRun>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, job_definition_id, status, execution_pool, attempt_count
            FROM job_runs
            ORDER BY created_at DESC
            LIMIT $1
            ",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(row_to_job_run).collect()
    }

    /// Leases the oldest queued job run.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the lease query fails or when stored
    /// state cannot be mapped back into the domain.
    pub async fn lease_next_queued_run(
        &self,
        worker_id: &str,
        lease_seconds: i64,
    ) -> Result<Option<JobRun>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            WITH candidate AS (
                SELECT id
                FROM job_runs
                WHERE status = 'queued'
                ORDER BY created_at
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            )
            UPDATE job_runs
            SET
                status = 'leased',
                lease_owner = $1,
                lease_expires_at = now() + ($2 * interval '1 second'),
                updated_at = now()
            FROM candidate
            WHERE job_runs.id = candidate.id
            RETURNING job_runs.id,
                      job_runs.job_definition_id,
                      job_runs.status,
                      job_runs.execution_pool,
                      job_runs.attempt_count
            ",
        )
        .bind(worker_id)
        .bind(lease_seconds)
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(row_to_job_run).transpose()
    }

    /// Cancels a non-terminal run and returns its latest state.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn cancel_run(&self, id: &JobRunId) -> Result<Option<JobRun>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            UPDATE job_runs
            SET
                status = 'cancelled',
                lease_expires_at = NULL,
                retry_ready_at = NULL,
                updated_at = now()
            WHERE id = $1
              AND status IN ('queued', 'leased', 'running', 'retry_scheduled')
            RETURNING id, job_definition_id, status, execution_pool, attempt_count
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            return row_to_job_run(&row).map(Some);
        }

        self.find_by_id(id).await
    }

    /// Finishes a running attempt only if no newer state has replaced it.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn finish_running_attempt(
        &self,
        id: &JobRunId,
        attempt_count: u32,
        status: JobRunStatus,
        retry_delay_seconds: Option<u64>,
    ) -> Result<Option<JobRun>, PostgresStoreError> {
        let retry_ready_at =
            retry_delay_seconds.map(|seconds| format!("now() + ({seconds} * interval '1 second')"));
        let status_value = status.to_string();
        let query = if retry_ready_at.is_some() {
            r"
            UPDATE job_runs
            SET
                status = $3,
                lease_owner = NULL,
                lease_expires_at = NULL,
                retry_ready_at = now() + ($4 * interval '1 second'),
                updated_at = now()
            WHERE id = $1
              AND attempt_count = $2
              AND status = 'running'
            RETURNING id, job_definition_id, status, execution_pool, attempt_count
            "
        } else {
            r"
            UPDATE job_runs
            SET
                status = $3,
                lease_owner = NULL,
                lease_expires_at = NULL,
                retry_ready_at = NULL,
                updated_at = now()
            WHERE id = $1
              AND attempt_count = $2
              AND status = 'running'
            RETURNING id, job_definition_id, status, execution_pool, attempt_count
            "
        };

        let mut query = sqlx::query(query)
            .bind(id.as_str())
            .bind(i32::try_from(attempt_count).map_err(|_| PostgresStoreError::AttemptOverflow)?)
            .bind(status_value);
        if let Some(delay) = retry_delay_seconds {
            query = query.bind(i32::try_from(delay).map_err(|_| {
                PostgresStoreError::InvalidPersistedValue("retry delay is too large".into())
            })?);
        }
        let row = query.fetch_optional(&self.pool).await?;

        row.as_ref().map(row_to_job_run).transpose()
    }

    /// Requeues retry-scheduled runs whose retry delay has elapsed.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn promote_ready_retries(&self) -> Result<u64, PostgresStoreError> {
        let result = sqlx::query(
            r"
            UPDATE job_runs
            SET
                status = 'queued',
                retry_ready_at = NULL,
                updated_at = now()
            WHERE status = 'retry_scheduled'
              AND retry_ready_at <= now()
            ",
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Requeues expired leased or running attempts that did not reach terminal state.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn recover_expired_leases(&self) -> Result<u64, PostgresStoreError> {
        let result = sqlx::query(
            r"
            UPDATE job_runs
            SET
                status = 'queued',
                lease_owner = NULL,
                lease_expires_at = NULL,
                updated_at = now()
            WHERE status IN ('leased', 'running')
              AND lease_expires_at <= now()
            ",
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Returns whether the run is currently cancelled.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails.
    pub async fn is_run_cancelled(&self, id: &JobRunId) -> Result<bool, PostgresStoreError> {
        let cancelled: bool =
            sqlx::query_scalar("SELECT status = 'cancelled' FROM job_runs WHERE id = $1")
                .bind(id.as_str())
                .fetch_optional(&self.pool)
                .await?
                .unwrap_or(false);

        Ok(cancelled)
    }

    /// Persists object-backed artifact metadata.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn upsert_artifact(&self, artifact: &JobArtifact) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO job_artifacts (
                id,
                job_run_id,
                job_attempt_id,
                name,
                object_key,
                content_type,
                size_bytes,
                checksum_sha256,
                kind
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (job_run_id, name, kind) DO UPDATE SET
                object_key = EXCLUDED.object_key,
                content_type = EXCLUDED.content_type,
                size_bytes = EXCLUDED.size_bytes,
                checksum_sha256 = EXCLUDED.checksum_sha256
            ",
        )
        .bind(artifact.id.as_str())
        .bind(artifact.run_id.as_str())
        .bind(artifact.attempt_id.as_ref().map(JobAttemptId::as_str))
        .bind(&artifact.name)
        .bind(&artifact.object_key)
        .bind(&artifact.content_type)
        .bind(i64::try_from(artifact.size_bytes).map_err(|_| {
            PostgresStoreError::InvalidPersistedValue("artifact size is too large".into())
        })?)
        .bind(&artifact.checksum_sha256)
        .bind(artifact.kind.as_str())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Lists object-backed artifacts for one run.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or values are invalid.
    pub async fn list_artifacts(
        &self,
        run_id: &JobRunId,
    ) -> Result<Vec<JobArtifact>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id,
                   job_run_id,
                   job_attempt_id,
                   name,
                   object_key,
                   content_type,
                   size_bytes,
                   checksum_sha256,
                   kind
            FROM job_artifacts
            WHERE job_run_id = $1
            ORDER BY created_at, name
            ",
        )
        .bind(run_id.as_str())
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(row_to_job_artifact).collect()
    }

    /// Finds one artifact by run and artifact id.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or values are invalid.
    pub async fn find_artifact(
        &self,
        run_id: &JobRunId,
        artifact_id: &ArtifactId,
    ) -> Result<Option<JobArtifact>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id,
                   job_run_id,
                   job_attempt_id,
                   name,
                   object_key,
                   content_type,
                   size_bytes,
                   checksum_sha256,
                   kind
            FROM job_artifacts
            WHERE job_run_id = $1
              AND id = $2
            ",
        )
        .bind(run_id.as_str())
        .bind(artifact_id.as_str())
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(row_to_job_artifact).transpose()
    }
}

#[async_trait]
impl JobRunRepository for PostgresStore {
    type Error = PostgresStoreError;

    async fn save(&self, run: &JobRun) -> Result<(), Self::Error> {
        sqlx::query(
            r"
            INSERT INTO job_runs (
                id,
                job_definition_id,
                status,
                execution_pool,
                attempt_count,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, now())
            ON CONFLICT (id) DO UPDATE SET
                status = EXCLUDED.status,
                execution_pool = EXCLUDED.execution_pool,
                attempt_count = EXCLUDED.attempt_count,
                updated_at = now()
            ",
        )
        .bind(run.id.as_str())
        .bind(run.job_definition_id.as_str())
        .bind(run.status.to_string())
        .bind(run.execution_pool.as_str())
        .bind(i32::try_from(run.attempt_count).map_err(|_| PostgresStoreError::AttemptOverflow)?)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_by_id(&self, id: &JobRunId) -> Result<Option<JobRun>, Self::Error> {
        let row = sqlx::query(
            r"
            SELECT id, job_definition_id, status, execution_pool, attempt_count
            FROM job_runs
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(row_to_job_run).transpose()
    }
}

#[async_trait]
impl JobRunLogRepository for PostgresStore {
    type Error = PostgresStoreError;

    async fn save_log(&self, log: &JobRunLog) -> Result<(), Self::Error> {
        sqlx::query(
            r"
            INSERT INTO job_run_logs (job_run_id, log_text, updated_at)
            VALUES ($1, $2, now())
            ON CONFLICT (job_run_id) DO UPDATE SET
                log_text = EXCLUDED.log_text,
                updated_at = now()
            ",
        )
        .bind(log.run_id.as_str())
        .bind(&log.text)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_log_by_run_id(&self, id: &JobRunId) -> Result<Option<JobRunLog>, Self::Error> {
        let row = sqlx::query(
            r"
            SELECT job_run_id, log_text
            FROM job_run_logs
            WHERE job_run_id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(row_to_job_run_log).transpose()
    }
}

#[async_trait]
impl JobArtifactRepository for PostgresStore {
    type Error = PostgresStoreError;

    async fn save_artifact(&self, artifact: &JobArtifact) -> Result<(), Self::Error> {
        self.upsert_artifact(artifact).await
    }

    async fn list_artifacts_by_run(
        &self,
        run_id: &JobRunId,
    ) -> Result<Vec<JobArtifact>, Self::Error> {
        self.list_artifacts(run_id).await
    }

    async fn find_artifact_by_run(
        &self,
        run_id: &JobRunId,
        artifact_id: &ArtifactId,
    ) -> Result<Option<JobArtifact>, Self::Error> {
        self.find_artifact(run_id, artifact_id).await
    }
}

/// `PostgreSQL` adapter error.
#[derive(Debug, Error)]
pub enum PostgresStoreError {
    #[error("postgres error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("invalid persisted value: {0}")]
    InvalidPersistedValue(String),
    #[error("job attempt count is too large to persist")]
    AttemptOverflow,
}

fn row_to_job_run(row: &sqlx::postgres::PgRow) -> Result<JobRun, PostgresStoreError> {
    let id: String = row.try_get("id")?;
    let job_definition_id: String = row.try_get("job_definition_id")?;
    let status: String = row.try_get("status")?;
    let execution_pool: String = row.try_get("execution_pool")?;
    let attempt_count: i32 = row.try_get("attempt_count")?;

    let mut run = JobRun::new(
        JobRunId::new(id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        JobDefinitionId::new(job_definition_id)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        ExecutionPoolName::new(execution_pool)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
    );
    run.status = parse_status(&status)?;
    run.attempt_count = u32::try_from(attempt_count)
        .map_err(|_| PostgresStoreError::InvalidPersistedValue("negative attempt count".into()))?;

    Ok(run)
}

fn generated_store_id(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("{prefix}_{nanos}")
}

fn row_to_job_definition(row: &sqlx::postgres::PgRow) -> Result<JobDefinition, PostgresStoreError> {
    let id: String = row.try_get("id")?;
    let name: String = row.try_get("name")?;
    let runtime_image: String = row.try_get("runtime_image")?;
    let command: Vec<String> = row.try_get("command")?;
    let bundle_object_key: String = row.try_get("bundle_object_key")?;
    let input_schema: String = row.try_get("input_schema")?;
    let retry_max_attempts: i32 = row.try_get("retry_max_attempts")?;
    let retry_delay_seconds: i32 = row.try_get("retry_delay_seconds")?;

    JobDefinition::new(
        JobDefinitionId::new(id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        name,
        runtime_image,
        command,
        bundle_object_key,
        input_schema,
        RetryPolicy {
            max_attempts: u32::try_from(retry_max_attempts).map_err(|_| {
                PostgresStoreError::InvalidPersistedValue("negative retry max attempts".into())
            })?,
            delay_seconds: u64::try_from(retry_delay_seconds).map_err(|_| {
                PostgresStoreError::InvalidPersistedValue("negative retry delay".into())
            })?,
        },
    )
    .map_err(PostgresStoreError::InvalidPersistedValue)
}

fn row_to_workflow_step(row: &sqlx::postgres::PgRow) -> Result<WorkflowStep, PostgresStoreError> {
    let id: String = row.try_get("id")?;
    let workflow_id: String = row.try_get("workflow_id")?;
    let job_definition_id: String = row.try_get("job_definition_id")?;
    let execution_pool: String = row.try_get("execution_pool")?;

    Ok(WorkflowStep {
        id: WorkflowStepId::new(id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        workflow_id: WorkflowId::new(workflow_id)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        position: row.try_get("position")?,
        name: row.try_get("name")?,
        job_definition_id: JobDefinitionId::new(job_definition_id)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        execution_pool: ExecutionPoolName::new(execution_pool)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
    })
}

fn row_to_automation(row: &sqlx::postgres::PgRow) -> Result<Automation, PostgresStoreError> {
    let id: String = row.try_get("id")?;
    let workflow_id: String = row.try_get("workflow_id")?;
    let status: String = row.try_get("status")?;
    let trigger_kind: String = row.try_get("trigger_kind")?;
    let interval_seconds: Option<i32> = row.try_get("interval_seconds")?;

    Ok(Automation {
        id: AutomationId::new(id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        workflow_id: WorkflowId::new(workflow_id)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        status: parse_automation_status(&status)?,
        trigger_kind: parse_automation_trigger_kind(&trigger_kind)?,
        interval_seconds: interval_seconds.map(i64::from),
    })
}

fn row_to_automation_trigger(
    row: &sqlx::postgres::PgRow,
) -> Result<AutomationTrigger, PostgresStoreError> {
    let automation_id: String = row.try_get("automation_id")?;
    let name: String = row.try_get("name")?;
    let kind: String = row.try_get("kind")?;

    Ok(AutomationTrigger {
        automation_id: AutomationId::new(automation_id)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        name: TriggerName::new(name).map_err(PostgresStoreError::InvalidPersistedValue)?,
        kind: parse_trigger_kind(&kind)?,
        config_json: row.try_get("config")?,
        plugin_id: row.try_get("plugin_id")?,
        enabled: row.try_get("enabled")?,
    })
}

fn row_to_custom_trigger_plugin(
    row: &sqlx::postgres::PgRow,
) -> Result<CustomTriggerPlugin, PostgresStoreError> {
    Ok(CustomTriggerPlugin {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        runtime_image: row.try_get("runtime_image")?,
        command: row.try_get("command")?,
        config_schema_json: row.try_get("config_schema")?,
    })
}

fn row_to_workflow_run(row: &sqlx::postgres::PgRow) -> Result<WorkflowRun, PostgresStoreError> {
    let id: String = row.try_get("id")?;
    let workflow_id: String = row.try_get("workflow_id")?;
    let automation_id: Option<String> = row.try_get("automation_id")?;
    let status: String = row.try_get("status")?;

    Ok(WorkflowRun {
        id: WorkflowRunId::new(id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        workflow_id: WorkflowId::new(workflow_id)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        automation_id: automation_id
            .map(AutomationId::new)
            .transpose()
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        status: parse_workflow_run_status(&status)?,
        current_step_position: row.try_get("current_step_position")?,
    })
}

fn row_to_workflow_step_run(
    row: &sqlx::postgres::PgRow,
) -> Result<WorkflowStepRun, PostgresStoreError> {
    let id: String = row.try_get("id")?;
    let workflow_run_id: String = row.try_get("workflow_run_id")?;
    let workflow_step_id: String = row.try_get("workflow_step_id")?;
    let job_run_id: String = row.try_get("job_run_id")?;
    let status: String = row.try_get("status")?;

    Ok(WorkflowStepRun {
        id: WorkflowStepRunId::new(id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        workflow_run_id: WorkflowRunId::new(workflow_run_id)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        workflow_step_id: WorkflowStepId::new(workflow_step_id)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        job_run_id: JobRunId::new(job_run_id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        position: row.try_get("position")?,
        status: parse_workflow_run_status(&status)?,
    })
}

fn row_to_job_run_log(row: &sqlx::postgres::PgRow) -> Result<JobRunLog, PostgresStoreError> {
    let run_id: String = row.try_get("job_run_id")?;
    let log_text: String = row.try_get("log_text")?;

    JobRunLog::new(
        JobRunId::new(run_id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        log_text,
    )
    .map_err(PostgresStoreError::InvalidPersistedValue)
}

fn row_to_job_artifact(row: &sqlx::postgres::PgRow) -> Result<JobArtifact, PostgresStoreError> {
    let id: String = row.try_get("id")?;
    let job_run_id: String = row.try_get("job_run_id")?;
    let job_attempt_id: Option<String> = row.try_get("job_attempt_id")?;
    let name: String = row.try_get("name")?;
    let object_key: String = row.try_get("object_key")?;
    let content_type: String = row.try_get("content_type")?;
    let size_bytes: i64 = row.try_get("size_bytes")?;
    let checksum_sha256: Option<String> = row.try_get("checksum_sha256")?;
    let kind: String = row.try_get("kind")?;

    JobArtifact::new(
        ArtifactId::new(id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        JobRunId::new(job_run_id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        job_attempt_id
            .map(JobAttemptId::new)
            .transpose()
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        name,
        object_key,
        content_type,
        u64::try_from(size_bytes).map_err(|_| {
            PostgresStoreError::InvalidPersistedValue("negative artifact size".into())
        })?,
        checksum_sha256,
        parse_artifact_kind(&kind)?,
    )
    .map_err(PostgresStoreError::InvalidPersistedValue)
}

fn parse_status(status: &str) -> Result<JobRunStatus, PostgresStoreError> {
    match status {
        "queued" => Ok(JobRunStatus::Queued),
        "leased" => Ok(JobRunStatus::Leased),
        "running" => Ok(JobRunStatus::Running),
        "succeeded" => Ok(JobRunStatus::Succeeded),
        "failed" => Ok(JobRunStatus::Failed),
        "cancelled" => Ok(JobRunStatus::Cancelled),
        "timed_out" => Ok(JobRunStatus::TimedOut),
        "retry_scheduled" => Ok(JobRunStatus::RetryScheduled),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown job run status {value}"
        ))),
    }
}

fn parse_workflow_status(status: &str) -> Result<WorkflowStatus, PostgresStoreError> {
    match status {
        "draft" => Ok(WorkflowStatus::Draft),
        "enabled" => Ok(WorkflowStatus::Enabled),
        "disabled" => Ok(WorkflowStatus::Disabled),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown workflow status {value}"
        ))),
    }
}

fn parse_workflow_run_status(status: &str) -> Result<WorkflowRunStatus, PostgresStoreError> {
    match status {
        "queued" => Ok(WorkflowRunStatus::Queued),
        "running" => Ok(WorkflowRunStatus::Running),
        "succeeded" => Ok(WorkflowRunStatus::Succeeded),
        "failed" => Ok(WorkflowRunStatus::Failed),
        "cancelled" => Ok(WorkflowRunStatus::Cancelled),
        "timed_out" => Ok(WorkflowRunStatus::TimedOut),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown workflow run status {value}"
        ))),
    }
}

fn parse_automation_status(status: &str) -> Result<AutomationStatus, PostgresStoreError> {
    match status {
        "enabled" => Ok(AutomationStatus::Enabled),
        "disabled" => Ok(AutomationStatus::Disabled),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown automation status {value}"
        ))),
    }
}

fn parse_automation_trigger_kind(
    trigger_kind: &str,
) -> Result<AutomationTriggerKind, PostgresStoreError> {
    match trigger_kind {
        "manual" => Ok(AutomationTriggerKind::Manual),
        "interval" => Ok(AutomationTriggerKind::Interval),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown automation trigger kind {value}"
        ))),
    }
}

fn parse_trigger_kind(trigger_kind: &str) -> Result<TriggerKind, PostgresStoreError> {
    match trigger_kind {
        "manual" => Ok(TriggerKind::Manual),
        "schedule" => Ok(TriggerKind::Schedule),
        "sql" => Ok(TriggerKind::Sql),
        "custom" => Ok(TriggerKind::Custom),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown trigger kind {value}"
        ))),
    }
}

fn parse_artifact_kind(kind: &str) -> Result<ArtifactObjectKind, PostgresStoreError> {
    match kind {
        "bundle" => Ok(ArtifactObjectKind::Bundle),
        "log" => Ok(ArtifactObjectKind::Log),
        "artifact" => Ok(ArtifactObjectKind::Artifact),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown artifact kind {value}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use capsulet_core::{
        ArtifactId, ArtifactObjectKind, Automation, AutomationId, AutomationStatus,
        AutomationTriggerKind, ExecutionPoolName, JobArtifact, JobDefinition, JobRun, JobRunId,
        JobRunLog, JobRunLogRepository, JobRunRepository, WorkflowDefinition, WorkflowId,
        WorkflowStatus,
    };

    use super::{PostgresStore, parse_status};

    fn database_url() -> Option<String> {
        std::env::var("CAPSULET_TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .ok()
    }

    fn unique_id(prefix: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_nanos();
        format!("{prefix}_{nanos}")
    }

    #[test]
    fn parses_known_status() {
        assert!(parse_status("queued").is_ok());
        assert!(parse_status("leased").is_ok());
        assert!(parse_status("not-real").is_err());
    }

    #[tokio::test]
    async fn migrates_and_persists_job_runs_when_database_is_available() {
        let Some(database_url) = database_url() else {
            return;
        };

        let store = PostgresStore::connect(&database_url)
            .await
            .expect("connect to postgres");
        store.migrate().await.expect("run migrations");

        let definition = JobDefinition::hello_python();
        store
            .upsert_job_definition(&definition)
            .await
            .expect("upsert job definition");

        let run = JobRun::new(
            JobRunId::new(unique_id("run_persistence_test")).expect("valid run id"),
            definition.id.clone(),
            ExecutionPoolName::new("mini").expect("valid pool"),
        );
        store.save(&run).await.expect("save run");

        let persisted = store
            .find_by_id(&run.id)
            .await
            .expect("find run")
            .expect("run exists");

        assert_eq!(persisted.id, run.id);
        assert_eq!(persisted.status, run.status);

        let leased = store
            .lease_next_queued_run("worker-test", 60)
            .await
            .expect("lease next run")
            .expect("queued run available");

        assert_eq!(leased.id, run.id);
    }

    #[tokio::test]
    async fn lease_query_does_not_hand_out_same_run_twice_when_database_is_available() {
        let Some(database_url) = database_url() else {
            return;
        };

        let store = PostgresStore::connect(&database_url)
            .await
            .expect("connect to postgres");
        store.migrate().await.expect("run migrations");
        let definition = JobDefinition::hello_python();
        store
            .upsert_job_definition(&definition)
            .await
            .expect("upsert job definition");

        let run = JobRun::new(
            JobRunId::new(unique_id("run_lease_test")).expect("valid run id"),
            definition.id.clone(),
            ExecutionPoolName::new("mini").expect("valid pool"),
        );
        store.save(&run).await.expect("save run");

        let first = store
            .lease_next_queued_run("worker-a", 60)
            .await
            .expect("lease first")
            .expect("run available");
        let second = store
            .lease_next_queued_run("worker-b", 60)
            .await
            .expect("lease second");

        assert_eq!(first.id, run.id);
        assert!(second.is_none());
    }

    #[tokio::test]
    async fn finds_job_definition_when_database_is_available() {
        let Some(database_url) = database_url() else {
            return;
        };

        let store = PostgresStore::connect(&database_url)
            .await
            .expect("connect to postgres");
        store.migrate().await.expect("run migrations");

        let definition = JobDefinition::hello_python();
        store
            .upsert_job_definition(&definition)
            .await
            .expect("upsert job definition");

        let persisted = store
            .find_job_definition(&definition.id)
            .await
            .expect("find definition")
            .expect("definition exists");

        assert_eq!(persisted, definition);
    }

    #[tokio::test]
    async fn saves_and_finds_interval_automation_when_database_is_available() {
        let Some(database_url) = database_url() else {
            return;
        };

        let store = PostgresStore::connect(&database_url)
            .await
            .expect("connect to postgres");
        store.migrate().await.expect("run migrations");

        let workflow = WorkflowDefinition {
            id: WorkflowId::new(unique_id("workflow_automation_test")).expect("workflow id"),
            name: "Automation persistence workflow".to_string(),
            description: String::new(),
            status: WorkflowStatus::Enabled,
            steps: Vec::new(),
        };
        store
            .upsert_workflow(&workflow)
            .await
            .expect("save workflow");

        let automation = Automation {
            id: AutomationId::new(unique_id("automation_interval_test")).expect("automation id"),
            name: "Interval automation".to_string(),
            description: String::new(),
            workflow_id: workflow.id,
            status: AutomationStatus::Enabled,
            trigger_kind: AutomationTriggerKind::Interval,
            interval_seconds: Some(30),
        };
        store
            .upsert_automation(&automation)
            .await
            .expect("save automation");

        let persisted = store
            .find_automation(&automation.id)
            .await
            .expect("find automation")
            .expect("automation exists");

        assert_eq!(persisted.interval_seconds, Some(30));
    }

    #[tokio::test]
    async fn saves_and_finds_job_run_logs_when_database_is_available() {
        let Some(database_url) = database_url() else {
            return;
        };

        let store = PostgresStore::connect(&database_url)
            .await
            .expect("connect to postgres");
        store.migrate().await.expect("run migrations");

        let definition = JobDefinition::hello_python();
        store
            .upsert_job_definition(&definition)
            .await
            .expect("upsert job definition");

        let run = JobRun::new(
            JobRunId::new(unique_id("run_log_test")).expect("valid run id"),
            definition.id.clone(),
            ExecutionPoolName::new("mini").expect("valid pool"),
        );
        store.save(&run).await.expect("save run");

        let log = JobRunLog::new(run.id.clone(), "hello from postgres logs\n").expect("valid log");
        store.save_log(&log).await.expect("save log");

        let persisted = store
            .find_log_by_run_id(&run.id)
            .await
            .expect("find log")
            .expect("log exists");

        assert_eq!(persisted, log);
    }

    #[tokio::test]
    async fn saves_lists_and_finds_artifacts_when_database_is_available() {
        let Some(database_url) = database_url() else {
            return;
        };

        let store = PostgresStore::connect(&database_url)
            .await
            .expect("connect to postgres");
        store.migrate().await.expect("run migrations");

        let definition = JobDefinition::hello_python();
        store
            .upsert_job_definition(&definition)
            .await
            .expect("upsert job definition");

        let run = JobRun::new(
            JobRunId::new(unique_id("run_artifact_test")).expect("valid run id"),
            definition.id.clone(),
            ExecutionPoolName::new("mini").expect("valid pool"),
        );
        let other_run = JobRun::new(
            JobRunId::new(unique_id("run_artifact_other_test")).expect("valid run id"),
            definition.id.clone(),
            ExecutionPoolName::new("mini").expect("valid pool"),
        );
        store.save(&run).await.expect("save run");
        store.save(&other_run).await.expect("save other run");

        let artifact = JobArtifact::new(
            ArtifactId::new(unique_id("artifact_postgres_test")).expect("valid artifact id"),
            run.id.clone(),
            None,
            "report.txt",
            "artifacts/run/report.txt",
            "text/plain",
            12,
            Some("abc123".to_string()),
            ArtifactObjectKind::Artifact,
        )
        .expect("valid artifact");
        store
            .upsert_artifact(&artifact)
            .await
            .expect("save artifact");

        let artifacts = store.list_artifacts(&run.id).await.expect("list artifacts");
        assert_eq!(artifacts, vec![artifact.clone()]);

        let persisted = store
            .find_artifact(&run.id, &artifact.id)
            .await
            .expect("find artifact")
            .expect("artifact exists");
        assert_eq!(persisted, artifact);

        let isolated = store
            .find_artifact(&other_run.id, &artifact.id)
            .await
            .expect("find artifact for other run");
        assert!(isolated.is_none());
    }
}
