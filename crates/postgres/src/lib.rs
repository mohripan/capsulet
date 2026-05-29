//! `PostgreSQL` persistence adapter for Capsulet.

use async_trait::async_trait;
use capsulet_core::{
    ExecutionPoolName, JobDefinitionId, JobRun, JobRunId, JobRunRepository, JobRunStatus,
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
        definition: &JobDefinitionRecord,
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
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6::jsonb, now())
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                runtime_image = EXCLUDED.runtime_image,
                command = EXCLUDED.command,
                bundle_object_key = EXCLUDED.bundle_object_key,
                input_schema = EXCLUDED.input_schema,
                updated_at = now()
            ",
        )
        .bind(definition.id.as_str())
        .bind(&definition.name)
        .bind(&definition.runtime_image)
        .bind(&definition.command)
        .bind(&definition.bundle_object_key)
        .bind(&definition.input_schema)
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

/// Minimal persisted job definition record for Sprint 002.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobDefinitionRecord {
    pub id: JobDefinitionId,
    pub name: String,
    pub runtime_image: String,
    pub command: Vec<String>,
    pub bundle_object_key: String,
    pub input_schema: String,
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

#[cfg(test)]
mod tests {
    use capsulet_core::{ExecutionPoolName, JobDefinitionId, JobRun, JobRunId, JobRunRepository};

    use super::{JobDefinitionRecord, PostgresStore, parse_status};

    fn database_url() -> Option<String> {
        std::env::var("CAPSULET_TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .ok()
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

        let definition = JobDefinitionRecord {
            id: JobDefinitionId::new("job_hello_python").expect("valid job id"),
            name: "Hello Python".to_string(),
            runtime_image: "python:3.12-slim".to_string(),
            command: vec!["python".to_string(), "/workspace/main.py".to_string()],
            bundle_object_key: "bundles/job_hello_python.tar.gz".to_string(),
            input_schema: "{}".to_string(),
        };
        store
            .upsert_job_definition(&definition)
            .await
            .expect("upsert job definition");

        let run = JobRun::new(
            JobRunId::new("run_persistence_test").expect("valid run id"),
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
}
