//! Job definitions, job runs, their logs and their artifacts.

use async_trait::async_trait;
use capsulet_application::{JobArtifactRepository, JobRunRepository};
use capsulet_core::{
    ArtifactId, JobArtifact, JobDefinition, JobDefinitionId, JobRun, JobRunId, JobRunLog,
};
use capsulet_postgres::PostgresStore;

use super::StoreBase;

/// Job definitions, job runs, their logs and their artifacts.
#[async_trait]
pub trait JobStore: StoreBase {
    async fn job_definition_exists(&self, id: &JobDefinitionId) -> Result<bool, Self::Error>;

    async fn upsert_job_definition(
        &self,
        definition: &JobDefinition,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error>;

    async fn list_job_definitions(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<JobDefinition>, Self::Error>;

    async fn find_job_definition(
        &self,
        id: &JobDefinitionId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<JobDefinition>, Self::Error>;

    async fn job_definition_has_active_workflow_runs(
        &self,
        id: &JobDefinitionId,
    ) -> Result<bool, Self::Error>;

    async fn job_definition_is_used_by_workflows(
        &self,
        id: &JobDefinitionId,
    ) -> Result<bool, Self::Error>;

    async fn delete_job_definition(
        &self,
        id: &JobDefinitionId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<bool, Self::Error>;

    /// Creates a job run owned by a project. Returns whether this call created it,
    /// so a caller that named the id can say it was already taken.
    async fn create_job_run(
        &self,
        run: &JobRun,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<bool, Self::Error>;

    async fn list_runs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<JobRun>, Self::Error>;

    /// Whether any project has a job run with this id. Run ids are unique across
    /// the installation, so a create needs to know, and must not say whose it is.
    async fn job_run_exists(&self, id: &JobRunId) -> Result<bool, Self::Error>;

    async fn find_run(
        &self,
        id: &JobRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<JobRun>, Self::Error>;

    async fn find_run_log(
        &self,
        id: &JobRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<JobRunLog>, Self::Error>;

    async fn cancel_run(
        &self,
        id: &JobRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<JobRun>, Self::Error>;

    async fn list_artifacts(
        &self,
        id: &JobRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<JobArtifact>, Self::Error>;

    async fn find_artifact(
        &self,
        run_id: &JobRunId,
        artifact_id: &ArtifactId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<JobArtifact>, Self::Error>;

    async fn save_artifact(&self, artifact: &JobArtifact) -> Result<(), Self::Error>;
}

#[async_trait]
impl JobStore for PostgresStore {
    async fn job_definition_exists(&self, id: &JobDefinitionId) -> Result<bool, Self::Error> {
        self.job_definition_exists(id).await
    }

    async fn create_job_run(
        &self,
        run: &JobRun,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<bool, Self::Error> {
        self.create_job_run_if_absent_in_project(run, tenant_id, project_id)
            .await
    }

    async fn upsert_job_definition(
        &self,
        definition: &JobDefinition,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error> {
        self.upsert_job_definition_in_project(definition, tenant_id, project_id)
            .await
    }

    async fn list_job_definitions(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<JobDefinition>, Self::Error> {
        self.list_job_definitions_in_project(tenant_id, project_id, limit)
            .await
    }

    async fn find_job_definition(
        &self,
        id: &JobDefinitionId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<JobDefinition>, Self::Error> {
        self.find_job_definition_in_project(id, tenant_id, project_id)
            .await
    }

    async fn job_definition_has_active_workflow_runs(
        &self,
        id: &JobDefinitionId,
    ) -> Result<bool, Self::Error> {
        self.job_definition_has_active_workflow_runs(id).await
    }

    async fn job_definition_is_used_by_workflows(
        &self,
        id: &JobDefinitionId,
    ) -> Result<bool, Self::Error> {
        self.job_definition_is_used_by_workflows(id).await
    }

    async fn delete_job_definition(
        &self,
        id: &JobDefinitionId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<bool, Self::Error> {
        self.delete_job_definition_in_project(id, tenant_id, project_id)
            .await
    }

    async fn list_runs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<JobRun>, Self::Error> {
        self.list_job_runs_in_project(tenant_id, project_id, limit)
            .await
    }

    async fn job_run_exists(&self, id: &JobRunId) -> Result<bool, Self::Error> {
        Ok(self.find_by_id(id).await?.is_some())
    }

    async fn find_run(
        &self,
        id: &JobRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<JobRun>, Self::Error> {
        self.find_job_run_in_project(id, tenant_id, project_id)
            .await
    }

    async fn find_run_log(
        &self,
        id: &JobRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<JobRunLog>, Self::Error> {
        self.find_job_run_log_in_project(id, tenant_id, project_id)
            .await
    }

    async fn cancel_run(
        &self,
        id: &JobRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<JobRun>, Self::Error> {
        self.cancel_job_run_in_project(id, tenant_id, project_id)
            .await
    }

    async fn list_artifacts(
        &self,
        id: &JobRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<JobArtifact>, Self::Error> {
        self.list_job_artifacts_in_project(id, tenant_id, project_id)
            .await
    }

    async fn find_artifact(
        &self,
        run_id: &JobRunId,
        artifact_id: &ArtifactId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<JobArtifact>, Self::Error> {
        self.find_job_artifact_in_project(run_id, artifact_id, tenant_id, project_id)
            .await
    }

    async fn save_artifact(&self, artifact: &JobArtifact) -> Result<(), Self::Error> {
        JobArtifactRepository::save_artifact(self, artifact).await
    }
}
