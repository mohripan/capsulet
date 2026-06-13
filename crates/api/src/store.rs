use std::fmt::Display;

use async_trait::async_trait;
use capsulet_core::{
    ArtifactId, Automation, AutomationId, AutomationTrigger, CustomTriggerPlugin, JobArtifact,
    JobDefinition, JobDefinitionId, JobRun, JobRunId, JobRunLog, JobRunLogRepository,
    JobRunRepository, WorkflowDefinition, WorkflowId, WorkflowRun, WorkflowRunId, WorkflowStepRun,
};
use capsulet_postgres::{PostgresStore, PostgresStoreError};
/// Storage operations required by the HTTP API.
#[async_trait]
pub trait ApiStore: Clone + Send + Sync + 'static {
    type Error: Display + Send + Sync + 'static;

    async fn job_definition_exists(&self, id: &JobDefinitionId) -> Result<bool, Self::Error>;
    async fn upsert_job_definition(&self, definition: &JobDefinition) -> Result<(), Self::Error>;
    async fn list_job_definitions(&self, limit: i64) -> Result<Vec<JobDefinition>, Self::Error>;
    async fn find_job_definition(
        &self,
        id: &JobDefinitionId,
    ) -> Result<Option<JobDefinition>, Self::Error>;
    async fn delete_job_definition(&self, id: &JobDefinitionId) -> Result<bool, Self::Error>;
    async fn upsert_workflow(&self, workflow: &WorkflowDefinition) -> Result<(), Self::Error>;
    async fn list_workflows(&self, limit: i64) -> Result<Vec<WorkflowDefinition>, Self::Error>;
    async fn find_workflow(
        &self,
        id: &WorkflowId,
    ) -> Result<Option<WorkflowDefinition>, Self::Error>;
    async fn upsert_automation(&self, automation: &Automation) -> Result<(), Self::Error>;
    async fn list_automations(&self, limit: i64) -> Result<Vec<Automation>, Self::Error>;
    async fn find_automation(&self, id: &AutomationId) -> Result<Option<Automation>, Self::Error>;
    async fn replace_automation_triggers(
        &self,
        automation_id: &AutomationId,
        triggers: &[AutomationTrigger],
        condition_json: &str,
    ) -> Result<(), Self::Error>;
    async fn list_automation_triggers(
        &self,
        automation_id: &AutomationId,
    ) -> Result<(Vec<AutomationTrigger>, String), Self::Error>;
    async fn upsert_custom_trigger_plugin(
        &self,
        plugin: &CustomTriggerPlugin,
    ) -> Result<(), Self::Error>;
    async fn list_custom_trigger_plugins(
        &self,
        limit: i64,
    ) -> Result<Vec<CustomTriggerPlugin>, Self::Error>;
    async fn find_custom_trigger_plugin(
        &self,
        id: &str,
    ) -> Result<Option<CustomTriggerPlugin>, Self::Error>;
    async fn create_workflow_run(
        &self,
        workflow_id: &WorkflowId,
        automation_id: Option<&AutomationId>,
        run_id: &WorkflowRunId,
        input_json: &str,
    ) -> Result<WorkflowRun, Self::Error>;
    async fn list_workflow_runs(&self, limit: i64) -> Result<Vec<WorkflowRun>, Self::Error>;
    async fn list_workflow_step_runs(
        &self,
        workflow_run_id: &WorkflowRunId,
    ) -> Result<Vec<WorkflowStepRun>, Self::Error>;
    async fn save_run(&self, run: &JobRun) -> Result<(), Self::Error>;
    async fn list_runs(&self, limit: i64) -> Result<Vec<JobRun>, Self::Error>;
    async fn find_run(&self, id: &JobRunId) -> Result<Option<JobRun>, Self::Error>;
    async fn find_run_log(&self, id: &JobRunId) -> Result<Option<JobRunLog>, Self::Error>;
    async fn cancel_run(&self, id: &JobRunId) -> Result<Option<JobRun>, Self::Error>;
    async fn list_artifacts(&self, id: &JobRunId) -> Result<Vec<JobArtifact>, Self::Error>;
    async fn find_artifact(
        &self,
        run_id: &JobRunId,
        artifact_id: &ArtifactId,
    ) -> Result<Option<JobArtifact>, Self::Error>;
    async fn save_artifact(&self, artifact: &JobArtifact) -> Result<(), Self::Error>;
}

#[async_trait]
impl ApiStore for PostgresStore {
    type Error = PostgresStoreError;

    async fn job_definition_exists(&self, id: &JobDefinitionId) -> Result<bool, Self::Error> {
        self.job_definition_exists(id).await
    }

    async fn save_run(&self, run: &JobRun) -> Result<(), Self::Error> {
        self.save(run).await
    }

    async fn upsert_job_definition(&self, definition: &JobDefinition) -> Result<(), Self::Error> {
        self.upsert_job_definition(definition).await
    }

    async fn list_job_definitions(&self, limit: i64) -> Result<Vec<JobDefinition>, Self::Error> {
        self.list_job_definitions(limit).await
    }

    async fn find_job_definition(
        &self,
        id: &JobDefinitionId,
    ) -> Result<Option<JobDefinition>, Self::Error> {
        self.find_job_definition(id).await
    }

    async fn delete_job_definition(&self, id: &JobDefinitionId) -> Result<bool, Self::Error> {
        self.delete_job_definition(id).await
    }

    async fn upsert_workflow(&self, workflow: &WorkflowDefinition) -> Result<(), Self::Error> {
        self.upsert_workflow(workflow).await
    }

    async fn list_workflows(&self, limit: i64) -> Result<Vec<WorkflowDefinition>, Self::Error> {
        self.list_workflows(limit).await
    }

    async fn find_workflow(
        &self,
        id: &WorkflowId,
    ) -> Result<Option<WorkflowDefinition>, Self::Error> {
        self.find_workflow(id).await
    }

    async fn upsert_automation(&self, automation: &Automation) -> Result<(), Self::Error> {
        self.upsert_automation(automation).await
    }

    async fn list_automations(&self, limit: i64) -> Result<Vec<Automation>, Self::Error> {
        self.list_automations(limit).await
    }

    async fn find_automation(&self, id: &AutomationId) -> Result<Option<Automation>, Self::Error> {
        self.find_automation(id).await
    }

    async fn replace_automation_triggers(
        &self,
        automation_id: &AutomationId,
        triggers: &[AutomationTrigger],
        condition_json: &str,
    ) -> Result<(), Self::Error> {
        self.replace_automation_triggers(automation_id, triggers, condition_json)
            .await
    }

    async fn list_automation_triggers(
        &self,
        automation_id: &AutomationId,
    ) -> Result<(Vec<AutomationTrigger>, String), Self::Error> {
        self.list_automation_triggers(automation_id).await
    }

    async fn upsert_custom_trigger_plugin(
        &self,
        plugin: &CustomTriggerPlugin,
    ) -> Result<(), Self::Error> {
        self.upsert_custom_trigger_plugin(plugin).await
    }

    async fn list_custom_trigger_plugins(
        &self,
        limit: i64,
    ) -> Result<Vec<CustomTriggerPlugin>, Self::Error> {
        self.list_custom_trigger_plugins(limit).await
    }

    async fn find_custom_trigger_plugin(
        &self,
        id: &str,
    ) -> Result<Option<CustomTriggerPlugin>, Self::Error> {
        self.find_custom_trigger_plugin(id).await
    }

    async fn create_workflow_run(
        &self,
        workflow_id: &WorkflowId,
        automation_id: Option<&AutomationId>,
        run_id: &WorkflowRunId,
        input_json: &str,
    ) -> Result<WorkflowRun, Self::Error> {
        self.create_workflow_run(workflow_id, automation_id, run_id, input_json)
            .await
    }

    async fn list_workflow_runs(&self, limit: i64) -> Result<Vec<WorkflowRun>, Self::Error> {
        self.list_workflow_runs(limit).await
    }

    async fn list_workflow_step_runs(
        &self,
        workflow_run_id: &WorkflowRunId,
    ) -> Result<Vec<WorkflowStepRun>, Self::Error> {
        self.list_workflow_step_runs(workflow_run_id).await
    }

    async fn list_runs(&self, limit: i64) -> Result<Vec<JobRun>, Self::Error> {
        self.list_job_runs(limit).await
    }

    async fn find_run(&self, id: &JobRunId) -> Result<Option<JobRun>, Self::Error> {
        self.find_by_id(id).await
    }

    async fn find_run_log(&self, id: &JobRunId) -> Result<Option<JobRunLog>, Self::Error> {
        self.find_log_by_run_id(id).await
    }

    async fn cancel_run(&self, id: &JobRunId) -> Result<Option<JobRun>, Self::Error> {
        self.cancel_run(id).await
    }

    async fn list_artifacts(&self, id: &JobRunId) -> Result<Vec<JobArtifact>, Self::Error> {
        self.list_artifacts(id).await
    }

    async fn find_artifact(
        &self,
        run_id: &JobRunId,
        artifact_id: &ArtifactId,
    ) -> Result<Option<JobArtifact>, Self::Error> {
        self.find_artifact(run_id, artifact_id).await
    }

    async fn save_artifact(&self, artifact: &JobArtifact) -> Result<(), Self::Error> {
        capsulet_core::JobArtifactRepository::save_artifact(self, artifact).await
    }
}
