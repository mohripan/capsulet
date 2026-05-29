use crate::domain::{JobRun, JobRunId};
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
