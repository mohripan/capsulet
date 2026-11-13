//! Job definitions, job runs, their logs and their artifacts.

use super::super::*;

#[async_trait::async_trait]
impl JobStore for FakeStore {
    async fn job_definition_exists(&self, id: &JobDefinitionId) -> Result<bool, Self::Error> {
        Ok(self
            .known_definitions
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .any(|known| known == id.as_str()))
    }

    async fn create_job_run(
        &self,
        run: &JobRun,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<bool, Self::Error> {
        if !self.owns(
            "job_definitions",
            run.job_definition_id().as_str(),
            tenant_id,
            project_id,
        ) {
            return Err("scope mismatch".to_string());
        }
        let mut runs = self.runs.lock().map_err(|error| error.to_string())?;
        if runs.iter().any(|existing| existing.id() == run.id()) {
            return if self.owns("job_runs", run.id().as_str(), tenant_id, project_id) {
                Ok(false)
            } else {
                Err("scope mismatch".to_string())
            };
        }
        self.claim_resource("job_runs", run.id().as_str(), tenant_id, project_id)?;
        runs.push(run.clone());
        Ok(true)
    }

    async fn upsert_job_definition(
        &self,
        definition: &JobDefinition,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error> {
        let mut ownership = self.ownership.lock().map_err(|error| error.to_string())?;
        if let Some(owner) = ownership.iter().find(|owner| {
            owner.resource == "job_definitions" && owner.id == definition.id().as_str()
        }) {
            if owner.tenant_id != tenant_id || owner.project_id != project_id {
                return Err("scope mismatch".to_string());
            }
        } else {
            ownership.push(FakeOwnership {
                resource: "job_definitions".to_string(),
                id: definition.id().as_str().to_string(),
                tenant_id: tenant_id.to_string(),
                project_id: project_id.to_string(),
            });
        }
        let mut records = self
            .job_definitions
            .lock()
            .map_err(|error| error.to_string())?;
        records.retain(|known| known.id() != definition.id());
        records.push(definition.clone());
        drop(records);
        let mut definitions = self
            .known_definitions
            .lock()
            .map_err(|error| error.to_string())?;
        if !definitions
            .iter()
            .any(|known| known == definition.id().as_str())
        {
            definitions.push(definition.id().as_str().to_string());
        }
        Ok(())
    }

    async fn list_job_definitions(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<JobDefinition>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        let ownership = self.ownership.lock().map_err(|error| error.to_string())?;
        Ok(self
            .known_definitions
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|id| {
                ownership
                    .iter()
                    .find(|owner| owner.resource == "job_definitions" && &owner.id == *id)
                    .map_or(tenant_id == "default" && project_id == "default", |owner| {
                        owner.tenant_id == tenant_id && owner.project_id == project_id
                    })
            })
            .take(limit)
            .map(|id| {
                JobDefinition::new(
                    JobDefinitionId::new(id.clone()).expect("fake definition id"),
                    id.clone(),
                    "python:3.12-slim",
                    vec![
                        "python".to_string(),
                        "-c".to_string(),
                        "print('fake')".to_string(),
                    ],
                    Vec::new(),
                    format!("bundles/{id}.py"),
                    "{}",
                    capsulet_core::RetryPolicy::no_retry(),
                )
                .expect("fake definition")
            })
            .collect())
    }

    async fn find_job_definition(
        &self,
        id: &JobDefinitionId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<JobDefinition>, Self::Error> {
        if self
            .resource_project("job_definitions", id.as_str())
            .await?
            != Some((tenant_id.to_string(), project_id.to_string()))
        {
            return Ok(None);
        }
        if let Some(definition) = self
            .job_definitions
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|definition| definition.id() == id)
            .cloned()
        {
            return Ok(Some(definition));
        }
        Ok(self
            .known_definitions
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|known| *known == id.as_str())
            .map(|known| {
                JobDefinition::new(
                    id.clone(),
                    known.clone(),
                    "python:3.12-slim",
                    vec![
                        "python".to_string(),
                        "-c".to_string(),
                        "print('fake')".to_string(),
                    ],
                    Vec::new(),
                    format!("bundles/{known}.py"),
                    "{}",
                    capsulet_core::RetryPolicy::no_retry(),
                )
                .expect("fake definition")
            }))
    }

    async fn delete_job_definition(
        &self,
        id: &JobDefinitionId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<bool, Self::Error> {
        if self
            .resource_project("job_definitions", id.as_str())
            .await?
            != Some((tenant_id.to_string(), project_id.to_string()))
        {
            return Ok(false);
        }
        self.job_definitions
            .lock()
            .map_err(|error| error.to_string())?
            .retain(|definition| definition.id() != id);
        let mut definitions = self
            .known_definitions
            .lock()
            .map_err(|error| error.to_string())?;
        let initial_len = definitions.len();
        definitions.retain(|known| known != id.as_str());
        Ok(definitions.len() != initial_len)
    }

    async fn job_definition_has_active_workflow_runs(
        &self,
        id: &JobDefinitionId,
    ) -> Result<bool, Self::Error> {
        let workflow_ids: Vec<_> = self
            .workflows
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|workflow| {
                workflow
                    .steps()
                    .iter()
                    .any(|step| step.job_definition_id() == id)
            })
            .map(|workflow| workflow.id().clone())
            .collect();
        Ok(self
            .workflow_runs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .any(|run| {
                workflow_ids.contains(run.workflow_id())
                    && matches!(
                        run.status(),
                        WorkflowRunStatus::Queued | WorkflowRunStatus::Running
                    )
            }))
    }

    async fn job_definition_is_used_by_workflows(
        &self,
        id: &JobDefinitionId,
    ) -> Result<bool, Self::Error> {
        Ok(self
            .workflows
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .any(|workflow| {
                workflow
                    .steps()
                    .iter()
                    .any(|step| step.job_definition_id() == id)
            }))
    }

    async fn list_runs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<JobRun>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        let runs = self.runs.lock().map_err(|error| error.to_string())?.clone();
        Ok(runs
            .into_iter()
            .filter(|run| self.owns("job_runs", run.id().as_str(), tenant_id, project_id))
            .take(limit)
            .collect())
    }

    async fn job_run_exists(&self, id: &JobRunId) -> Result<bool, Self::Error> {
        Ok(self
            .runs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .any(|run| run.id() == id))
    }

    async fn find_run(
        &self,
        id: &JobRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<JobRun>, Self::Error> {
        if !self.owned_by("job_runs", id.as_str(), tenant_id, project_id)? {
            return Ok(None);
        }
        Ok(self
            .runs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|run| run.id() == id)
            .cloned())
    }

    async fn find_run_log(
        &self,
        id: &JobRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<JobRunLog>, Self::Error> {
        if !self.owned_by("job_runs", id.as_str(), tenant_id, project_id)? {
            return Ok(None);
        }
        Ok(self
            .logs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|log| log.run_id == *id)
            .cloned())
    }

    async fn cancel_run(
        &self,
        id: &JobRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<JobRun>, Self::Error> {
        if !self.owned_by("job_runs", id.as_str(), tenant_id, project_id)? {
            return Ok(None);
        }
        let mut runs = self.runs.lock().map_err(|error| error.to_string())?;
        let Some(run) = runs.iter_mut().rev().find(|run| run.id() == id) else {
            return Ok(None);
        };
        if !run.status().is_terminal() {
            run.apply(JobRunTransition::Cancel)
                .map_err(|error| error.to_string())?;
        }
        Ok(Some(run.clone()))
    }

    async fn list_artifacts(
        &self,
        id: &JobRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<JobArtifact>, Self::Error> {
        if !self.owned_by("job_runs", id.as_str(), tenant_id, project_id)? {
            return Ok(Vec::new());
        }
        Ok(self
            .artifacts
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|artifact| artifact.run_id() == id)
            .cloned()
            .collect())
    }

    async fn find_artifact(
        &self,
        run_id: &JobRunId,
        artifact_id: &ArtifactId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<JobArtifact>, Self::Error> {
        if !self.owned_by("job_runs", run_id.as_str(), tenant_id, project_id)? {
            return Ok(None);
        }
        Ok(self
            .artifacts
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|artifact| artifact.run_id() == run_id && artifact.id() == artifact_id)
            .cloned())
    }

    async fn save_artifact(&self, artifact: &JobArtifact) -> Result<(), Self::Error> {
        self.artifacts
            .lock()
            .map_err(|error| error.to_string())?
            .push(artifact.clone());
        Ok(())
    }
}
