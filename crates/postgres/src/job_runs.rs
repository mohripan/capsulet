use capsulet_core::{JobDefinition, JobRun, JobRunId, JobRunRepository, JobRunStatus};

use crate::{PostgresStore, PostgresStoreError, rows::row_to_job_run};
impl PostgresStore {
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
            SELECT id, job_definition_id, status, execution_pool, input::text AS input, attempt_count, created_at::text AS created_at
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
                heartbeat_at = now(),
                updated_at = now()
            FROM candidate
            WHERE job_runs.id = candidate.id
            RETURNING job_runs.id,
                      job_runs.job_definition_id,
                      job_runs.status,
                      job_runs.execution_pool,
                      job_runs.input::text AS input,
                      job_runs.attempt_count,
                      job_runs.created_at::text AS created_at
            ",
        )
        .bind(worker_id)
        .bind(lease_seconds)
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(row_to_job_run).transpose()
    }

    /// Renews an active lease when it is still owned by the calling worker.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn heartbeat_run(
        &self,
        id: &JobRunId,
        worker_id: &str,
        lease_seconds: i64,
    ) -> Result<bool, PostgresStoreError> {
        let result = sqlx::query(
            r"
            UPDATE job_runs
            SET heartbeat_at = now(),
                lease_expires_at = now() + ($3 * interval '1 second'),
                updated_at = now()
            WHERE id = $1
              AND lease_owner = $2
              AND status IN ('leased', 'running')
            ",
        )
        .bind(id.as_str())
        .bind(worker_id)
        .bind(lease_seconds)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
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
                lease_owner = NULL,
                lease_expires_at = NULL,
                heartbeat_at = NULL,
                retry_ready_at = NULL,
                updated_at = now()
            WHERE id = $1
              AND status IN ('queued', 'leased', 'running', 'retry_scheduled')
            RETURNING id, job_definition_id, status, execution_pool, input::text AS input, attempt_count, created_at::text AS created_at
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
                heartbeat_at = NULL,
                retry_ready_at = now() + ($4 * interval '1 second'),
                updated_at = now()
            WHERE id = $1
              AND attempt_count = $2
              AND status = 'running'
            RETURNING id, job_definition_id, status, execution_pool, input::text AS input, attempt_count, created_at::text AS created_at
            "
        } else {
            r"
            UPDATE job_runs
            SET
                status = $3,
                lease_owner = NULL,
                lease_expires_at = NULL,
                heartbeat_at = NULL,
                retry_ready_at = NULL,
                updated_at = now()
            WHERE id = $1
              AND attempt_count = $2
              AND status = 'running'
            RETURNING id, job_definition_id, status, execution_pool, input::text AS input, attempt_count, created_at::text AS created_at
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
                heartbeat_at = NULL,
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
}
