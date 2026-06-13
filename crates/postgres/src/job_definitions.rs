use capsulet_core::{JobDefinition, JobDefinitionId};

use crate::{PostgresStore, PostgresStoreError, rows::row_to_job_definition};
impl PostgresStore {
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
}
