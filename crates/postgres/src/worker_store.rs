use async_trait::async_trait;
use capsulet_application::{
    JobArtifactRepository, JobRunLogRepository, JobRunRepository, execution::WorkerStore,
};
use capsulet_core::{JobArtifact, JobDefinition, JobDefinitionId, JobRun, JobRunLog};

use crate::{PostgresStore, PostgresStoreError};

#[async_trait]
impl WorkerStore for PostgresStore {
    type Error = PostgresStoreError;

    async fn lease_next_queued_run(
        &self,
        worker_id: &str,
        lease_seconds: i64,
        pool_limits: &[(String, u32)],
        reattach_running: bool,
    ) -> Result<Option<JobRun>, Self::Error> {
        self.lease_next_queued_run_with_pool_limits_and_reattach(
            worker_id,
            lease_seconds,
            pool_limits,
            reattach_running,
        )
        .await
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
        JobArtifactRepository::save_artifact(self, artifact).await
    }

    async fn find_run(&self, id: &capsulet_core::JobRunId) -> Result<Option<JobRun>, Self::Error> {
        self.find_by_id(id).await
    }

    async fn finish_running_attempt(
        &self,
        id: &capsulet_core::JobRunId,
        attempt_count: u32,
        status: capsulet_core::JobRunStatus,
        retry_delay_seconds: Option<u64>,
    ) -> Result<Option<JobRun>, Self::Error> {
        self.finish_running_attempt(id, attempt_count, status, retry_delay_seconds)
            .await
    }

    async fn promote_ready_retries(&self) -> Result<u64, Self::Error> {
        self.promote_ready_retries().await
    }

    async fn recover_expired_leases(&self, preserve_running: bool) -> Result<u64, Self::Error> {
        self.recover_expired_leases_for_runner(preserve_running)
            .await
    }

    async fn list_upstream_artifacts(
        &self,
        id: &capsulet_core::JobRunId,
    ) -> Result<Vec<(String, JobArtifact)>, Self::Error> {
        Ok(self
            .list_upstream_artifacts(id)
            .await?
            .into_iter()
            .map(|input| (input.producer_step_id, input.artifact))
            .collect())
    }

    async fn heartbeat_run(
        &self,
        id: &capsulet_core::JobRunId,
        worker_id: &str,
        lease_seconds: i64,
    ) -> Result<bool, Self::Error> {
        self.heartbeat_run(id, worker_id, lease_seconds).await
    }

    async fn is_run_cancelled(&self, id: &capsulet_core::JobRunId) -> Result<bool, Self::Error> {
        self.is_run_cancelled(id).await
    }

    async fn advance_workflow_runs_for_job_run(
        &self,
        id: &capsulet_core::JobRunId,
    ) -> Result<u64, Self::Error> {
        self.advance_workflow_runs_for_job_run(id).await
    }

    async fn job_run_timeout_seconds(
        &self,
        id: &capsulet_core::JobRunId,
    ) -> Result<Option<u64>, Self::Error> {
        self.job_run_timeout_seconds(id).await
    }

    async fn active_leased_run_ids(&self) -> Result<Vec<capsulet_core::JobRunId>, Self::Error> {
        self.active_leased_run_ids().await
    }
}
