use capsulet_core::{ArtifactId, JobArtifact, JobAttemptId, JobRunId};

use crate::{PostgresStore, PostgresStoreError, rows::row_to_job_artifact};

/// Metadata for an artifact produced by a direct workflow prerequisite.
#[derive(Debug, Clone)]
pub struct UpstreamArtifact {
    pub producer_step_id: String,
    pub artifact: JobArtifact,
}

impl PostgresStore {
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
        .bind(artifact.id().as_str())
        .bind(artifact.run_id().as_str())
        .bind(artifact.attempt_id().map(JobAttemptId::as_str))
        .bind(artifact.name())
        .bind(artifact.object_key())
        .bind(artifact.content_type())
        .bind(i64::try_from(artifact.size_bytes()).map_err(|_| {
            PostgresStoreError::InvalidPersistedValue("artifact size is too large".into())
        })?)
        .bind(artifact.checksum_sha256())
        .bind(artifact.kind().as_str())
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

    /// Lists artifacts from successful direct prerequisites of a workflow job run.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or values are invalid.
    pub async fn list_upstream_artifacts(
        &self,
        run_id: &JobRunId,
    ) -> Result<Vec<UpstreamArtifact>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT dependency.from_step_id AS producer_step_id,
                   artifact.id, artifact.job_run_id, artifact.job_attempt_id,
                   artifact.name, artifact.object_key, artifact.content_type,
                   artifact.size_bytes, artifact.checksum_sha256, artifact.kind
            FROM workflow_step_runs current_step
            JOIN workflow_runs workflow_run
              ON workflow_run.id = current_step.workflow_run_id
            JOIN workflow_step_dependencies dependency
              ON dependency.workflow_id = workflow_run.workflow_id
             AND dependency.to_step_id = current_step.workflow_step_id
            JOIN workflow_step_runs upstream_step
              ON upstream_step.workflow_run_id = current_step.workflow_run_id
             AND upstream_step.workflow_step_id = dependency.from_step_id
             AND upstream_step.status = 'succeeded'
            JOIN job_artifacts artifact
              ON artifact.job_run_id = upstream_step.job_run_id
             AND artifact.kind = 'artifact'
            WHERE current_step.job_run_id = $1
            ORDER BY dependency.from_step_id, artifact.name
            ",
        )
        .bind(run_id.as_str())
        .fetch_all(&self.pool)
        .await?;

        rows.iter()
            .map(|row| {
                use sqlx::Row as _;
                Ok(UpstreamArtifact {
                    producer_step_id: row.try_get("producer_step_id")?,
                    artifact: row_to_job_artifact(row)?,
                })
            })
            .collect()
    }
}
