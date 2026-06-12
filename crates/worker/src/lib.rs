use async_trait::async_trait;
use capsulet_core::{
    ArtifactId, ArtifactObjectKind, JobArtifact, JobDefinition, JobDefinitionId, JobRun, JobRunLog,
    JobRunLogRepository, JobRunRepository, JobRunStatus,
};
use capsulet_postgres::{PostgresStore, PostgresStoreError};
use capsulet_runner::{CancellationCheck, ExecutionPoolsConfig, RunExecution, RunOutcome, Runner};
use capsulet_storage::{ObjectStore, run_object_key};
use thiserror::Error;

pub mod runtime;

const INLINE_LOG_LIMIT_BYTES: usize = 64 * 1024;

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

    /// Finds the job definition required to execute a run.
    ///
    /// # Errors
    ///
    /// Returns an implementation-specific persistence error.
    async fn find_job_definition(
        &self,
        id: &JobDefinitionId,
    ) -> Result<Option<JobDefinition>, Self::Error>;

    /// Saves bounded logs captured for a run.
    ///
    /// # Errors
    ///
    /// Returns an implementation-specific persistence error.
    async fn save_log(&self, log: &JobRunLog) -> Result<(), Self::Error>;
    async fn save_artifact(&self, artifact: &JobArtifact) -> Result<(), Self::Error>;

    async fn find_run(&self, id: &capsulet_core::JobRunId) -> Result<Option<JobRun>, Self::Error>;

    async fn finish_running_attempt(
        &self,
        id: &capsulet_core::JobRunId,
        attempt_count: u32,
        status: JobRunStatus,
        retry_delay_seconds: Option<u64>,
    ) -> Result<Option<JobRun>, Self::Error>;

    async fn promote_ready_retries(&self) -> Result<u64, Self::Error>;

    async fn recover_expired_leases(&self) -> Result<u64, Self::Error>;

    async fn is_run_cancelled(&self, id: &capsulet_core::JobRunId) -> Result<bool, Self::Error>;
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

    async fn find_job_definition(
        &self,
        id: &JobDefinitionId,
    ) -> Result<Option<JobDefinition>, Self::Error> {
        self.find_job_definition(id).await
    }

    async fn save_log(&self, log: &JobRunLog) -> Result<(), Self::Error> {
        JobRunLogRepository::save_log(self, log).await
    }

    async fn save_artifact(&self, artifact: &JobArtifact) -> Result<(), Self::Error> {
        capsulet_core::JobArtifactRepository::save_artifact(self, artifact).await
    }

    async fn find_run(&self, id: &capsulet_core::JobRunId) -> Result<Option<JobRun>, Self::Error> {
        self.find_by_id(id).await
    }

    async fn finish_running_attempt(
        &self,
        id: &capsulet_core::JobRunId,
        attempt_count: u32,
        status: JobRunStatus,
        retry_delay_seconds: Option<u64>,
    ) -> Result<Option<JobRun>, Self::Error> {
        self.finish_running_attempt(id, attempt_count, status, retry_delay_seconds)
            .await
    }

    async fn promote_ready_retries(&self) -> Result<u64, Self::Error> {
        self.promote_ready_retries().await
    }

    async fn recover_expired_leases(&self) -> Result<u64, Self::Error> {
        self.recover_expired_leases().await
    }

    async fn is_run_cancelled(&self, id: &capsulet_core::JobRunId) -> Result<bool, Self::Error> {
        self.is_run_cancelled(id).await
    }
}

/// Outcome of a single worker tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerTickOutcome {
    NoRunAvailable,
    RunSucceeded,
    RunFailed,
    RunTimedOut,
    RunCancelled,
    RunRetryScheduled,
}

/// Executes one queued run if one is available.
///
/// # Errors
///
/// Returns [`WorkerError`] when persistence, state transition, or execution
/// fails.
pub async fn execute_one_queued_run<S, R, O>(
    store: &S,
    runner: &R,
    object_store: &O,
    pools: &ExecutionPoolsConfig,
    worker_id: &str,
    lease_seconds: i64,
) -> Result<WorkerTickOutcome, WorkerError>
where
    S: WorkerStore,
    R: Runner,
    O: ObjectStore,
{
    store
        .recover_expired_leases()
        .await
        .map_err(WorkerError::store)?;
    store
        .promote_ready_retries()
        .await
        .map_err(WorkerError::store)?;

    let Some(mut run) = store
        .lease_next_queued_run(worker_id, lease_seconds)
        .await
        .map_err(WorkerError::store)?
    else {
        return Ok(WorkerTickOutcome::NoRunAvailable);
    };

    let definition = store
        .find_job_definition(&run.job_definition_id)
        .await
        .map_err(WorkerError::store)?
        .ok_or_else(|| WorkerError::MissingJobDefinition(run.job_definition_id.to_string()))?;
    let pool = pools
        .find(run.execution_pool.as_str())
        .cloned()
        .ok_or_else(|| WorkerError::MissingExecutionPool(run.execution_pool.to_string()))?;

    run.record_attempt_started()
        .map_err(|error| WorkerError::InvalidState(error.to_string()))?;
    store.save_run(&run).await.map_err(WorkerError::store)?;

    let definition = materialize_script_bundle(object_store, definition).await?;
    let execution = RunExecution {
        run: run.clone(),
        definition,
        pool,
    };
    let cancellation = StoreCancellationCheck { store };
    let report = runner
        .execute(&execution, &cancellation)
        .await
        .map_err(WorkerError::runner)?;

    persist_logs(store, object_store, &run, report.logs).await?;

    persist_report_artifacts(store, object_store, &run, report.artifacts).await?;

    let (final_status, retry_delay, outcome) = match report.outcome {
        RunOutcome::Succeeded => (
            JobRunStatus::Succeeded,
            None,
            WorkerTickOutcome::RunSucceeded,
        ),
        RunOutcome::Cancelled => (
            JobRunStatus::Cancelled,
            None,
            WorkerTickOutcome::RunCancelled,
        ),
        RunOutcome::Failed => retry_decision(
            &run,
            &execution.definition,
            JobRunStatus::Failed,
            WorkerTickOutcome::RunFailed,
        ),
        RunOutcome::TimedOut => retry_decision(
            &run,
            &execution.definition,
            JobRunStatus::TimedOut,
            WorkerTickOutcome::RunTimedOut,
        ),
    };

    let latest = store
        .finish_running_attempt(&run.id, run.attempt_count, final_status, retry_delay)
        .await
        .map_err(WorkerError::store)?;
    if latest.is_none() {
        return Ok(
            match store
                .find_run(&run.id)
                .await
                .map_err(WorkerError::store)?
                .map(|latest| latest.status)
            {
                Some(JobRunStatus::Cancelled) => WorkerTickOutcome::RunCancelled,
                _ => outcome,
            },
        );
    }

    Ok(outcome)
}

async fn materialize_script_bundle<O>(
    object_store: &O,
    mut definition: JobDefinition,
) -> Result<JobDefinition, WorkerError>
where
    O: ObjectStore,
{
    if !definition
        .command
        .iter()
        .any(|part| part == "/capsulet/workspace/main.py")
    {
        return Ok(definition);
    }

    let Some(bytes) = object_store
        .get(&definition.bundle_object_key)
        .await
        .map_err(WorkerError::object_store)?
    else {
        return Err(WorkerError::MissingScriptBundle(
            definition.bundle_object_key,
        ));
    };
    let script = String::from_utf8_lossy(&bytes).into_owned();
    definition.command = vec!["python".to_string(), "-c".to_string(), script];

    Ok(definition)
}

async fn persist_logs<S, O>(
    store: &S,
    object_store: &O,
    run: &JobRun,
    logs: Option<String>,
) -> Result<(), WorkerError>
where
    S: WorkerStore,
    O: ObjectStore,
{
    let Some(logs) = logs.filter(|logs| !logs.is_empty()) else {
        return Ok(());
    };
    let inline = truncate_utf8(&logs, INLINE_LOG_LIMIT_BYTES);
    store
        .save_log(&JobRunLog::new(run.id.clone(), inline).map_err(WorkerError::InvalidLog)?)
        .await
        .map_err(WorkerError::store)?;

    if logs.len() <= INLINE_LOG_LIMIT_BYTES {
        return Ok(());
    }

    let object_key = run_object_key(&run.id, ArtifactObjectKind::Log, "stdout.log")
        .map_err(WorkerError::object_store)?;
    let size_bytes = u64::try_from(logs.len())
        .map_err(|_| WorkerError::InvalidArtifact("log is too large".to_string()))?;
    object_store
        .put(&object_key, logs.into_bytes())
        .await
        .map_err(WorkerError::object_store)?;
    let metadata = JobArtifact::new(
        ArtifactId::new(format!("log_{}_stdout", run.id.as_str()))
            .map_err(WorkerError::InvalidArtifact)?,
        run.id.clone(),
        None,
        "stdout.log",
        object_key,
        "text/plain",
        size_bytes,
        None,
        ArtifactObjectKind::Log,
    )
    .map_err(WorkerError::InvalidArtifact)?;
    store
        .save_artifact(&metadata)
        .await
        .map_err(WorkerError::store)
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

async fn persist_report_artifacts<S, O>(
    store: &S,
    object_store: &O,
    run: &JobRun,
    artifacts: Vec<capsulet_runner::CollectedArtifact>,
) -> Result<(), WorkerError>
where
    S: WorkerStore,
    O: ObjectStore,
{
    for artifact in artifacts {
        let object_key = run_object_key(&run.id, ArtifactObjectKind::Artifact, &artifact.name)
            .map_err(WorkerError::object_store)?;
        let size_bytes = u64::try_from(artifact.bytes.len())
            .map_err(|_| WorkerError::InvalidArtifact("artifact is too large".to_string()))?;
        object_store
            .put(&object_key, artifact.bytes)
            .await
            .map_err(WorkerError::object_store)?;
        let metadata = JobArtifact::new(
            ArtifactId::new(format!("artifact_{}_{}", run.id.as_str(), artifact.name))
                .map_err(WorkerError::InvalidArtifact)?,
            run.id.clone(),
            None,
            artifact.name,
            object_key,
            artifact.content_type,
            size_bytes,
            None,
            ArtifactObjectKind::Artifact,
        )
        .map_err(WorkerError::InvalidArtifact)?;
        store
            .save_artifact(&metadata)
            .await
            .map_err(WorkerError::store)?;
    }

    Ok(())
}

fn retry_decision(
    run: &JobRun,
    definition: &JobDefinition,
    failed_status: JobRunStatus,
    exhausted_outcome: WorkerTickOutcome,
) -> (JobRunStatus, Option<u64>, WorkerTickOutcome) {
    if run.attempt_count < definition.retry_max_attempts {
        (
            JobRunStatus::RetryScheduled,
            Some(definition.retry_delay_seconds),
            WorkerTickOutcome::RunRetryScheduled,
        )
    } else {
        (failed_status, None, exhausted_outcome)
    }
}

struct StoreCancellationCheck<'a, S> {
    store: &'a S,
}

#[async_trait]
impl<S> CancellationCheck for StoreCancellationCheck<'_, S>
where
    S: WorkerStore,
{
    type Error = String;

    async fn is_cancelled(&self, id: &capsulet_core::JobRunId) -> Result<bool, Self::Error> {
        self.store
            .is_run_cancelled(id)
            .await
            .map_err(|error| error.to_string())
    }
}

/// Worker use-case error.
#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("store error: {0}")]
    Store(String),
    #[error("runner error: {0}")]
    Runner(String),
    #[error("missing job definition for leased run: {0}")]
    MissingJobDefinition(String),
    #[error("missing execution pool for leased run: {0}")]
    MissingExecutionPool(String),
    #[error("missing script bundle object: {0}")]
    MissingScriptBundle(String),
    #[error("invalid captured log: {0}")]
    InvalidLog(String),
    #[error("invalid artifact metadata: {0}")]
    InvalidArtifact(String),
    #[error("object storage error: {0}")]
    ObjectStore(String),
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

    fn object_store(error: impl std::fmt::Display) -> Self {
        Self::ObjectStore(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };

    use capsulet_core::{
        ExecutionPoolName, JobArtifact, JobDefinition, JobDefinitionId, JobRun, JobRunId, JobRunLog,
    };
    use capsulet_runner::{ExecutionPoolConfig, ExecutionPoolsConfig, PoolResources, StubRunner};
    use capsulet_storage::FilesystemObjectStore;

    use super::{WorkerStore, WorkerTickOutcome, execute_one_queued_run};

    #[derive(Debug, Clone, Default)]
    struct FakeStore {
        queued: Arc<Mutex<Vec<JobRun>>>,
        saved: Arc<Mutex<Vec<JobRun>>>,
        definitions: Arc<Mutex<Vec<JobDefinition>>>,
        logs: Arc<Mutex<Vec<JobRunLog>>>,
        artifacts: Arc<Mutex<Vec<JobArtifact>>>,
        cancelled: Arc<Mutex<Vec<capsulet_core::JobRunId>>>,
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

        async fn find_job_definition(
            &self,
            id: &JobDefinitionId,
        ) -> Result<Option<JobDefinition>, Self::Error> {
            Ok(self
                .definitions
                .lock()
                .map_err(|error| error.to_string())?
                .iter()
                .find(|definition| definition.id == *id)
                .cloned())
        }

        async fn save_log(&self, log: &JobRunLog) -> Result<(), Self::Error> {
            self.logs
                .lock()
                .map_err(|error| error.to_string())?
                .push(log.clone());
            Ok(())
        }

        async fn save_artifact(&self, artifact: &JobArtifact) -> Result<(), Self::Error> {
            self.artifacts
                .lock()
                .map_err(|error| error.to_string())?
                .push(artifact.clone());
            Ok(())
        }

        async fn find_run(
            &self,
            id: &capsulet_core::JobRunId,
        ) -> Result<Option<JobRun>, Self::Error> {
            let saved = self
                .saved
                .lock()
                .map_err(|error| error.to_string())?
                .clone();
            let queued = self
                .queued
                .lock()
                .map_err(|error| error.to_string())?
                .clone();

            Ok(saved
                .iter()
                .rev()
                .chain(queued.iter().rev())
                .find(|run| run.id == *id)
                .cloned())
        }

        async fn finish_running_attempt(
            &self,
            id: &capsulet_core::JobRunId,
            attempt_count: u32,
            status: capsulet_core::JobRunStatus,
            _retry_delay_seconds: Option<u64>,
        ) -> Result<Option<JobRun>, Self::Error> {
            let Some(mut run) = self.find_run(id).await? else {
                return Ok(None);
            };
            if run.status != capsulet_core::JobRunStatus::Running
                || run.attempt_count != attempt_count
            {
                return Ok(None);
            }
            run.status = status;
            self.save_run(&run).await?;
            Ok(Some(run))
        }

        async fn promote_ready_retries(&self) -> Result<u64, Self::Error> {
            Ok(0)
        }

        async fn recover_expired_leases(&self) -> Result<u64, Self::Error> {
            Ok(0)
        }

        async fn is_run_cancelled(
            &self,
            id: &capsulet_core::JobRunId,
        ) -> Result<bool, Self::Error> {
            Ok(self
                .cancelled
                .lock()
                .map_err(|error| error.to_string())?
                .iter()
                .any(|cancelled| cancelled == id))
        }
    }

    impl FakeStore {
        fn with_run(run: JobRun) -> Self {
            Self {
                queued: Arc::new(Mutex::new(vec![run])),
                saved: Arc::default(),
                definitions: Arc::new(Mutex::new(vec![JobDefinition::hello_python()])),
                logs: Arc::default(),
                artifacts: Arc::default(),
                cancelled: Arc::default(),
            }
        }

        fn with_definition(self, definition: JobDefinition) -> Self {
            *self.definitions.lock().expect("definitions mutex") = vec![definition];
            self
        }

        fn with_cancelled(self, id: capsulet_core::JobRunId) -> Self {
            self.cancelled.lock().expect("cancelled mutex").push(id);
            self
        }

        fn without_definitions(mut self) -> Self {
            self.definitions = Arc::default();
            self
        }

        fn saved_runs(&self) -> Vec<JobRun> {
            self.saved.lock().expect("saved mutex").clone()
        }

        fn saved_artifacts(&self) -> Vec<JobArtifact> {
            self.artifacts.lock().expect("artifacts mutex").clone()
        }
    }

    fn object_store() -> FilesystemObjectStore {
        FilesystemObjectStore::new(std::env::temp_dir().join("capsulet-worker-test-artifacts"))
    }

    fn pools() -> ExecutionPoolsConfig {
        let pool = ExecutionPoolConfig {
            description: String::new(),
            node_selector: BTreeMap::default(),
            tolerations: Vec::new(),
            resources: PoolResources::default(),
            timeout_seconds: 60,
            max_concurrent_jobs: 1,
            ttl_seconds_after_finished: None,
        };
        ExecutionPoolsConfig {
            default_pool: "mini".to_string(),
            pools: [("mini".to_string(), pool)].into(),
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
        let outcome = execute_one_queued_run(
            &FakeStore::default(),
            &StubRunner::success(),
            &object_store(),
            &pools(),
            "w1",
            60,
        )
        .await
        .expect("worker tick");

        assert_eq!(outcome, WorkerTickOutcome::NoRunAvailable);
    }

    #[tokio::test]
    async fn stub_success_runner_completes_run() {
        let store = FakeStore::with_run(run());

        let outcome = execute_one_queued_run(
            &store,
            &StubRunner::success(),
            &object_store(),
            &pools(),
            "w1",
            60,
        )
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

        let outcome = execute_one_queued_run(
            &store,
            &StubRunner::failure(),
            &object_store(),
            &pools(),
            "w1",
            60,
        )
        .await
        .expect("worker tick");

        assert_eq!(outcome, WorkerTickOutcome::RunFailed);
        let saved = store.saved_runs();
        assert_eq!(saved[0].status, capsulet_core::JobRunStatus::Running);
        assert_eq!(saved[1].status, capsulet_core::JobRunStatus::Failed);
    }

    #[tokio::test]
    async fn stub_failure_runner_schedules_retry_when_attempts_remain() {
        let mut definition = JobDefinition::hello_python();
        definition.retry_max_attempts = 2;
        definition.retry_delay_seconds = 1;
        let store = FakeStore::with_run(run()).with_definition(definition);

        let outcome = execute_one_queued_run(
            &store,
            &StubRunner::failure(),
            &object_store(),
            &pools(),
            "w1",
            60,
        )
        .await
        .expect("worker tick");

        assert_eq!(outcome, WorkerTickOutcome::RunRetryScheduled);
        let saved = store.saved_runs();
        assert_eq!(saved[0].status, capsulet_core::JobRunStatus::Running);
        assert_eq!(saved[1].status, capsulet_core::JobRunStatus::RetryScheduled);
    }

    #[tokio::test]
    async fn cancelled_run_is_not_completed_by_runner() {
        let run = run();
        let store = FakeStore::with_run(run.clone()).with_cancelled(run.id.clone());

        let outcome = execute_one_queued_run(
            &store,
            &StubRunner::success(),
            &object_store(),
            &pools(),
            "w1",
            60,
        )
        .await
        .expect("worker tick");

        assert_eq!(outcome, WorkerTickOutcome::RunCancelled);
        let saved = store.saved_runs();
        assert_eq!(saved[1].status, capsulet_core::JobRunStatus::Cancelled);
    }

    #[tokio::test]
    async fn errors_when_definition_is_missing() {
        let store = FakeStore::with_run(run()).without_definitions();

        let error = execute_one_queued_run(
            &store,
            &StubRunner::success(),
            &object_store(),
            &pools(),
            "w1",
            60,
        )
        .await
        .expect_err("missing definition");

        assert!(matches!(error, super::WorkerError::MissingJobDefinition(_)));
    }

    #[tokio::test]
    async fn errors_when_pool_is_missing() {
        let store = FakeStore::with_run(run());
        let pools = ExecutionPoolsConfig {
            default_pool: "mini".to_string(),
            pools: BTreeMap::default(),
        };

        let error = execute_one_queued_run(
            &store,
            &StubRunner::success(),
            &object_store(),
            &pools,
            "w1",
            60,
        )
        .await
        .expect_err("missing pool");

        assert!(matches!(error, super::WorkerError::MissingExecutionPool(_)));
    }

    #[tokio::test]
    async fn stores_runner_artifacts() {
        let store = FakeStore::with_run(run());

        let outcome = execute_one_queued_run(
            &store,
            &StubRunner::success_with_artifact("artifact text"),
            &object_store(),
            &pools(),
            "w1",
            60,
        )
        .await
        .expect("worker tick");

        assert_eq!(outcome, WorkerTickOutcome::RunSucceeded);
        let artifacts = store.saved_artifacts();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].name, "stub-artifact.txt");
    }

    #[tokio::test]
    async fn offloads_large_logs_as_artifact() {
        let store = FakeStore::with_run(run());
        let logs = "x".repeat(super::INLINE_LOG_LIMIT_BYTES + 1);

        let outcome = execute_one_queued_run(
            &store,
            &StubRunner::success_with_logs(logs),
            &object_store(),
            &pools(),
            "w1",
            60,
        )
        .await
        .expect("worker tick");

        assert_eq!(outcome, WorkerTickOutcome::RunSucceeded);
        let artifacts = store.saved_artifacts();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, capsulet_core::ArtifactObjectKind::Log);
    }
}
