use capsulet_core::{
    WorkflowDefinition, WorkflowGraph, WorkflowId, WorkflowStepDependency, WorkflowStepId,
};
use sqlx::Row;

use crate::{
    PostgresStore, PostgresStoreError,
    rows::{parse_domain_value, row_to_workflow_step},
};
impl PostgresStore {
    /// Deletes a workflow only in the selected project.
    ///
    /// # Errors
    /// Returns [`PostgresStoreError`] when deletion fails.
    pub async fn delete_workflow_in_project(
        &self,
        id: &WorkflowId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<bool, PostgresStoreError> {
        let result = sqlx::query(
            "DELETE FROM workflow_definitions WHERE id = $1 AND tenant_id = $2 AND project_id = $3",
        )
        .bind(id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
    /// Finds a workflow in a selected project.
    ///
    /// # Errors
    /// Returns [`PostgresStoreError`] when lookup fails.
    pub async fn find_workflow_in_project(
        &self,
        id: &WorkflowId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<WorkflowDefinition>, PostgresStoreError> {
        let row = sqlx::query(
            r"SELECT id, name, description, status, deadline_seconds FROM workflow_definitions
              WHERE id = $1 AND tenant_id = $2 AND project_id = $3",
        )
        .bind(id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(row) => Ok(Some(self.workflow_from_row(&row).await?)),
            None => Ok(None),
        }
    }
    /// Lists workflows visible in a selected project.
    ///
    /// # Errors
    /// Returns [`PostgresStoreError`] when lookup fails.
    pub async fn list_workflows_in_project(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<WorkflowDefinition>, PostgresStoreError> {
        let rows = sqlx::query(
            r"SELECT id, name, description, status, deadline_seconds FROM workflow_definitions
              WHERE tenant_id = $1 AND project_id = $2
              ORDER BY updated_at DESC, id ASC LIMIT $3",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let mut workflows = Vec::with_capacity(rows.len());
        for row in rows {
            workflows.push(self.workflow_from_row(&row).await?);
        }
        Ok(workflows)
    }
    /// Saves a workflow in the default bootstrap project.
    ///
    /// # Errors
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn upsert_workflow(
        &self,
        workflow: &WorkflowDefinition,
    ) -> Result<(), PostgresStoreError> {
        self.upsert_workflow_in_project(workflow, "default", "default")
            .await
    }
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn upsert_workflow_in_project(
        &self,
        workflow: &WorkflowDefinition,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), PostgresStoreError> {
        WorkflowGraph::new(workflow.id(), workflow.steps(), workflow.dependencies())?;
        let mut tx = self.pool.begin().await?;
        lock_workflow_jobs(&mut tx, workflow, tenant_id, project_id).await?;
        let result = sqlx::query(
            r"
            INSERT INTO workflow_definitions (id, name, description, status, deadline_seconds, tenant_id, project_id, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, now())
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                status = EXCLUDED.status,
                deadline_seconds = EXCLUDED.deadline_seconds,
                updated_at = now()
            WHERE workflow_definitions.tenant_id = EXCLUDED.tenant_id
              AND workflow_definitions.project_id = EXCLUDED.project_id
            ",
        )
        .bind(workflow.id().as_str())
        .bind(workflow.name())
        .bind(workflow.description())
        .bind(workflow.status().to_string())
        .bind(workflow.deadline_seconds().and_then(|value| i64::try_from(value).ok()))
        .bind(tenant_id)
        .bind(project_id)
        .execute(&mut *tx)
        .await?;

        if result.rows_affected() == 0 {
            return Err(PostgresStoreError::ScopeMismatch);
        }

        sqlx::query("DELETE FROM workflow_step_dependencies WHERE workflow_id = $1")
            .bind(workflow.id().as_str())
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM workflow_steps WHERE workflow_id = $1")
            .bind(workflow.id().as_str())
            .execute(&mut *tx)
            .await?;

        for step in workflow.steps() {
            sqlx::query(
                r"
                INSERT INTO workflow_steps (
                    id, workflow_id, position, name, job_definition_id, execution_pool, timeout_seconds, updated_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, now())
                ",
            )
            .bind(step.id().as_str())
            .bind(workflow.id().as_str())
            .bind(step.position())
            .bind(step.name())
            .bind(step.job_definition_id().as_str())
            .bind(step.execution_pool().as_str())
            .bind(step.timeout_seconds().and_then(|value| i64::try_from(value).ok()))
            .execute(&mut *tx)
            .await?;
        }

        for dependency in workflow.dependencies() {
            sqlx::query(
                "INSERT INTO workflow_step_dependencies (workflow_id, from_step_id, to_step_id, policy) VALUES ($1, $2, $3, $4)",
            )
            .bind(workflow.id().as_str())
            .bind(dependency.from_step_id().as_str())
            .bind(dependency.to_step_id().as_str())
            .bind(dependency.policy().to_string())
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
            SELECT id, name, description, status, deadline_seconds
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
            SELECT id, name, description, status, deadline_seconds
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

    /// Deletes a workflow definition and its dependent steps.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when deletion fails.
    pub async fn delete_workflow(&self, id: &WorkflowId) -> Result<bool, PostgresStoreError> {
        let result = sqlx::query("DELETE FROM workflow_definitions WHERE id = $1")
            .bind(id.as_str())
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn workflow_from_row(
        &self,
        row: &sqlx::postgres::PgRow,
    ) -> Result<WorkflowDefinition, PostgresStoreError> {
        let id: String = row.try_get("id")?;
        let workflow_id = WorkflowId::new(id).map_err(PostgresStoreError::InvalidPersistedValue)?;
        let step_rows = sqlx::query(
            r"
            SELECT id, workflow_id, position, name, job_definition_id, execution_pool, timeout_seconds
            FROM workflow_steps
            WHERE workflow_id = $1
            ORDER BY position ASC
            ",
        )
        .bind(workflow_id.as_str())
        .fetch_all(&self.pool)
        .await?;

        let dependency_rows = sqlx::query(
            r"
            SELECT from_step_id, to_step_id, policy
            FROM workflow_step_dependencies
            WHERE workflow_id = $1
            ORDER BY from_step_id ASC, to_step_id ASC
            ",
        )
        .bind(workflow_id.as_str())
        .fetch_all(&self.pool)
        .await?;

        let dependencies = dependency_rows
            .iter()
            .map(|dependency| {
                Ok(WorkflowStepDependency::with_policy(
                    WorkflowStepId::new(dependency.try_get::<String, _>("from_step_id")?)
                        .map_err(PostgresStoreError::InvalidPersistedValue)?,
                    WorkflowStepId::new(dependency.try_get::<String, _>("to_step_id")?)
                        .map_err(PostgresStoreError::InvalidPersistedValue)?,
                    parse_domain_value(dependency.try_get::<String, _>("policy")?.as_str())?,
                ))
            })
            .collect::<Result<Vec<_>, PostgresStoreError>>()?;

        let deadline_seconds = row
            .try_get::<Option<i64>, _>("deadline_seconds")
            .unwrap_or(None)
            .map(u64::try_from)
            .transpose()
            .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?;
        let workflow = WorkflowDefinition::with_dependencies(
            workflow_id.clone(),
            row.try_get::<String, _>("name")?,
            row.try_get::<String, _>("description")?,
            parse_domain_value(row.try_get::<String, _>("status")?.as_str())?,
            step_rows
                .iter()
                .map(row_to_workflow_step)
                .collect::<Result<Vec<_>, _>>()?,
            dependencies,
        )
        .with_deadline_seconds(deadline_seconds);
        WorkflowGraph::new(&workflow_id, workflow.steps(), workflow.dependencies())?;
        Ok(workflow)
    }
}

async fn lock_workflow_jobs(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workflow: &WorkflowDefinition,
    tenant_id: &str,
    project_id: &str,
) -> Result<(), PostgresStoreError> {
    // Hold the referenced rows through commit: a preflight API check does not protect
    // against concurrent deletion or a legacy ownership update.
    for step in workflow.steps() {
        let found = sqlx::query_scalar::<_, i32>(
            "SELECT 1 FROM job_definitions WHERE id = $1 AND tenant_id = $2 AND project_id = $3 FOR SHARE",
        )
        .bind(step.job_definition_id().as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&mut **tx)
        .await?;
        if found.is_none() {
            return Err(PostgresStoreError::ScopeMismatch);
        }
    }
    Ok(())
}
