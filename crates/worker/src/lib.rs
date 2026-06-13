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
mod tests;
