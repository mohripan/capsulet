use crate::domain::{JobRun, JobRunId};

/// Repository port for durable job run state.
///
/// Infrastructure crates will implement this against `PostgreSQL` later. Keeping
/// it here makes the application boundary visible without selecting a database
/// client during Sprint 001.
pub trait JobRunRepository {
    type Error;

    /// Persists a job run aggregate.
    ///
    /// # Errors
    ///
    /// Returns the implementation-specific repository error when persistence
    /// fails.
    fn save(&mut self, run: &JobRun) -> Result<(), Self::Error>;

    /// Finds a job run by id.
    ///
    /// # Errors
    ///
    /// Returns the implementation-specific repository error when lookup fails.
    fn find_by_id(&self, id: &JobRunId) -> Result<Option<JobRun>, Self::Error>;
}
