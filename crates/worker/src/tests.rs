use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use capsulet_core::{
    ExecutionPoolName, JobArtifact, JobDefinition, JobDefinitionId, JobRun, JobRunId, JobRunLog,
    JobRunStatus, JobRunTransition,
};
use capsulet_runner::{
    CancellationCheck, ExecutionPoolConfig, ExecutionPoolsConfig, PoolResources, RunExecution,
    RunReport, Runner, StubRunner,
};
use capsulet_storage::FilesystemObjectStore;

use super::{WorkerStore, WorkerTickOutcome, execute_one_queued_run};

#[derive(Debug, Clone, Default)]
struct FakeStore {
    queued: Arc<Mutex<Vec<JobRun>>>,
    saved: Arc<Mutex<Vec<JobRun>>>,
    definitions: Arc<Mutex<Vec<JobDefinition>>>,
    logs: Arc<Mutex<Vec<JobRunLog>>>,
    artifacts: Arc<Mutex<Vec<JobArtifact>>>,
    upstream_artifacts: Arc<Mutex<Vec<(String, JobArtifact)>>>,
    cancelled: Arc<Mutex<Vec<capsulet_core::JobRunId>>>,
    heartbeat_count: Arc<Mutex<u32>>,
}

#[async_trait::async_trait]
impl WorkerStore for FakeStore {
    type Error = String;

    async fn lease_next_queued_run(
        &self,
        _worker_id: &str,
        _lease_seconds: i64,
        _pool_limits: &[(String, u32)],
        _reattach_running: bool,
    ) -> Result<Option<JobRun>, Self::Error> {
        let mut run = self.queued.lock().map_err(|error| error.to_string())?.pop();
        if let Some(run) = &mut run {
            run.apply(JobRunTransition::Lease)
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
            .find(|definition| definition.id() == id)
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

    async fn find_run(&self, id: &capsulet_core::JobRunId) -> Result<Option<JobRun>, Self::Error> {
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
            .find(|run| run.id() == id)
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
        if run.status() != capsulet_core::JobRunStatus::Running
            || run.attempt_count() != attempt_count
        {
            return Ok(None);
        }
        if status == JobRunStatus::RetryScheduled {
            run.apply(JobRunTransition::Fail)
                .and_then(|()| run.apply(JobRunTransition::ScheduleRetry))
                .map_err(|error| error.to_string())?;
        } else {
            let transition = match status {
                JobRunStatus::Succeeded => JobRunTransition::Succeed,
                JobRunStatus::Failed => JobRunTransition::Fail,
                JobRunStatus::Cancelled => JobRunTransition::Cancel,
                JobRunStatus::TimedOut => JobRunTransition::TimeOut,
                status => return Err(format!("unsupported fake completion status {status}")),
            };
            run.apply(transition).map_err(|error| error.to_string())?;
        }
        self.save_run(&run).await?;
        Ok(Some(run))
    }

    async fn promote_ready_retries(&self) -> Result<u64, Self::Error> {
        Ok(0)
    }

    async fn recover_expired_leases(&self, _preserve_running: bool) -> Result<u64, Self::Error> {
        Ok(0)
    }

    async fn list_upstream_artifacts(
        &self,
        _id: &capsulet_core::JobRunId,
    ) -> Result<Vec<(String, JobArtifact)>, Self::Error> {
        Ok(self
            .upstream_artifacts
            .lock()
            .map_err(|error| error.to_string())?
            .clone())
    }

    async fn heartbeat_run(
        &self,
        _id: &capsulet_core::JobRunId,
        _worker_id: &str,
        _lease_seconds: i64,
    ) -> Result<bool, Self::Error> {
        *self
            .heartbeat_count
            .lock()
            .map_err(|error| error.to_string())? += 1;
        Ok(true)
    }

    async fn is_run_cancelled(&self, id: &capsulet_core::JobRunId) -> Result<bool, Self::Error> {
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
            upstream_artifacts: Arc::default(),
            cancelled: Arc::default(),
            heartbeat_count: Arc::default(),
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

    fn heartbeat_count(&self) -> u32 {
        *self.heartbeat_count.lock().expect("heartbeat mutex")
    }
}

#[derive(Clone, Copy)]
struct SlowRunner;

#[async_trait::async_trait]
impl Runner for SlowRunner {
    type Error = std::convert::Infallible;

    async fn execute<C>(
        &self,
        _execution: &RunExecution,
        _cancellation: &C,
    ) -> Result<RunReport, Self::Error>
    where
        C: CancellationCheck + Sync,
    {
        tokio::time::sleep(std::time::Duration::from_millis(1_100)).await;
        Ok(RunReport::succeeded(None))
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
        runtime_class_name: None,
        service_account_name: None,
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
async fn renews_lease_while_runner_is_active() {
    let store = FakeStore::with_run(run());

    let outcome = execute_one_queued_run(
        &store,
        &SlowRunner,
        &object_store(),
        &pools(),
        "worker-test",
        3,
    )
    .await
    .expect("slow execution succeeds");

    assert_eq!(outcome, WorkerTickOutcome::RunSucceeded);
    assert!(store.heartbeat_count() >= 3);
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
    assert_eq!(saved[0].status(), capsulet_core::JobRunStatus::Running);
    assert_eq!(saved[0].attempt_count(), 1);
    assert_eq!(saved[1].status(), capsulet_core::JobRunStatus::Succeeded);
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
    assert_eq!(saved[0].status(), capsulet_core::JobRunStatus::Running);
    assert_eq!(saved[1].status(), capsulet_core::JobRunStatus::Failed);
}

#[tokio::test]
async fn stub_failure_runner_schedules_retry_when_attempts_remain() {
    let definition = JobDefinition::new(
        JobDefinition::hello_python().id().clone(),
        "Hello Python",
        "python:3.12-slim",
        vec![
            "python".to_string(),
            "-c".to_string(),
            "print('hello from capsulet')".to_string(),
        ],
        "bundles/job_hello_python.tar.gz",
        "{}",
        capsulet_core::RetryPolicy::new(2, 1).expect("retry policy"),
    )
    .expect("retryable definition");
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
    assert_eq!(saved[0].status(), capsulet_core::JobRunStatus::Running);
    assert_eq!(
        saved[1].status(),
        capsulet_core::JobRunStatus::RetryScheduled
    );
}

#[tokio::test]
async fn cancelled_run_is_not_completed_by_runner() {
    let run = run();
    let store = FakeStore::with_run(run.clone()).with_cancelled(run.id().clone());

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
    assert_eq!(saved[1].status(), capsulet_core::JobRunStatus::Cancelled);
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
    assert_eq!(artifacts[0].name(), "stub-artifact.txt");
}

#[tokio::test]
async fn offloads_large_logs_as_artifact() {
    let store = FakeStore::with_run(run());
    let logs = "x".repeat(crate::worker::INLINE_LOG_LIMIT_BYTES + 1);

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
    assert_eq!(artifacts[0].kind(), capsulet_core::ArtifactObjectKind::Log);
}
