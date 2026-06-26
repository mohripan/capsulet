use async_trait::async_trait;
use capsulet_core::{ArtifactId, JobArtifact, JobRun, JobRunId, JobRunLog};

/// Repository port for durable job run state.
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

    /// Persists artifact metadata.
    ///
    /// # Errors
    ///
    /// Returns the implementation-specific repository error when persistence
    /// fails.
    async fn save_artifact(&self, artifact: &JobArtifact) -> Result<(), Self::Error>;

    /// Lists artifact metadata for a job run.
    ///
    /// # Errors
    ///
    /// Returns the implementation-specific repository error when lookup fails.
    async fn list_artifacts_by_run(
        &self,
        run_id: &JobRunId,
    ) -> Result<Vec<JobArtifact>, Self::Error>;

    /// Finds artifact metadata by run and artifact id.
    ///
    /// # Errors
    ///
    /// Returns the implementation-specific repository error when lookup fails.
    async fn find_artifact_by_run(
        &self,
        run_id: &JobRunId,
        artifact_id: &ArtifactId,
    ) -> Result<Option<JobArtifact>, Self::Error>;
}
