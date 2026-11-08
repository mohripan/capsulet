//! Job runs, their logs and their artifacts, read and changed within one project.
//!
//! The API answers for one project at a time, so every query here carries the tenant
//! and project in its `WHERE` clause. A run another project owns matches no row,
//! which is the same answer as a run that does not exist. The unscoped reads the
//! workers use stay where they were; a worker is handed a run, it does not look one
//! up on somebody's behalf.

use capsulet_core::{ArtifactId, JobArtifact, JobRun, JobRunId, JobRunLog};

use crate::rows::{row_to_job_artifact, row_to_job_run, row_to_job_run_log};
use crate::{PostgresStore, PostgresStoreError};

const JOB_RUN_COLUMNS: &str = "id, job_definition_id, status, execution_pool, input::text AS input, \
     attempt_count, created_at::text AS created_at";

const ARTIFACT_COLUMNS: &str = "artifacts.id, artifacts.job_run_id, artifacts.job_attempt_id, \
     artifacts.name, artifacts.object_key, artifacts.content_type, artifacts.size_bytes, \
     artifacts.checksum_sha256, artifacts.kind";

impl PostgresStore {
    /// Finds a job run the project owns.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails or the row is invalid.
    pub async fn find_job_run_in_project(
        &self,
        id: &JobRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<JobRun>, PostgresStoreError> {
        let row = sqlx::query(&format!(
            "SELECT {JOB_RUN_COLUMNS} FROM job_runs \
             WHERE id = $1 AND tenant_id = $2 AND project_id = $3"
        ))
        .bind(id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_job_run).transpose()
    }

    /// Finds the log of a job run the project owns.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails or the row is invalid.
    pub async fn find_job_run_log_in_project(
        &self,
        id: &JobRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<JobRunLog>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT logs.job_run_id, logs.log_text
            FROM job_run_logs logs
            JOIN job_runs runs ON runs.id = logs.job_run_id
            WHERE logs.job_run_id = $1 AND runs.tenant_id = $2 AND runs.project_id = $3
            ",
        )
        .bind(id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_job_run_log).transpose()
    }

    /// Cancels a job run the project owns, if it has not finished, and returns how it
    /// stands. `None` when the project has no such run.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the write fails.
    pub async fn cancel_job_run_in_project(
        &self,
        id: &JobRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<JobRun>, PostgresStoreError> {
        let row = sqlx::query(&format!(
            "UPDATE job_runs \
             SET status = 'cancelled', lease_owner = NULL, lease_expires_at = NULL, \
                 heartbeat_at = NULL, retry_ready_at = NULL, updated_at = now() \
             WHERE id = $1 AND tenant_id = $2 AND project_id = $3 \
               AND status IN ('queued', 'leased', 'running', 'retry_scheduled') \
             RETURNING {JOB_RUN_COLUMNS}"
        ))
        .bind(id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?;
        if let Some(row) = row {
            return row_to_job_run(&row).map(Some);
        }
        self.find_job_run_in_project(id, tenant_id, project_id)
            .await
    }

    /// Lists the artifacts of a job run the project owns.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails or a row is invalid.
    pub async fn list_job_artifacts_in_project(
        &self,
        run_id: &JobRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<JobArtifact>, PostgresStoreError> {
        let rows = sqlx::query(&format!(
            "SELECT {ARTIFACT_COLUMNS} FROM job_artifacts artifacts \
             JOIN job_runs runs ON runs.id = artifacts.job_run_id \
             WHERE artifacts.job_run_id = $1 AND runs.tenant_id = $2 AND runs.project_id = $3 \
             ORDER BY artifacts.created_at, artifacts.name"
        ))
        .bind(run_id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_job_artifact).collect()
    }

    /// Finds one artifact of a job run the project owns.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails or the row is invalid.
    pub async fn find_job_artifact_in_project(
        &self,
        run_id: &JobRunId,
        artifact_id: &ArtifactId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<JobArtifact>, PostgresStoreError> {
        let row = sqlx::query(&format!(
            "SELECT {ARTIFACT_COLUMNS} FROM job_artifacts artifacts \
             JOIN job_runs runs ON runs.id = artifacts.job_run_id \
             WHERE artifacts.job_run_id = $1 AND artifacts.id = $2 \
               AND runs.tenant_id = $3 AND runs.project_id = $4"
        ))
        .bind(run_id.as_str())
        .bind(artifact_id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_job_artifact).transpose()
    }
}
