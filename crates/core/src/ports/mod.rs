use crate::domain::{ArtifactId, JobArtifact, JobRun, JobRunId, JobRunLog};
use async_trait::async_trait;

/// Repository port for durable job run state.
///
/// Infrastructure crates implement this against concrete stores such as
/// `PostgreSQL`. Keeping it here preserves the application boundary without
/// leaking database clients into the domain core.
#[async_trait]
pub trait JobRunRepository {
    type Error;

    /// Persists a job run aggregate.
    ///
    /// # Errors
    ///
    /// Returns the implementation-specific repository error when persistence
    /// fails.
    async fn save(&self, run: &JobRun) -> Result<(), Self::Error>;

    /// Finds a job run by id.
    ///
    /// # Errors
    ///
    /// Returns the implementation-specific repository error when lookup fails.
    async fn find_by_id(&self, id: &JobRunId) -> Result<Option<JobRun>, Self::Error>;
}

/// Repository port for bounded job run logs.
///
/// The first implementation stores logs in `PostgreSQL`, but this port keeps
/// callers independent from the storage backend so object storage can replace
/// large-log persistence later.
#[async_trait]
pub trait JobRunLogRepository {
    type Error;

    /// Persists bounded log output for one run.
    ///
    /// # Errors
    ///
    /// Returns the implementation-specific repository error when persistence
    /// fails.
    async fn save_log(&self, log: &JobRunLog) -> Result<(), Self::Error>;

    /// Finds bounded log output by run id.
    ///
    /// # Errors
    ///
    /// Returns the implementation-specific repository error when lookup fails.
    async fn find_log_by_run_id(&self, id: &JobRunId) -> Result<Option<JobRunLog>, Self::Error>;
}

/// Repository port for object-backed job artifacts.
#[async_trait]
pub trait JobArtifactRepository {
    type Error;

    async fn save_artifact(&self, artifact: &JobArtifact) -> Result<(), Self::Error>;
    async fn list_artifacts_by_run(
        &self,
        run_id: &JobRunId,
    ) -> Result<Vec<JobArtifact>, Self::Error>;
    async fn find_artifact_by_run(
        &self,
        run_id: &JobRunId,
        artifact_id: &ArtifactId,
    ) -> Result<Option<JobArtifact>, Self::Error>;
}
