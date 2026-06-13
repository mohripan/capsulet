//! `PostgreSQL` persistence adapter for Capsulet.

use sqlx::{PgPool, postgres::PgPoolOptions};
use thiserror::Error;

mod artifacts;
mod automations;
mod job_definitions;
mod job_runs;
mod repositories;
mod rows;
mod workflow_runs;
mod workflows;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

#[cfg(test)]
mod tests;

/// `PostgreSQL`-backed store for Capsulet persistence.
#[derive(Debug, Clone)]
pub struct PostgresStore {
    pub(crate) pool: PgPool,
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
