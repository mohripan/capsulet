//! `PostgreSQL` persistence adapter for Capsulet.

use async_trait::async_trait;
use capsulet_core::{
    ArtifactId, ArtifactObjectKind, ExecutionPoolName, JobArtifact, JobArtifactRepository,
    JobAttemptId, JobDefinition, JobDefinitionId, JobRun, JobRunId, JobRunLog, JobRunLogRepository,
    JobRunRepository, JobRunStatus, RetryPolicy,
};
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
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
        ArtifactId, ArtifactObjectKind, ExecutionPoolName, JobArtifact, JobDefinition, JobRun,
        JobRunId, JobRunLog, JobRunLogRepository, JobRunRepository,
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
