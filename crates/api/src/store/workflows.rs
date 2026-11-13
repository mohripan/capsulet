//! Workflow definitions, workflow runs and their steps.

use async_trait::async_trait;
use capsulet_core::{
    AutomationId, WorkflowDefinition, WorkflowId, WorkflowRun, WorkflowRunId, WorkflowStepRun,
};
use capsulet_postgres::PostgresStore;

use super::StoreBase;

/// Workflow definitions, workflow runs and their steps.
#[async_trait]
pub trait WorkflowStore: StoreBase {
    async fn upsert_workflow(
        &self,
        workflow: &WorkflowDefinition,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error>;

    async fn list_workflows(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<WorkflowDefinition>, Self::Error>;

    async fn find_workflow(
        &self,
        id: &WorkflowId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<WorkflowDefinition>, Self::Error>;

    async fn workflow_has_active_runs(&self, id: &WorkflowId) -> Result<bool, Self::Error>;

    async fn delete_workflow(
        &self,
        id: &WorkflowId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<bool, Self::Error>;

    async fn create_workflow_run(
        &self,
        workflow_id: &WorkflowId,
        automation_id: Option<&AutomationId>,
        run_id: &WorkflowRunId,
        input_json: &str,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<WorkflowRun, Self::Error>;

    async fn list_workflow_runs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<WorkflowRun>, Self::Error>;

    async fn find_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<WorkflowRun>, Self::Error>;

    async fn remove_queued_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<WorkflowRun>, Self::Error>;

    async fn cancel_running_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<WorkflowRun>, Self::Error>;

    async fn resume_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<WorkflowRun>, Self::Error>;

    async fn list_workflow_step_runs(
        &self,
        workflow_run_id: &WorkflowRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<WorkflowStepRun>, Self::Error>;
}

#[async_trait]
impl WorkflowStore for PostgresStore {
    async fn upsert_workflow(
        &self,
        workflow: &WorkflowDefinition,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error> {
        self.upsert_workflow_in_project(workflow, tenant_id, project_id)
            .await
    }

    async fn list_workflows(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<WorkflowDefinition>, Self::Error> {
        self.list_workflows_in_project(tenant_id, project_id, limit)
            .await
    }

    async fn find_workflow(
        &self,
        id: &WorkflowId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<WorkflowDefinition>, Self::Error> {
        self.find_workflow_in_project(id, tenant_id, project_id)
            .await
    }

    async fn workflow_has_active_runs(&self, id: &WorkflowId) -> Result<bool, Self::Error> {
        self.workflow_has_active_runs(id).await
    }

    async fn delete_workflow(
        &self,
        id: &WorkflowId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<bool, Self::Error> {
        self.delete_workflow_in_project(id, tenant_id, project_id)
            .await
    }

    async fn create_workflow_run(
        &self,
        workflow_id: &WorkflowId,
        automation_id: Option<&AutomationId>,
        run_id: &WorkflowRunId,
        input_json: &str,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<WorkflowRun, Self::Error> {
        self.create_workflow_run_in_project(
            workflow_id,
            automation_id,
            run_id,
            input_json,
            tenant_id,
            project_id,
        )
        .await
    }

    async fn list_workflow_runs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<WorkflowRun>, Self::Error> {
        self.list_workflow_runs_in_project(tenant_id, project_id, limit)
            .await
    }

    async fn find_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<WorkflowRun>, Self::Error> {
        self.find_workflow_run_in_project(workflow_run_id, tenant_id, project_id)
            .await
    }

    async fn remove_queued_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<WorkflowRun>, Self::Error> {
        PostgresStore::remove_queued_workflow_run(self, workflow_run_id, tenant_id, project_id)
            .await
    }

    async fn cancel_running_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<WorkflowRun>, Self::Error> {
        PostgresStore::cancel_running_workflow_run(self, workflow_run_id, tenant_id, project_id)
            .await
    }

    async fn resume_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<WorkflowRun>, Self::Error> {
        PostgresStore::resume_workflow_run(self, workflow_run_id, tenant_id, project_id).await
    }

    async fn list_workflow_step_runs(
        &self,
        workflow_run_id: &WorkflowRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<WorkflowStepRun>, Self::Error> {
        PostgresStore::list_workflow_step_runs(self, workflow_run_id, tenant_id, project_id).await
    }
}
