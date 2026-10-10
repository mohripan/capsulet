use capsulet_core::{JobDefinition, JobDefinitionId};

use crate::{PostgresStore, PostgresStoreError, rows::row_to_job_definition};
impl PostgresStore {
    /// Deletes a job definition only in the selected project.
    ///
    /// # Errors
    /// Returns [`PostgresStoreError`] when deletion fails.
    pub async fn delete_job_definition_in_project(
        &self,
        id: &JobDefinitionId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<bool, PostgresStoreError> {
        let result = sqlx::query(
            "DELETE FROM job_definitions WHERE id = $1 AND tenant_id = $2 AND project_id = $3",
        )
        .bind(id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
    /// Finds a job definition in an explicitly selected project.
    ///
    /// # Errors
    /// Returns [`PostgresStoreError`] when lookup fails.
    pub async fn find_job_definition_in_project(
        &self,
        id: &JobDefinitionId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<JobDefinition>, PostgresStoreError> {
        let row = sqlx::query(
            r"SELECT id, name, runtime_image, command, python_dependencies, bundle_object_key,
                     input_schema::text AS input_schema, retry_max_attempts, retry_delay_seconds
              FROM job_definitions WHERE id = $1 AND tenant_id = $2 AND project_id = $3",
        )
        .bind(id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_job_definition).transpose()
    }
    /// Lists job definitions visible in a selected project.
    ///
    /// # Errors
    /// Returns [`PostgresStoreError`] when lookup fails.
    pub async fn list_job_definitions_in_project(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<JobDefinition>, PostgresStoreError> {
        let rows = sqlx::query(
            r"SELECT id, name, runtime_image, command, python_dependencies, bundle_object_key,
                     input_schema::text AS input_schema, retry_max_attempts, retry_delay_seconds
              FROM job_definitions
              WHERE tenant_id = $1 AND project_id = $2
              ORDER BY updated_at DESC, id ASC LIMIT $3",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_job_definition).collect()
    }
    /// Saves a job definition for an explicitly selected project.
    ///
    /// # Errors
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn upsert_job_definition_in_project(
        &self,
        definition: &JobDefinition,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), PostgresStoreError> {
        let result = sqlx::query(
            r"
            INSERT INTO job_definitions (
                id, name, runtime_image, command, python_dependencies, bundle_object_key,
                input_schema, retry_max_attempts, retry_delay_seconds, tenant_id, project_id, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb, $8, $9, $10, $11, now())
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                runtime_image = EXCLUDED.runtime_image,
                command = EXCLUDED.command,
                python_dependencies = EXCLUDED.python_dependencies,
                bundle_object_key = EXCLUDED.bundle_object_key,
                input_schema = EXCLUDED.input_schema,
                retry_max_attempts = EXCLUDED.retry_max_attempts,
                retry_delay_seconds = EXCLUDED.retry_delay_seconds,
                updated_at = now()
            WHERE job_definitions.tenant_id = EXCLUDED.tenant_id
              AND job_definitions.project_id = EXCLUDED.project_id
            ",
        )
        .bind(definition.id().as_str())
        .bind(definition.name())
        .bind(definition.runtime_image())
        .bind(definition.command())
        .bind(definition.python_dependencies())
        .bind(definition.bundle_object_key())
        .bind(definition.input_schema())
        .bind(i32::try_from(definition.retry_max_attempts()).map_err(|_| PostgresStoreError::AttemptOverflow)?)
        .bind(i32::try_from(definition.retry_delay_seconds()).map_err(|_| {
            PostgresStoreError::InvalidPersistedValue("retry delay is too large".into())
        })?)
        .bind(tenant_id)
        .bind(project_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(PostgresStoreError::ScopeMismatch);
        }
        Ok(())
    }
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn upsert_job_definition(
        &self,
        definition: &JobDefinition,
    ) -> Result<(), PostgresStoreError> {
        self.upsert_job_definition_in_project(definition, "default", "default")
            .await
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
                   python_dependencies,
                   bundle_object_key,
                   input_schema::text AS input_schema,
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
                   python_dependencies,
                   bundle_object_key,
                   input_schema::text AS input_schema,
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
}
