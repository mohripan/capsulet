use capsulet_core::{WorkflowDefinition, WorkflowId};
use sqlx::Row;

use crate::{
    PostgresStore, PostgresStoreError,
    rows::{parse_workflow_status, row_to_workflow_step},
};
impl PostgresStore {
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
}
