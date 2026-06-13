use async_trait::async_trait;
use capsulet_core::{
    ArtifactId, JobArtifact, JobArtifactRepository, JobRun, JobRunId, JobRunLog,
    JobRunLogRepository, JobRunRepository,
};

use crate::{
    PostgresStore, PostgresStoreError,
    rows::{row_to_job_run, row_to_job_run_log},
};

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
                input,
                attempt_count,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5::jsonb, $6, now())
            ON CONFLICT (id) DO UPDATE SET
                status = EXCLUDED.status,
                execution_pool = EXCLUDED.execution_pool,
                input = EXCLUDED.input,
                attempt_count = EXCLUDED.attempt_count,
                updated_at = now()
            ",
        )
        .bind(run.id.as_str())
        .bind(run.job_definition_id.as_str())
        .bind(run.status.to_string())
        .bind(run.execution_pool.as_str())
        .bind(&run.input_json)
        .bind(i32::try_from(run.attempt_count).map_err(|_| PostgresStoreError::AttemptOverflow)?)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_by_id(&self, id: &JobRunId) -> Result<Option<JobRun>, Self::Error> {
        let row = sqlx::query(
            r"
            SELECT id, job_definition_id, status, execution_pool, input::text AS input, attempt_count
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
