//! `PostgreSQL` persistence adapter for Capsulet.

use std::{env, time::Duration};

use sqlx::{PgPool, postgres::PgPoolOptions};
use thiserror::Error;

mod artifacts;
mod audit;
mod automations;
mod job_definitions;
mod job_runs;
mod metrics;
mod projects;
mod repositories;
mod retention;
mod rows;
mod service_accounts;
mod trigger_events;
mod workflow_runs;
mod workflows;

pub use artifacts::UpstreamArtifact;
pub use audit::AuditEvent;
pub use projects::{NewProjectMembership, ProjectMembershipRecord, ProjectRecord};
pub use retention::RetentionCandidate;
pub use service_accounts::{NewServiceAccount, ServiceAccountRecord};
pub use trigger_events::{CustomRuntimeTrigger, ScheduleTrigger, TriggerEvent};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

#[cfg(test)]
mod tests;

/// `PostgreSQL`-backed store for Capsulet persistence.
#[derive(Debug, Clone)]
pub struct PostgresStore {
    pub(crate) pool: PgPool,
}

/// `PostgreSQL` connection-pool settings.
#[derive(Debug, Clone, Copy)]
pub struct PostgresPoolConfig {
    pub max_connections: u32,
    pub acquire_timeout: Duration,
    pub statement_timeout: Duration,
}

impl Default for PostgresPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 5,
            acquire_timeout: Duration::from_secs(30),
            statement_timeout: Duration::from_secs(30),
        }
    }
}

impl PostgresPoolConfig {
    /// Loads pool settings from environment variables.
    ///
    /// Supported variables:
    /// - `CAPSULET_DB_MAX_CONNECTIONS`
    /// - `CAPSULET_DB_ACQUIRE_TIMEOUT_SECONDS`
    /// - `CAPSULET_DB_STATEMENT_TIMEOUT_SECONDS`
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when a configured value is invalid.
    pub fn from_env() -> Result<Self, PostgresStoreError> {
        let default = Self::default();
        Ok(Self {
            max_connections: env_positive_u32(
                "CAPSULET_DB_MAX_CONNECTIONS",
                default.max_connections,
            )?,
            acquire_timeout: Duration::from_secs(env_positive_u64(
                "CAPSULET_DB_ACQUIRE_TIMEOUT_SECONDS",
                default.acquire_timeout.as_secs(),
            )?),
            statement_timeout: Duration::from_secs(env_positive_u64(
                "CAPSULET_DB_STATEMENT_TIMEOUT_SECONDS",
                default.statement_timeout.as_secs(),
            )?),
        })
    }
}

impl PostgresStore {
    /// Connects to `PostgreSQL`.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the connection pool cannot be
    /// created.
    pub async fn connect(database_url: &str) -> Result<Self, PostgresStoreError> {
        Self::connect_with_config(database_url, PostgresPoolConfig::default()).await
    }

    /// Connects to `PostgreSQL` using an explicit pool configuration.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the connection pool cannot be
    /// created or configured.
    pub async fn connect_with_config(
        database_url: &str,
        config: PostgresPoolConfig,
    ) -> Result<Self, PostgresStoreError> {
        let statement_timeout = format!("{}ms", config.statement_timeout.as_millis());
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(config.acquire_timeout)
            .after_connect(move |connection, _metadata| {
                let statement_timeout = statement_timeout.clone();
                Box::pin(async move {
                    sqlx::query("SELECT set_config('statement_timeout', $1, false)")
                        .bind(statement_timeout)
                        .execute(connection)
                        .await?;
                    Ok(())
                })
            })
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

    /// Verifies that the database can serve a query.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when `PostgreSQL` is unavailable.
    pub async fn ping(&self) -> Result<(), PostgresStoreError> {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(())
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
    #[error("invalid postgres pool configuration: {0}")]
    InvalidPoolConfig(String),
    #[error("invalid workflow graph: {0}")]
    WorkflowGraph(#[from] capsulet_core::WorkflowGraphError),
}

fn env_positive_u32(name: &str, default: u32) -> Result<u32, PostgresStoreError> {
    let Some(value) = env::var(name).ok() else {
        return Ok(default);
    };
    let parsed = value
        .parse::<u32>()
        .map_err(|_| PostgresStoreError::InvalidPoolConfig(format!("{name} must be an integer")))?;
    if parsed == 0 {
        return Err(PostgresStoreError::InvalidPoolConfig(format!(
            "{name} must be greater than zero"
        )));
    }
    Ok(parsed)
}

fn env_positive_u64(name: &str, default: u64) -> Result<u64, PostgresStoreError> {
    let Some(value) = env::var(name).ok() else {
        return Ok(default);
    };
    let parsed = value
        .parse::<u64>()
        .map_err(|_| PostgresStoreError::InvalidPoolConfig(format!("{name} must be an integer")))?;
    if parsed == 0 {
        return Err(PostgresStoreError::InvalidPoolConfig(format!(
            "{name} must be greater than zero"
        )));
    }
    Ok(parsed)
}
