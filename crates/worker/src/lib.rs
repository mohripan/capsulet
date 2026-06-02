use async_trait::async_trait;
use capsulet_core::{JobRun, JobRunRepository, JobRunStatus};
use capsulet_postgres::{PostgresStore, PostgresStoreError};
use capsulet_runner::{RunOutcome, Runner};
use thiserror::Error;

/// Storage operations required by the worker lease-and-run path.
#[async_trait]
pub trait WorkerStore: Clone + Send + Sync + 'static {
    type Error: std::fmt::Display + Send + Sync + 'static;

    /// Leases the next queued run for a worker.
    ///
    /// # Errors
    ///
    /// Returns an implementation-specific persistence error.
    async fn lease_next_queued_run(
        &self,
        worker_id: &str,
        lease_seconds: i64,
    ) -> Result<Option<JobRun>, Self::Error>;

    /// Saves a job run after worker-side state changes.
    ///
    /// # Errors
    ///
    /// Returns an implementation-specific persistence error.
    async fn save_run(&self, run: &JobRun) -> Result<(), Self::Error>;
}

#[async_trait]
impl WorkerStore for PostgresStore {
    type Error = PostgresStoreError;

    async fn lease_next_queued_run(
        &self,
        worker_id: &str,
        lease_seconds: i64,
    ) -> Result<Option<JobRun>, Self::Error> {
        self.lease_next_queued_run(worker_id, lease_seconds).await
    }

    async fn save_run(&self, run: &JobRun) -> Result<(), Self::Error> {
        self.save(run).await
    }
}

/// Outcome of a single worker tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerTickOutcome {
    NoRunAvailable,
    RunSucceeded,
    RunFailed,
}

/// Executes one queued run if one is available.
///
/// # Errors
///
/// Returns [`WorkerError`] when persistence, state transition, or execution
/// fails.
pub async fn execute_one_queued_run<S, R>(
    store: &S,
    runner: &R,
    worker_id: &str,
    lease_seconds: i64,
) -> Result<WorkerTickOutcome, WorkerError>
where
    S: WorkerStore,
    R: Runner,
{
    let Some(mut run) = store
        .lease_next_queued_run(worker_id, lease_seconds)
        .await
        .map_err(WorkerError::store)?
    else {
        return Ok(WorkerTickOutcome::NoRunAvailable);
    };

    run.record_attempt_started()
        .map_err(|error| WorkerError::InvalidState(error.to_string()))?;
    store.save_run(&run).await.map_err(WorkerError::store)?;

    match runner.execute(&run).await.map_err(WorkerError::runner)? {
        RunOutcome::Succeeded => {
            run.transition_to(JobRunStatus::Succeeded)
                .map_err(|error| WorkerError::InvalidState(error.to_string()))?;
            store.save_run(&run).await.map_err(WorkerError::store)?;
            Ok(WorkerTickOutcome::RunSucceeded)
        }
        RunOutcome::Failed => {
            run.transition_to(JobRunStatus::Failed)
                .map_err(|error| WorkerError::InvalidState(error.to_string()))?;
            store.save_run(&run).await.map_err(WorkerError::store)?;
            Ok(WorkerTickOutcome::RunFailed)
        }
    }
}

/// Worker use-case error.
#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("store error: {0}")]
    Store(String),
    #[error("runner error: {0}")]
    Runner(String),
    #[error("invalid job run state: {0}")]
    InvalidState(String),
}

impl WorkerError {
    fn store(error: impl std::fmt::Display) -> Self {
        Self::Store(error.to_string())
    }

    fn runner(error: impl std::fmt::Display) -> Self {
        Self::Runner(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use capsulet_core::{ExecutionPoolName, JobDefinitionId, JobRun, JobRunId};
    use capsulet_runner::StubRunner;

    use super::{WorkerStore, WorkerTickOutcome, execute_one_queued_run};

    #[derive(Debug, Clone, Default)]
    struct FakeStore {
        queued: Arc<Mutex<Vec<JobRun>>>,
        saved: Arc<Mutex<Vec<JobRun>>>,
    }

    #[async_trait::async_trait]
    impl WorkerStore for FakeStore {
        type Error = String;

        async fn lease_next_queued_run(
            &self,
            _worker_id: &str,
            _lease_seconds: i64,
        ) -> Result<Option<JobRun>, Self::Error> {
            let mut run = self.queued.lock().map_err(|error| error.to_string())?.pop();
            if let Some(run) = &mut run {
                run.transition_to(capsulet_core::JobRunStatus::Leased)
                    .map_err(|error| error.to_string())?;
            }
            Ok(run)
        }

        async fn save_run(&self, run: &JobRun) -> Result<(), Self::Error> {
            self.saved
                .lock()
                .map_err(|error| error.to_string())?
                .push(run.clone());
            Ok(())
        }
    }

    impl FakeStore {
        fn with_run(run: JobRun) -> Self {
            Self {
                queued: Arc::new(Mutex::new(vec![run])),
                saved: Arc::default(),
            }
        }

        fn saved_runs(&self) -> Vec<JobRun> {
            self.saved.lock().expect("saved mutex").clone()
        }
    }

    fn run() -> JobRun {
        JobRun::new(
            JobRunId::new("run_1").expect("valid run id"),
            JobDefinitionId::new("job_hello_python").expect("valid definition id"),
            ExecutionPoolName::new("mini").expect("valid pool"),
        )
    }

    #[tokio::test]
    async fn returns_no_run_when_queue_is_empty() {
        let outcome =
            execute_one_queued_run(&FakeStore::default(), &StubRunner::success(), "w1", 60)
                .await
                .expect("worker tick");

        assert_eq!(outcome, WorkerTickOutcome::NoRunAvailable);
    }

    #[tokio::test]
    async fn stub_success_runner_completes_run() {
        let store = FakeStore::with_run(run());

        let outcome = execute_one_queued_run(&store, &StubRunner::success(), "w1", 60)
            .await
            .expect("worker tick");

        assert_eq!(outcome, WorkerTickOutcome::RunSucceeded);
        let saved = store.saved_runs();
        assert_eq!(saved[0].status, capsulet_core::JobRunStatus::Running);
        assert_eq!(saved[0].attempt_count, 1);
        assert_eq!(saved[1].status, capsulet_core::JobRunStatus::Succeeded);
    }

    #[tokio::test]
    async fn stub_failure_runner_fails_run() {
        let store = FakeStore::with_run(run());

        let outcome = execute_one_queued_run(&store, &StubRunner::failure(), "w1", 60)
            .await
            .expect("worker tick");

        assert_eq!(outcome, WorkerTickOutcome::RunFailed);
        let saved = store.saved_runs();
        assert_eq!(saved[0].status, capsulet_core::JobRunStatus::Running);
        assert_eq!(saved[1].status, capsulet_core::JobRunStatus::Failed);
    }
}
