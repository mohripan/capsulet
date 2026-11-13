//! Workflow definitions, workflow runs and their steps.

use super::super::*;

#[async_trait::async_trait]
impl WorkflowStore for FakeStore {
    async fn upsert_workflow(
        &self,
        workflow: &WorkflowDefinition,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error> {
        let mut ownership = self.ownership.lock().map_err(|error| error.to_string())?;
        if let Some(owner) = ownership
            .iter()
            .find(|owner| owner.resource == "workflows" && owner.id == workflow.id().as_str())
        {
            if owner.tenant_id != tenant_id || owner.project_id != project_id {
                return Err("scope mismatch".to_string());
            }
        } else {
            ownership.push(FakeOwnership {
                resource: "workflows".to_string(),
                id: workflow.id().as_str().to_string(),
                tenant_id: tenant_id.to_string(),
                project_id: project_id.to_string(),
            });
        }
        let mut workflows = self.workflows.lock().map_err(|error| error.to_string())?;
        workflows.retain(|existing| existing.id() != workflow.id());
        workflows.push(workflow.clone());
        Ok(())
    }

    async fn list_workflows(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<WorkflowDefinition>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        let ownership = self.ownership.lock().map_err(|error| error.to_string())?;
        Ok(self
            .workflows
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|workflow| {
                ownership
                    .iter()
                    .find(|owner| {
                        owner.resource == "workflows" && owner.id == workflow.id().as_str()
                    })
                    .map_or(tenant_id == "default" && project_id == "default", |owner| {
                        owner.tenant_id == tenant_id && owner.project_id == project_id
                    })
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_workflow(
        &self,
        id: &WorkflowId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<WorkflowDefinition>, Self::Error> {
        if self.resource_project("workflows", id.as_str()).await?
            != Some((tenant_id.to_string(), project_id.to_string()))
        {
            return Ok(None);
        }
        Ok(self
            .workflows
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|workflow| workflow.id().clone() == *id)
            .cloned())
    }

    async fn delete_workflow(
        &self,
        id: &WorkflowId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<bool, Self::Error> {
        if self.resource_project("workflows", id.as_str()).await?
            != Some((tenant_id.to_string(), project_id.to_string()))
        {
            return Ok(false);
        }
        let mut workflows = self.workflows.lock().map_err(|error| error.to_string())?;
        let before = workflows.len();
        workflows.retain(|workflow| workflow.id() != id);
        Ok(workflows.len() != before)
    }

    async fn workflow_has_active_runs(&self, id: &WorkflowId) -> Result<bool, Self::Error> {
        Ok(self
            .workflow_runs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .any(|run| {
                run.workflow_id() == id
                    && matches!(
                        run.status(),
                        WorkflowRunStatus::Queued | WorkflowRunStatus::Running
                    )
            }))
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
        if !self.owns("workflows", workflow_id.as_str(), tenant_id, project_id) {
            return Err("scope mismatch".to_string());
        }
        self.claim_resource("workflow_runs", run_id.as_str(), tenant_id, project_id)?;
        let run = WorkflowRun::new(
            run_id.clone(),
            workflow_id.clone(),
            automation_id.cloned(),
            input_json,
            capsulet_core::WorkflowRunStatus::Queued,
            0,
            "2026-06-13 12:00:00+00",
        );
        self.workflow_runs
            .lock()
            .map_err(|error| error.to_string())?
            .push(run.clone());
        Ok(run)
    }

    async fn list_workflow_runs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<WorkflowRun>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        let runs = self
            .workflow_runs
            .lock()
            .map_err(|error| error.to_string())?
            .clone();
        Ok(runs
            .into_iter()
            .filter(|run| self.owns("workflow_runs", run.id().as_str(), tenant_id, project_id))
            .take(limit)
            .collect())
    }

    async fn find_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<WorkflowRun>, Self::Error> {
        if !self.owned_by(
            "workflow_runs",
            workflow_run_id.as_str(),
            tenant_id,
            project_id,
        )? {
            return Ok(None);
        }
        Ok(self
            .workflow_runs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|run| run.id() == workflow_run_id)
            .cloned())
    }

    async fn remove_queued_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<WorkflowRun>, Self::Error> {
        if !self.owned_by(
            "workflow_runs",
            workflow_run_id.as_str(),
            tenant_id,
            project_id,
        )? {
            return Ok(None);
        }
        let has_step = self
            .workflow_step_runs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .any(|step_run| step_run.workflow_run_id() == workflow_run_id);
        let mut runs = self
            .workflow_runs
            .lock()
            .map_err(|error| error.to_string())?;
        let Some(run) = runs.iter_mut().find(|run| run.id() == workflow_run_id) else {
            return Ok(None);
        };
        if run.status() == WorkflowRunStatus::Queued && !has_step {
            *run = run.clone().with_status(WorkflowRunStatus::Removed);
        }
        Ok(Some(run.clone()))
    }

    async fn cancel_running_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<WorkflowRun>, Self::Error> {
        if !self.owned_by(
            "workflow_runs",
            workflow_run_id.as_str(),
            tenant_id,
            project_id,
        )? {
            return Ok(None);
        }
        let mut runs = self
            .workflow_runs
            .lock()
            .map_err(|error| error.to_string())?;
        let Some(run) = runs.iter_mut().find(|run| run.id() == workflow_run_id) else {
            return Ok(None);
        };
        if run.status() != WorkflowRunStatus::Running {
            return Ok(Some(run.clone()));
        }
        *run = run.clone().with_status(WorkflowRunStatus::Cancelled);

        let mut step_runs = self
            .workflow_step_runs
            .lock()
            .map_err(|error| error.to_string())?;
        if let Some(step_run) = step_runs.iter_mut().find(|step_run| {
            step_run.workflow_run_id() == workflow_run_id
                && step_run.position() == run.current_step_position()
        }) {
            *step_run = step_run.clone().with_status(WorkflowRunStatus::Cancelled);
            let mut job_runs = self.runs.lock().map_err(|error| error.to_string())?;
            if let Some(job_run) = job_runs
                .iter_mut()
                .find(|job_run| job_run.id() == step_run.job_run_id())
            {
                job_run
                    .apply(JobRunTransition::Cancel)
                    .map_err(|error| error.to_string())?;
            }
        }
        Ok(Some(run.clone()))
    }

    async fn resume_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<WorkflowRun>, Self::Error> {
        if !self.owned_by(
            "workflow_runs",
            workflow_run_id.as_str(),
            tenant_id,
            project_id,
        )? {
            return Ok(None);
        }
        let mut runs = self
            .workflow_runs
            .lock()
            .map_err(|error| error.to_string())?;
        let Some(run) = runs.iter_mut().find(|run| run.id() == workflow_run_id) else {
            return Ok(None);
        };
        if matches!(
            run.status(),
            WorkflowRunStatus::Failed | WorkflowRunStatus::TimedOut
        ) {
            *run = run.clone().with_status(WorkflowRunStatus::Running);
            self.workflow_step_runs
                .lock()
                .map_err(|error| error.to_string())?
                .retain(|step| {
                    step.workflow_run_id() != workflow_run_id
                        || step.status() == WorkflowRunStatus::Succeeded
                });
        }
        Ok(Some(run.clone()))
    }

    async fn list_workflow_step_runs(
        &self,
        workflow_run_id: &WorkflowRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<WorkflowStepRun>, Self::Error> {
        if !self.owned_by(
            "workflow_runs",
            workflow_run_id.as_str(),
            tenant_id,
            project_id,
        )? {
            return Ok(Vec::new());
        }
        Ok(self
            .workflow_step_runs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|step_run| step_run.workflow_run_id() == workflow_run_id)
            .cloned()
            .collect())
    }
}
