use std::sync::{Arc, Mutex};

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request},
};
use capsulet_application::AgentRunRecord;
use capsulet_core::{
    AgentDefinition, AgentId, AgentRunId, ArtifactId, ArtifactObjectKind, Automation, AutomationId,
    AutomationStatus, AutomationTrigger, Claim, ClaimId, CustomTriggerPlugin, Entity, EntityId,
    Event, EventId, Evidence, EvidenceId, ExecutionPoolName, GraphDefinition, GraphId, JobArtifact,
    JobDefinition, JobDefinitionId, JobRun, JobRunId, JobRunLog, JobRunStatus, JobRunTransition,
    MemoryContract, MemoryContractId, Relationship, RelationshipId, Source, SourceId,
    WorkflowDefinition, WorkflowId, WorkflowRun, WorkflowRunId, WorkflowRunStatus, WorkflowStatus,
    WorkflowStep, WorkflowStepId, WorkflowStepRun, WorkflowStepRunId,
};
use capsulet_postgres::{
    AdmissionSnapshot, NewProjectMembership, ProjectMembershipRecord, ProjectRecord, TriggerEvent,
};
use capsulet_storage::ObjectStore;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use super::{ApiStore, AppState, AuthConfig, router};
use crate::state::AdmissionConfig;

#[derive(Debug, Clone, Default)]
struct FakeStore {
    known_definitions: Arc<Mutex<Vec<String>>>,
    job_definitions: Arc<Mutex<Vec<JobDefinition>>>,
    runs: Arc<Mutex<Vec<JobRun>>>,
    logs: Arc<Mutex<Vec<JobRunLog>>>,
    artifacts: Arc<Mutex<Vec<JobArtifact>>>,
    workflows: Arc<Mutex<Vec<WorkflowDefinition>>>,
    automations: Arc<Mutex<Vec<Automation>>>,
    automation_triggers: Arc<Mutex<Vec<AutomationTrigger>>>,
    automation_conditions: Arc<Mutex<Vec<(String, String)>>>,
    trigger_plugins: Arc<Mutex<Vec<CustomTriggerPlugin>>>,
    workflow_runs: Arc<Mutex<Vec<WorkflowRun>>>,
    workflow_step_runs: Arc<Mutex<Vec<WorkflowStepRun>>>,
    graphs: Arc<Mutex<Vec<GraphDefinition>>>,
    agents: Arc<Mutex<Vec<AgentDefinition>>>,
    agent_runs: Arc<Mutex<Vec<AgentRunRecord>>>,
    memory_sources: Arc<Mutex<Vec<Source>>>,
    memory_evidence: Arc<Mutex<Vec<Evidence>>>,
    memory_entities: Arc<Mutex<Vec<Entity>>>,
    memory_claims: Arc<Mutex<Vec<Claim>>>,
    memory_events: Arc<Mutex<Vec<Event>>>,
    memory_relationships: Arc<Mutex<Vec<Relationship>>>,
    memory_contracts: Arc<Mutex<Vec<MemoryContract>>>,
    projects: Arc<Mutex<Vec<ProjectRecord>>>,
    project_memberships: Arc<Mutex<Vec<ProjectMembershipRecord>>>,
    ownership: Arc<Mutex<Vec<FakeOwnership>>>,
}

fn queued_run(id: &str, pool: &str) -> JobRun {
    JobRun::new(
        JobRunId::new(id).expect("run id"),
        JobDefinitionId::new("job_hello_python").expect("definition id"),
        ExecutionPoolName::new(pool).expect("pool"),
    )
}

fn queued_workflow_run(id: &str, workflow_id: &str) -> WorkflowRun {
    WorkflowRun::new(
        WorkflowRunId::new(id).expect("workflow run id"),
        WorkflowId::new(workflow_id).expect("workflow id"),
        None,
        "{}",
        WorkflowRunStatus::Queued,
        0,
        "",
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FakeOwnership {
    resource: String,
    id: String,
    tenant_id: String,
    project_id: String,
}

#[derive(Debug, Clone, Default)]
struct FakeObjectStore {
    objects: ObjectMap,
}

type ObjectMap = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

#[async_trait::async_trait]
impl ObjectStore for FakeObjectStore {
    type Error = String;

    async fn put(&self, key: &str, bytes: Vec<u8>) -> Result<(), Self::Error> {
        self.objects
            .lock()
            .map_err(|error| error.to_string())?
            .push((key.to_string(), bytes));
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self
            .objects
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .rev()
            .find(|(stored_key, _)| stored_key == key)
            .map(|(_, bytes)| bytes.clone()))
    }

    async fn exists(&self, key: &str) -> Result<bool, Self::Error> {
        Ok(self
            .objects
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .any(|(stored_key, _)| stored_key == key))
    }

    async fn delete(&self, key: &str) -> Result<(), Self::Error> {
        self.objects
            .lock()
            .map_err(|error| error.to_string())?
            .retain(|(stored_key, _)| stored_key != key);
        Ok(())
    }
}

#[async_trait::async_trait]
impl ApiStore for FakeStore {
    type Error = String;

    async fn ping(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn admission_snapshot(
        &self,
        execution_pool: &str,
    ) -> Result<AdmissionSnapshot, Self::Error> {
        let runs = self.runs.lock().map_err(|error| error.to_string())?;
        let queued_runs = runs
            .iter()
            .filter(|run| run.status() == JobRunStatus::Queued)
            .count();
        let queued_runs_in_pool = runs
            .iter()
            .filter(|run| {
                run.status() == JobRunStatus::Queued
                    && run.execution_pool().as_str() == execution_pool
            })
            .count();
        drop(runs);
        let queued_workflow_runs = self
            .workflow_runs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|run| run.status() == WorkflowRunStatus::Queued)
            .count();

        Ok(AdmissionSnapshot {
            queued_runs: u64::try_from(queued_runs).map_err(|error| error.to_string())?,
            queued_runs_in_pool: u64::try_from(queued_runs_in_pool)
                .map_err(|error| error.to_string())?,
            queued_workflow_runs: u64::try_from(queued_workflow_runs)
                .map_err(|error| error.to_string())?,
        })
    }

    async fn list_projects(
        &self,
        tenant_id: &str,
        project_ids: &[String],
    ) -> Result<Vec<ProjectRecord>, Self::Error> {
        Ok(self
            .projects
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|project| {
                project.tenant_id == tenant_id && project_ids.iter().any(|id| id == &project.id)
            })
            .cloned()
            .collect())
    }

    async fn list_all_projects(&self, tenant_id: &str) -> Result<Vec<ProjectRecord>, Self::Error> {
        Ok(self
            .projects
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|project| project.tenant_id == tenant_id)
            .cloned()
            .collect())
    }

    async fn list_project_memberships(
        &self,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<ProjectMembershipRecord>, Self::Error> {
        Ok(self
            .project_memberships
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|membership| {
                membership.tenant_id == tenant_id && membership.project_id == project_id
            })
            .cloned()
            .collect())
    }

    async fn list_principal_project_memberships(
        &self,
        tenant_id: &str,
        principal_name: &str,
    ) -> Result<Vec<ProjectMembershipRecord>, Self::Error> {
        Ok(self
            .project_memberships
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|membership| {
                membership.tenant_id == tenant_id && membership.principal_name == principal_name
            })
            .cloned()
            .collect())
    }

    async fn upsert_project_membership(
        &self,
        membership: &NewProjectMembership,
    ) -> Result<ProjectMembershipRecord, Self::Error> {
        let mut records = self
            .project_memberships
            .lock()
            .map_err(|error| error.to_string())?;
        records.retain(|record| {
            !(record.tenant_id == membership.tenant_id
                && record.project_id == membership.project_id
                && record.principal_kind == membership.principal_kind
                && record.principal_name == membership.principal_name)
        });
        let record = ProjectMembershipRecord {
            id: membership.id.clone(),
            tenant_id: membership.tenant_id.clone(),
            project_id: membership.project_id.clone(),
            principal_kind: membership.principal_kind.clone(),
            principal_name: membership.principal_name.clone(),
            role: membership.role.clone(),
            created_by: membership.created_by.clone(),
            created_at: "2026-06-24 00:00:00+00".to_string(),
            updated_at: "2026-06-24 00:00:00+00".to_string(),
        };
        records.push(record.clone());
        Ok(record)
    }

    async fn delete_project_membership(
        &self,
        tenant_id: &str,
        project_id: &str,
        principal_kind: &str,
        principal_name: &str,
    ) -> Result<bool, Self::Error> {
        let mut records = self
            .project_memberships
            .lock()
            .map_err(|error| error.to_string())?;
        let initial_len = records.len();
        records.retain(|record| {
            !(record.tenant_id == tenant_id
                && record.project_id == project_id
                && record.principal_kind == principal_kind
                && record.principal_name == principal_name)
        });
        Ok(records.len() != initial_len)
    }

    async fn resource_project(
        &self,
        resource: &str,
        id: &str,
    ) -> Result<Option<(String, String)>, Self::Error> {
        Ok(self
            .ownership
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|ownership| ownership.resource == resource && ownership.id == id)
            .map(|ownership| (ownership.tenant_id.clone(), ownership.project_id.clone()))
            .or_else(|| Some(("default".to_string(), "default".to_string()))))
    }

    async fn set_resource_project(
        &self,
        resource: &str,
        id: &str,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error> {
        let mut ownership = self.ownership.lock().map_err(|error| error.to_string())?;
        ownership.retain(|ownership| !(ownership.resource == resource && ownership.id == id));
        ownership.push(FakeOwnership {
            resource: resource.to_string(),
            id: id.to_string(),
            tenant_id: tenant_id.to_string(),
            project_id: project_id.to_string(),
        });
        Ok(())
    }

    async fn enqueue_trigger_event(
        &self,
        _event: &TriggerEvent,
        _idempotency_key: &str,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn job_definition_exists(&self, id: &JobDefinitionId) -> Result<bool, Self::Error> {
        Ok(self
            .known_definitions
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .any(|known| known == id.as_str()))
    }

    async fn save_run(&self, run: &JobRun) -> Result<(), Self::Error> {
        self.runs
            .lock()
            .map_err(|error| error.to_string())?
            .push(run.clone());
        Ok(())
    }

    async fn upsert_job_definition(&self, definition: &JobDefinition) -> Result<(), Self::Error> {
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

    async fn list_job_definitions(&self, limit: i64) -> Result<Vec<JobDefinition>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .known_definitions
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
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
    ) -> Result<Option<JobDefinition>, Self::Error> {
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

    async fn delete_job_definition(&self, id: &JobDefinitionId) -> Result<bool, Self::Error> {
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

    async fn upsert_workflow(&self, workflow: &WorkflowDefinition) -> Result<(), Self::Error> {
        let mut workflows = self.workflows.lock().map_err(|error| error.to_string())?;
        workflows.retain(|existing| existing.id() != workflow.id());
        workflows.push(workflow.clone());
        Ok(())
    }

    async fn list_workflows(&self, limit: i64) -> Result<Vec<WorkflowDefinition>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .workflows
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_workflow(
        &self,
        id: &WorkflowId,
    ) -> Result<Option<WorkflowDefinition>, Self::Error> {
        Ok(self
            .workflows
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|workflow| workflow.id().clone() == *id)
            .cloned())
    }

    async fn upsert_graph(&self, graph: &GraphDefinition) -> Result<(), Self::Error> {
        let mut graphs = self.graphs.lock().map_err(|error| error.to_string())?;
        graphs.retain(|existing| existing.id() != graph.id());
        graphs.push(graph.clone());
        Ok(())
    }

    async fn list_graphs(&self, limit: i64) -> Result<Vec<GraphDefinition>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .graphs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_graph(&self, id: &GraphId) -> Result<Option<GraphDefinition>, Self::Error> {
        Ok(self
            .graphs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|graph| graph.id() == id)
            .cloned())
    }

    async fn upsert_agent(&self, agent: &AgentDefinition) -> Result<(), Self::Error> {
        let mut agents = self.agents.lock().map_err(|error| error.to_string())?;
        agents.retain(|existing| existing.id() != agent.id());
        agents.push(agent.clone());
        Ok(())
    }

    async fn list_agents(&self, limit: i64) -> Result<Vec<AgentDefinition>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .agents
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_agent(&self, id: &AgentId) -> Result<Option<AgentDefinition>, Self::Error> {
        Ok(self
            .agents
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|agent| agent.id() == id)
            .cloned())
    }

    async fn upsert_agent_run(&self, run: &AgentRunRecord) -> Result<(), Self::Error> {
        let mut runs = self.agent_runs.lock().map_err(|error| error.to_string())?;
        runs.retain(|existing| existing.id != run.id);
        runs.push(run.clone());
        Ok(())
    }

    async fn list_agent_runs(&self, limit: i64) -> Result<Vec<AgentRunRecord>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .agent_runs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_agent_run(&self, id: &AgentRunId) -> Result<Option<AgentRunRecord>, Self::Error> {
        Ok(self
            .agent_runs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|run| run.id == *id)
            .cloned())
    }

    async fn upsert_memory_source(&self, source: &Source) -> Result<(), Self::Error> {
        let mut sources = self
            .memory_sources
            .lock()
            .map_err(|error| error.to_string())?;
        sources.retain(|existing| existing.id() != source.id());
        sources.push(source.clone());
        Ok(())
    }

    async fn list_memory_sources(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Source>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .memory_sources
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|source| {
                source.scope().tenant_id() == tenant_id && source.scope().project_id() == project_id
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_memory_source(&self, id: &SourceId) -> Result<Option<Source>, Self::Error> {
        Ok(self
            .memory_sources
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|source| source.id() == id)
            .cloned())
    }

    async fn upsert_memory_evidence(&self, evidence: &Evidence) -> Result<(), Self::Error> {
        let mut records = self
            .memory_evidence
            .lock()
            .map_err(|error| error.to_string())?;
        records.retain(|existing| existing.id() != evidence.id());
        records.push(evidence.clone());
        Ok(())
    }

    async fn list_memory_evidence(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Evidence>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .memory_evidence
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|evidence| {
                evidence.scope().tenant_id() == tenant_id
                    && evidence.scope().project_id() == project_id
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_memory_evidence(&self, id: &EvidenceId) -> Result<Option<Evidence>, Self::Error> {
        Ok(self
            .memory_evidence
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|evidence| evidence.id() == id)
            .cloned())
    }

    async fn upsert_memory_entity(&self, entity: &Entity) -> Result<(), Self::Error> {
        let mut entities = self
            .memory_entities
            .lock()
            .map_err(|error| error.to_string())?;
        entities.retain(|existing| existing.id() != entity.id());
        entities.push(entity.clone());
        Ok(())
    }

    async fn list_memory_entities(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Entity>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .memory_entities
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|entity| {
                entity.scope().tenant_id() == tenant_id && entity.scope().project_id() == project_id
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_memory_entity(&self, id: &EntityId) -> Result<Option<Entity>, Self::Error> {
        Ok(self
            .memory_entities
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|entity| entity.id() == id)
            .cloned())
    }

    async fn upsert_memory_claim(&self, claim: &Claim) -> Result<(), Self::Error> {
        let mut claims = self
            .memory_claims
            .lock()
            .map_err(|error| error.to_string())?;
        claims.retain(|existing| existing.id() != claim.id());
        claims.push(claim.clone());
        Ok(())
    }

    async fn list_memory_claims(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Claim>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .memory_claims
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|claim| {
                claim.scope().tenant_id() == tenant_id && claim.scope().project_id() == project_id
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_memory_claim(&self, id: &ClaimId) -> Result<Option<Claim>, Self::Error> {
        Ok(self
            .memory_claims
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|claim| claim.id() == id)
            .cloned())
    }

    async fn upsert_memory_event(&self, event: &Event) -> Result<(), Self::Error> {
        let mut events = self
            .memory_events
            .lock()
            .map_err(|error| error.to_string())?;
        events.retain(|existing| existing.id() != event.id());
        events.push(event.clone());
        Ok(())
    }

    async fn list_memory_events(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Event>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .memory_events
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|event| {
                event.scope().tenant_id() == tenant_id && event.scope().project_id() == project_id
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_memory_event(&self, id: &EventId) -> Result<Option<Event>, Self::Error> {
        Ok(self
            .memory_events
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|event| event.id() == id)
            .cloned())
    }

    async fn upsert_memory_relationship(
        &self,
        relationship: &Relationship,
    ) -> Result<(), Self::Error> {
        let mut relationships = self
            .memory_relationships
            .lock()
            .map_err(|error| error.to_string())?;
        relationships.retain(|existing| existing.id() != relationship.id());
        relationships.push(relationship.clone());
        Ok(())
    }

    async fn list_memory_relationships(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Relationship>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .memory_relationships
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|relationship| {
                relationship.scope().tenant_id() == tenant_id
                    && relationship.scope().project_id() == project_id
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_memory_relationship(
        &self,
        id: &RelationshipId,
    ) -> Result<Option<Relationship>, Self::Error> {
        Ok(self
            .memory_relationships
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|relationship| relationship.id() == id)
            .cloned())
    }

    async fn upsert_memory_contract(&self, contract: &MemoryContract) -> Result<(), Self::Error> {
        let mut contracts = self
            .memory_contracts
            .lock()
            .map_err(|error| error.to_string())?;
        contracts.retain(|existing| existing.id() != contract.id());
        contracts.push(contract.clone());
        Ok(())
    }

    async fn list_memory_contracts(&self, limit: i64) -> Result<Vec<MemoryContract>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .memory_contracts
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_memory_contract(
        &self,
        id: &MemoryContractId,
    ) -> Result<Option<MemoryContract>, Self::Error> {
        Ok(self
            .memory_contracts
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|contract| contract.id() == id)
            .cloned())
    }

    async fn delete_workflow(&self, id: &WorkflowId) -> Result<bool, Self::Error> {
        let mut workflows = self.workflows.lock().map_err(|error| error.to_string())?;
        let before = workflows.len();
        workflows.retain(|workflow| workflow.id() != id);
        Ok(workflows.len() != before)
    }

    async fn upsert_automation(&self, automation: &Automation) -> Result<(), Self::Error> {
        let mut automations = self.automations.lock().map_err(|error| error.to_string())?;
        automations.retain(|existing| existing.id() != automation.id());
        automations.push(automation.clone());
        Ok(())
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

    async fn list_automations(&self, limit: i64) -> Result<Vec<Automation>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .automations
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_automation(&self, id: &AutomationId) -> Result<Option<Automation>, Self::Error> {
        Ok(self
            .automations
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|automation| automation.id().clone() == *id)
            .cloned())
    }

    async fn set_automation_status(
        &self,
        id: &AutomationId,
        status: AutomationStatus,
    ) -> Result<Option<Automation>, Self::Error> {
        let mut automations = self.automations.lock().map_err(|error| error.to_string())?;
        let Some(automation) = automations
            .iter_mut()
            .find(|automation| automation.id().clone() == *id)
        else {
            return Ok(None);
        };
        *automation = automation.clone().with_status(status);
        Ok(Some(automation.clone()))
    }

    async fn delete_automation(&self, id: &AutomationId) -> Result<bool, Self::Error> {
        let mut automations = self.automations.lock().map_err(|error| error.to_string())?;
        let initial_len = automations.len();
        automations.retain(|automation| automation.id().clone() != *id);
        let deleted = automations.len() != initial_len;
        drop(automations);

        if deleted {
            self.automation_triggers
                .lock()
                .map_err(|error| error.to_string())?
                .retain(|trigger| trigger.automation_id() != id);
            self.automation_conditions
                .lock()
                .map_err(|error| error.to_string())?
                .retain(|(stored_id, _)| stored_id != id.as_str());
        }
        Ok(deleted)
    }

    async fn replace_automation_triggers(
        &self,
        automation_id: &AutomationId,
        triggers: &[AutomationTrigger],
        condition_json: &str,
    ) -> Result<(), Self::Error> {
        let mut stored_triggers = self
            .automation_triggers
            .lock()
            .map_err(|error| error.to_string())?;
        stored_triggers.retain(|trigger| trigger.automation_id() != automation_id);
        stored_triggers.extend(triggers.iter().cloned());
        let mut conditions = self
            .automation_conditions
            .lock()
            .map_err(|error| error.to_string())?;
        conditions.retain(|(id, _)| id != automation_id.as_str());
        conditions.push((
            automation_id.as_str().to_string(),
            condition_json.to_string(),
        ));
        Ok(())
    }

    async fn list_automation_triggers(
        &self,
        automation_id: &AutomationId,
    ) -> Result<(Vec<AutomationTrigger>, String), Self::Error> {
        let triggers = self
            .automation_triggers
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|trigger| trigger.automation_id() == automation_id)
            .cloned()
            .collect();
        let condition = self
            .automation_conditions
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|(id, _)| id == automation_id.as_str())
            .map_or_else(|| "{}".to_string(), |(_, condition)| condition.clone());
        Ok((triggers, condition))
    }

    async fn upsert_custom_trigger_plugin(
        &self,
        plugin: &CustomTriggerPlugin,
    ) -> Result<(), Self::Error> {
        let mut plugins = self
            .trigger_plugins
            .lock()
            .map_err(|error| error.to_string())?;
        plugins.retain(|existing| existing.id() != plugin.id());
        plugins.push(plugin.clone());
        Ok(())
    }

    async fn list_custom_trigger_plugins(
        &self,
        limit: i64,
    ) -> Result<Vec<CustomTriggerPlugin>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .trigger_plugins
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_custom_trigger_plugin(
        &self,
        id: &str,
    ) -> Result<Option<CustomTriggerPlugin>, Self::Error> {
        Ok(self
            .trigger_plugins
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|plugin| plugin.id() == id)
            .cloned())
    }

    async fn create_workflow_run(
        &self,
        workflow_id: &WorkflowId,
        automation_id: Option<&AutomationId>,
        run_id: &WorkflowRunId,
        input_json: &str,
    ) -> Result<WorkflowRun, Self::Error> {
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

    async fn list_workflow_runs(&self, limit: i64) -> Result<Vec<WorkflowRun>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .workflow_runs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
    ) -> Result<Option<WorkflowRun>, Self::Error> {
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
    ) -> Result<Option<WorkflowRun>, Self::Error> {
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
    ) -> Result<Option<WorkflowRun>, Self::Error> {
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
    ) -> Result<Option<WorkflowRun>, Self::Error> {
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
    ) -> Result<Vec<WorkflowStepRun>, Self::Error> {
        Ok(self
            .workflow_step_runs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|step_run| step_run.workflow_run_id() == workflow_run_id)
            .cloned()
            .collect())
    }

    async fn list_runs(&self, limit: i64) -> Result<Vec<JobRun>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .runs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_run(&self, id: &JobRunId) -> Result<Option<JobRun>, Self::Error> {
        Ok(self
            .runs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|run| run.id() == id)
            .cloned())
    }

    async fn find_run_log(&self, id: &JobRunId) -> Result<Option<JobRunLog>, Self::Error> {
        Ok(self
            .logs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|log| log.run_id == *id)
            .cloned())
    }

    async fn cancel_run(&self, id: &JobRunId) -> Result<Option<JobRun>, Self::Error> {
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

    async fn list_artifacts(&self, id: &JobRunId) -> Result<Vec<JobArtifact>, Self::Error> {
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
    ) -> Result<Option<JobArtifact>, Self::Error> {
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

impl FakeStore {
    fn with_definition(id: &str) -> Self {
        let store = Self::default();
        store.ensure_default_projects();
        store
            .known_definitions
            .lock()
            .expect("definition mutex")
            .push(id.to_string());
        store
    }

    fn ensure_default_projects(&self) {
        let mut projects = self.projects.lock().expect("projects mutex");
        if !projects.is_empty() {
            return;
        }
        projects.push(ProjectRecord {
            id: "default".to_string(),
            tenant_id: "default".to_string(),
            name: "Default Project".to_string(),
        });
        projects.push(ProjectRecord {
            id: "finance".to_string(),
            tenant_id: "default".to_string(),
            name: "Finance".to_string(),
        });
        drop(projects);
        let mut memberships = self
            .project_memberships
            .lock()
            .expect("project memberships mutex");
        memberships.push(ProjectMembershipRecord {
            id: "default_owner".to_string(),
            tenant_id: "default".to_string(),
            project_id: "default".to_string(),
            principal_kind: "user".to_string(),
            principal_name: "owner".to_string(),
            role: "project_admin".to_string(),
            created_by: "system".to_string(),
            created_at: "2026-06-24 00:00:00+00".to_string(),
            updated_at: "2026-06-24 00:00:00+00".to_string(),
        });
        memberships.push(ProjectMembershipRecord {
            id: "finance_user".to_string(),
            tenant_id: "default".to_string(),
            project_id: "finance".to_string(),
            principal_kind: "user".to_string(),
            principal_name: "finance".to_string(),
            role: "project_operator".to_string(),
            created_by: "system".to_string(),
            created_at: "2026-06-24 00:00:00+00".to_string(),
            updated_at: "2026-06-24 00:00:00+00".to_string(),
        });
    }

    fn with_workflow(self, id: &str) -> Self {
        let workflow_id = WorkflowId::new(id).expect("workflow id");
        self.workflows
            .lock()
            .expect("workflows mutex")
            .push(WorkflowDefinition::new(
                workflow_id.clone(),
                "Test workflow",
                "",
                WorkflowStatus::Enabled,
                vec![WorkflowStep::new(
                    WorkflowStepId::new(format!("{id}_step_1")).expect("step id"),
                    workflow_id,
                    1,
                    "Run job",
                    JobDefinitionId::new("job_hello_python").expect("job id"),
                    ExecutionPoolName::new("mini").expect("pool"),
                )],
            ));
        self
    }

    fn with_run(self, run: JobRun) -> Self {
        self.runs.lock().expect("runs mutex").push(run);
        self
    }

    fn with_log(self, log: JobRunLog) -> Self {
        self.logs.lock().expect("logs mutex").push(log);
        self
    }

    fn with_artifact(self, artifact: JobArtifact) -> Self {
        self.artifacts
            .lock()
            .expect("artifacts mutex")
            .push(artifact);
        self
    }

    fn with_workflow_run(self, run: WorkflowRun) -> Self {
        self.workflow_runs
            .lock()
            .expect("workflow runs mutex")
            .push(run);
        self
    }

    fn with_workflow_step_run(self, step_run: WorkflowStepRun) -> Self {
        self.workflow_step_runs
            .lock()
            .expect("workflow step runs mutex")
            .push(step_run);
        self
    }
}

fn test_app(store: FakeStore) -> axum::Router {
    test_app_with_admission(store, AdmissionConfig::default())
}

fn test_app_with_admission(store: FakeStore, admission: AdmissionConfig) -> axum::Router {
    let object_store = FakeObjectStore::default();
    object_store.objects.lock().expect("objects mutex").push((
        "artifacts/run_with_artifact/report.txt".to_string(),
        b"report".to_vec(),
    ));
    router(
        AppState::new(
            store,
            object_store,
            ["mini".to_string(), "large".to_string()],
        )
        .with_admission(admission),
    )
}

fn authenticated_app(store: FakeStore) -> axum::Router {
    store.ensure_default_projects();
    let auth = AuthConfig::from_json(
        r#"[
            {"name":"reader","role":"viewer","token":"viewer-token-0123456789-abcdefgh"},
            {"name":"runner","role":"operator","token":"operator-token-0123456789-abcdef"},
            {"name":"finance","role":"operator","token":"finance-token-0123456789-abcdefghijkl","project_id":"finance"},
            {"name":"ci-runner","role":"operator","token":"ci-runner-token-0123456789-abcdef","scopes":["jobs:run"]},
            {"name":"owner","role":"admin","token":"admin-token-0123456789-abcdefghijkl"}
        ]"#,
    )
    .expect("auth config");
    router(
        AppState::new(
            store,
            FakeObjectStore::default(),
            ["mini".to_string(), "large".to_string()],
        )
        .with_auth(auth),
    )
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("collect response")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("json response")
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end memory API contract covers source, evidence, entity, and claim wiring"
)]
async fn creates_lists_and_reads_claim_memory() {
    let app = authenticated_app(FakeStore::default());

    let source_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/memory/sources")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "source_exec_update",
                        "kind": "executive_update",
                        "uri": "file:///updates/atlas.md",
                        "title": "Atlas executive update",
                        "authority": "high"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(source_response.status(), axum::http::StatusCode::CREATED);

    let evidence_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/memory/evidence")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "evidence_exec_update",
                        "source_id": "source_exec_update",
                        "locator": "updates/atlas.md#L12",
                        "excerpt": "Project Atlas launch date moved to August 1.",
                        "observed_at": "2026-07-05T10:00:00Z"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(evidence_response.status(), axum::http::StatusCode::CREATED);

    let entity_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/memory/entities")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "project_atlas",
                        "entity_type": "Project",
                        "name": "Project Atlas",
                        "aliases": ["Atlas"]
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(entity_response.status(), axum::http::StatusCode::CREATED);

    let claim_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/memory/claims")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "claim_launch_august",
                        "subject_id": "project_atlas",
                        "predicate": "launch_date",
                        "object": "2026-08-01",
                        "evidence_ids": ["evidence_exec_update"],
                        "confidence": 0.93,
                        "authority": "high",
                        "status": "active",
                        "observed_at": "2026-07-05T10:00:00Z",
                        "valid_from": "2026-07-05T00:00:00Z"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(claim_response.status(), axum::http::StatusCode::CREATED);

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/memory/claims")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(list_response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(list_response).await["claims"][0]["evidence_ids"],
        json!(["evidence_exec_update"])
    );

    let get_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/memory/claims/claim_launch_august")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(get_response.status(), axum::http::StatusCode::OK);
    assert_eq!(response_json(get_response).await["status"], "active");
}

#[tokio::test]
async fn creates_lists_and_reads_memory_contract() {
    let app = authenticated_app(FakeStore::default());
    let dsl = r"
entity Project:
  fields:
    name: string
    owner: Person

entity Person:
  fields:
    name: string

relation owns:
  from: Person
  to: Project

claim_policy:
  require_source: true
  store_confidence: true
  allow_contradictions: true
  min_confidence: 0.8

trust_policy:
  source_priority:
    - legal_contracts
    - board_minutes
";

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/memory/contracts")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "contract_project_memory",
                        "name": "Project memory",
                        "source": dsl
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(create_response.status(), axum::http::StatusCode::CREATED);
    let body = response_json(create_response).await;
    assert_eq!(
        body["compiled"]["entity_types"],
        json!(["Project", "Person"])
    );
    assert_eq!(
        body["compiled"]["claim_policy"]["min_confidence"],
        json!(0.8)
    );

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/memory/contracts")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(list_response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(list_response).await["contracts"][0]["id"],
        "contract_project_memory"
    );

    let get_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/memory/contracts/contract_project_memory")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(get_response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(get_response).await["compiled"]["relations"][0]["name"],
        "owns"
    );
}

fn rag_graph_request(id: &str) -> Value {
    json!({
        "id": id,
        "name": "RAG Graph",
        "nodes": [
            {
                "id": "normalize",
                "name": "Normalize",
                "kind": "query_normalizer",
                "ports": [
                    { "id": "normalize.input", "direction": "input", "value_type": "user_query" },
                    { "id": "normalize.output", "direction": "output", "value_type": "normalized_query" }
                ]
            },
            {
                "id": "embed",
                "name": "Embed",
                "kind": "embedding",
                "ports": [
                    { "id": "embed.input", "direction": "input", "value_type": "normalized_query" },
                    { "id": "embed.output", "direction": "output", "value_type": "embedding_vector" }
                ]
            }
        ],
        "hyperedges": [
            {
                "id": "normalize_to_embed",
                "sources": [
                    { "kind": "port", "node_id": "normalize", "port_id": "normalize.output", "value_type": "normalized_query" }
                ],
                "targets": [
                    { "kind": "port", "node_id": "embed", "port_id": "embed.input", "value_type": "normalized_query" }
                ]
            }
        ],
        "transition_policy": {
            "mode": "static",
            "cycles_allowed": false
        }
    })
}

async fn create_graph(app: &axum::Router, id: &str) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/graphs")
                .header("content-type", "application/json")
                .body(Body::from(rag_graph_request(id).to_string()))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
}

async fn create_agent(app: &axum::Router, id: &str, graph_id: &str) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/agents")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": id,
                        "name": "RAG Agent",
                        "graph_id": graph_id,
                        "budget": {
                            "max_steps": 8,
                            "max_tokens": 4096,
                            "max_seconds": 30,
                        "max_cost_micros": 100_000
                        }
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
}

#[tokio::test]
async fn protected_routes_require_a_valid_bearer_token() {
    let app = authenticated_app(FakeStore::default());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/jobs/runs")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), 401);
    assert_eq!(
        response_json(response).await["code"],
        "authentication_required"
    );
}

#[tokio::test]
async fn activity_stream_requires_auth_and_uses_sse() {
    let app = authenticated_app(FakeStore::default());
    let unauthenticated_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events/stream")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(unauthenticated_response.status(), 401);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/stream")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), 200);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
}

#[tokio::test]
async fn rate_limit_is_partitioned_by_client_ip() {
    let app = test_app(FakeStore::default());
    for _ in 0..105 {
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/livez")
                    .header("x-forwarded-for", "10.10.10.1")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
    }

    let response = app
        .oneshot(
            Request::builder()
                .uri("/livez")
                .header("x-forwarded-for", "10.10.10.2")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
}

#[tokio::test]
async fn rate_limit_uses_bearer_token_before_local_fallback() {
    let app = authenticated_app(FakeStore::default());
    for _ in 0..105 {
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/livez")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
    }

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/auth/me")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
}

#[tokio::test]
async fn job_definitions_are_scoped_to_selected_project() {
    let app = authenticated_app(FakeStore::default());
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/job-definitions")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .header("x-capsulet-project-id", "finance")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "job_finance_report",
                        "name": "Finance report",
                        "python_script": "print('finance')"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(create_response.status(), 201);

    let default_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/job-definitions")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        response_json(default_response).await["job_definitions"],
        json!([])
    );

    let finance_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/job-definitions")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .header("x-capsulet-project-id", "finance")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        response_json(finance_response).await["job_definitions"][0]["id"],
        "job_finance_report"
    );
}

#[tokio::test]
async fn principal_cannot_select_project_without_membership() {
    let response = authenticated_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .uri("/v1/job-definitions")
                .header(
                    "authorization",
                    "Bearer finance-token-0123456789-abcdefghijkl",
                )
                .header("x-capsulet-project-id", "default")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn project_admin_can_manage_project_memberships() {
    let app = authenticated_app(FakeStore::default());
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/projects/finance/memberships")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "principal_kind": "user",
                        "principal_name": "developer",
                        "role": "project_operator"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(create_response.status(), 200);

    let list_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/projects/finance/memberships")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let body = response_json(list_response).await;
    assert!(
        body["memberships"]
            .as_array()
            .expect("memberships")
            .iter()
            .any(|membership| membership["principal_name"] == "developer"
                && membership["role"] == "project_operator")
    );
}

#[tokio::test]
async fn stored_project_admin_membership_can_manage_project_memberships() {
    let store = FakeStore::default();
    store.ensure_default_projects();
    store
        .project_memberships
        .lock()
        .expect("project memberships mutex")
        .push(ProjectMembershipRecord {
            id: "finance_reader_admin".to_string(),
            tenant_id: "default".to_string(),
            project_id: "finance".to_string(),
            principal_kind: "user".to_string(),
            principal_name: "reader".to_string(),
            role: "project_admin".to_string(),
            created_by: "system".to_string(),
            created_at: "2026-06-24 00:00:00+00".to_string(),
            updated_at: "2026-06-24 00:00:00+00".to_string(),
        });
    let app = authenticated_app(store);
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/projects/finance/memberships")
                .header("authorization", "Bearer viewer-token-0123456789-abcdefgh")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "principal_kind": "user",
                        "principal_name": "developer",
                        "role": "project_operator"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn viewer_cannot_submit_work_but_operator_can() {
    let app = authenticated_app(FakeStore::with_definition("auth_job"));
    let request_body = json!({
        "job_definition_id": "auth_job",
        "execution_pool": "mini"
    })
    .to_string();
    let viewer_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/jobs/runs")
                .header("content-type", "application/json")
                .header("authorization", "Bearer viewer-token-0123456789-abcdefgh")
                .body(Body::from(request_body.clone()))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(viewer_response.status(), 403);

    let operator_response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/jobs/runs")
                .header("content-type", "application/json")
                .header("authorization", "Bearer operator-token-0123456789-abcdef")
                .body(Body::from(request_body))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(operator_response.status(), 201);
}

#[tokio::test]
async fn current_principal_reports_authenticated_identity() {
    let response = authenticated_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .uri("/v1/auth/me")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), 200);
    assert_eq!(
        response_json(response).await,
        json!({
            "name":"owner",
            "role":"admin",
            "platform_admin":true,
            "tenant_id":"default",
            "project_id":"default",
            "project_memberships":[
                {"tenant_id":"default","project_id":"default","role":"project_admin"}
            ],
            "scopes":["*"]
        })
    );
}

#[tokio::test]
async fn current_principal_reports_project_memberships() {
    let response = authenticated_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .uri("/v1/auth/me")
                .header(
                    "authorization",
                    "Bearer finance-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), 200);
    assert_eq!(
        response_json(response).await["project_memberships"],
        json!([
            {"tenant_id":"default","project_id":"finance","role":"project_operator"}
        ])
    );
}

#[tokio::test]
async fn lists_only_projects_visible_to_the_principal() {
    let response = authenticated_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .uri("/v1/projects")
                .header(
                    "authorization",
                    "Bearer finance-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), 200);
    assert_eq!(
        response_json(response).await,
        json!({
            "projects": [
                {"id":"finance","tenant_id":"default","name":"Finance"}
            ]
        })
    );
}

#[tokio::test]
async fn scoped_token_can_only_use_declared_permissions() {
    let app = authenticated_app(FakeStore::with_definition("scoped_job"));
    let run_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/jobs/runs")
                .header("content-type", "application/json")
                .header("authorization", "Bearer ci-runner-token-0123456789-abcdef")
                .body(Body::from(
                    json!({
                        "job_definition_id": "scoped_job",
                        "execution_pool": "mini"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(run_response.status(), 201);

    let workflow_response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/workflows")
                .header("content-type", "application/json")
                .header(
                    "authorization",
                    "Bearer ci-runner-token-0123456789-abcdef",
                )
                .body(Body::from(
                    json!({
                        "id": "scoped_workflow",
                        "name": "Scoped Workflow",
                        "steps": [
                            { "id": "only", "name": "Only", "job_definition_id": "scoped_job", "execution_pool": "mini" }
                        ]
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(workflow_response.status(), 403);
}

#[tokio::test]
async fn creates_and_returns_workflow_dag_dependencies() {
    let app = test_app(FakeStore::with_definition("job_graph"));
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/workflows")
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "id": "workflow_graph",
                    "name": "Graph",
                    "steps": [
                        { "id": "root_a", "name": "A", "job_definition_id": "job_graph", "execution_pool": "mini" },
                        { "id": "root_b", "name": "B", "job_definition_id": "job_graph", "execution_pool": "mini" },
                        { "id": "merge", "name": "Merge", "job_definition_id": "job_graph", "execution_pool": "mini" }
                    ],
                    "dependencies": [
                        { "from_step_id": "root_a", "to_step_id": "merge" },
                        { "from_step_id": "root_b", "to_step_id": "merge" }
                    ]
                }).to_string())).expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    assert_eq!(
        response_json(response).await["dependencies"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/workflows/workflow_graph")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        response_json(response).await["dependencies"][1]["to_step_id"],
        "merge"
    );
}

#[tokio::test]
async fn rejects_cyclic_workflow_dependencies() {
    let app = test_app(FakeStore::with_definition("job_graph"));
    let response = app.oneshot(
        Request::builder().method(Method::POST).uri("/v1/workflows").header("content-type", "application/json")
            .body(Body::from(json!({
                "name": "Cycle",
                "steps": [
                    { "id": "a", "name": "A", "job_definition_id": "job_graph", "execution_pool": "mini" },
                    { "id": "b", "name": "B", "job_definition_id": "job_graph", "execution_pool": "mini" }
                ],
                "dependencies": [
                    { "from_step_id": "a", "to_step_id": "b" },
                    { "from_step_id": "b", "to_step_id": "a" }
                ]
            }).to_string())).expect("request")
    ).await.expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    assert!(
        response_json(response).await["message"]
            .as_str()
            .unwrap()
            .contains("cycle")
    );
}

#[tokio::test]
async fn creates_and_returns_typed_agent_graph() {
    let app = test_app(FakeStore::default());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/graphs")
                .header("content-type", "application/json")
                .body(Body::from(rag_graph_request("rag_graph").to_string()))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    let body = response_json(response).await;
    assert_eq!(body["id"], "rag_graph");
    assert_eq!(body["nodes"][0]["kind"], "query_normalizer");
    assert_eq!(body["transition_policy"]["mode"], "static");

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/graphs")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        response_json(list_response).await["graphs"][0]["id"],
        "rag_graph"
    );

    let get_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/graphs/rag_graph")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        response_json(get_response).await["hyperedges"][0]["targets"][0]["node_id"],
        "embed"
    );
}

#[tokio::test]
async fn rejects_invalid_typed_graph_wiring() {
    let app = test_app(FakeStore::default());
    let mut request = rag_graph_request("bad_graph");
    request["nodes"][1]["ports"][0]["value_type"] = json!("prompt");
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/graphs")
                .header("content-type", "application/json")
                .body(Body::from(request.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(response_json(response).await["code"], "validation_error");
}

#[tokio::test]
async fn creates_and_returns_agent_definition() {
    let app = test_app(FakeStore::default());
    create_graph(&app, "rag_agent_graph").await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/agents")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "rag_agent",
                        "name": "RAG Agent",
                        "graph_id": "rag_agent_graph",
                        "budget": {
                            "max_steps": 12,
                            "max_tokens": 8192,
                            "max_seconds": 60,
                        "max_cost_micros": 250_000
                        },
                        "termination_conditions": ["validator_pass", "safety_failure"]
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    assert_eq!(response_json(response).await["graph_id"], "rag_agent_graph");

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/agents")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        response_json(list_response).await["agents"][0]["id"],
        "rag_agent"
    );

    let get_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/agents/rag_agent")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let body = response_json(get_response).await;
    assert_eq!(body["budget"]["max_steps"], 12);
    assert_eq!(body["termination_conditions"][0], "validator_pass");
}

#[tokio::test]
async fn starts_and_returns_agent_run() {
    let app = test_app(FakeStore::default());
    create_graph(&app, "run_graph").await;
    create_agent(&app, "run_agent", "run_graph").await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/agents/run_agent/runs")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "agent_run_1",
                        "initial_state": { "query": "What is Capsulet?" }
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    let body = response_json(response).await;
    assert_eq!(body["id"], "agent_run_1");
    assert_eq!(body["agent_id"], "run_agent");
    assert_eq!(body["status"], "queued");
    assert_eq!(body["state_version"], 0);
    assert_eq!(body["state"]["query"], "What is Capsulet?");

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/agent-runs")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        response_json(list_response).await["agent_runs"][0]["id"],
        "agent_run_1"
    );

    let get_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/agent-runs/agent_run_1")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        response_json(get_response).await["state"]["query"],
        "What is Capsulet?"
    );
}

#[tokio::test]
async fn healthz_returns_ok() {
    let response = test_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(response_json(response).await, json!({ "status": "ok" }));
}

#[tokio::test]
async fn lists_configured_execution_pools() {
    let response = test_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .uri("/v1/execution-pools")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "execution_pools": [
                { "name": "mini", "description": "Default execution pool", "is_default": true, "host_group": "mini" },
                { "name": "large", "description": "Configured execution pool", "is_default": false, "host_group": "large" }
            ]
        })
    );

    let response = test_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .uri("/v1/host-groups")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "host_groups": [
                { "name": "mini", "description": "Default host group", "is_default": true, "execution_pool": "mini", "host_count": null },
                { "name": "large", "description": "Configured host group", "is_default": false, "execution_pool": "large", "host_count": null }
            ]
        })
    );
}

#[tokio::test]
async fn creates_and_lists_custom_trigger_plugins() {
    let app = test_app(FakeStore::default());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/trigger-plugins")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "plugin_customer_threshold",
                        "name": "Customer threshold",
                        "runtime_image": "python:3.12-slim",
                        "python_script": "import json\nprint(json.dumps({'matched': False}))",
                        "config_schema": { "type": "object" }
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    assert_eq!(
        response_json(response).await["id"],
        "plugin_customer_threshold"
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/trigger-plugins")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(
        body["trigger_plugins"][0]["runtime_image"],
        "python:3.12-slim"
    );
    assert_eq!(
        body["trigger_plugins"][0]["python_script"],
        "import json\nprint(json.dumps({'matched': False}))"
    );
}

#[tokio::test]
async fn creates_automation_with_trigger_condition_graph() {
    let store = FakeStore::with_definition("job_hello_python").with_workflow("wf_pipeline");
    store
        .trigger_plugins
        .lock()
        .expect("plugins mutex")
        .push(CustomTriggerPlugin::new(
            "plugin_threshold",
            "Threshold plugin",
            "",
            "python:3.12-slim",
            vec!["python".to_string(), "/plugin/check.py".to_string()],
            "{}",
        ));

    let response = test_app(store)
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/automations")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                            "id": "automation_customer_pipeline",
                            "name": "Customer pipeline",
                            "workflow_id": "wf_pipeline",
                            "triggers": [
                            {
                                "name": "nightly",
                                "kind": "schedule",
                                "config": { "interval_seconds": 300 }
                            },
                            {
                                "name": "orders_changed",
                                "kind": "sql",
                                "config": { "connection_name": "orders", "query": "select 1" }
                            },
                            {
                                "name": "threshold",
                                "kind": "custom",
                                "plugin_id": "plugin_threshold",
                                "config": { "limit": 10 }
                            }
                        ],
                        "condition": {
                            "all": [
                                { "trigger": "nightly" },
                                {
                                    "any": [
                                        { "trigger": "orders_changed" },
                                        { "trigger": "threshold" }
                                    ]
                                }
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    let body = response_json(response).await;
    assert!(body.get("trigger_kind").is_none());
    assert!(body.get("interval_seconds").is_none());
    assert_eq!(body["triggers"][2]["kind"], "custom");
    assert_eq!(
        body["condition"]["all"][1]["any"][0]["trigger"],
        "orders_changed"
    );
}

#[tokio::test]
async fn creates_cron_automation_without_legacy_interval_seconds() {
    let response =
        test_app(FakeStore::with_definition("job_hello_python").with_workflow("wf_pipeline"))
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/automations")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "id": "automation_cron",
                            "name": "Cron automation",
                            "workflow_id": "wf_pipeline",
                            "triggers": [{
                                "name": "nightly",
                                "kind": "schedule",
                                "config": { "cron": "0 0 * * * *", "timezone": "UTC" }
                            }],
                            "condition": { "trigger": "nightly" }
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    let body = response_json(response).await;
    assert!(body.get("trigger_kind").is_none());
    assert!(body.get("interval_seconds").is_none());
    assert_eq!(body["triggers"][0]["config"]["timezone"], "UTC");
}

#[tokio::test]
async fn creates_sql_automation_without_legacy_interval_seconds() {
    let response =
        test_app(FakeStore::with_definition("job_hello_python").with_workflow("wf_pipeline"))
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/automations")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "id": "automation_sql",
                            "name": "SQL automation",
                            "workflow_id": "wf_pipeline",
                            "triggers": [{
                                "name": "ready",
                                "kind": "sql",
                                "config": { "connection_name": "control", "query": "SELECT true" }
                            }],
                            "condition": { "trigger": "ready" }
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    assert_eq!(response_json(response).await["triggers"][0]["kind"], "sql");
}

#[tokio::test]
async fn disabled_automation_can_still_be_triggered_manually() {
    let app = test_app(FakeStore::with_definition("job_hello_python").with_workflow("wf_pipeline"));
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/automations")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "automation_toggle_test",
                        "name": "Toggle test",
                        "workflow_id": "wf_pipeline",
                        "triggers": [{ "name": "manual_ready", "kind": "manual", "config": {} }],
                        "condition": { "trigger": "manual_ready" }
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(create_response.status(), axum::http::StatusCode::CREATED);

    let disable_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/automations/automation_toggle_test/disable")
                .body(Body::from("{}"))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(disable_response.status(), axum::http::StatusCode::OK);
    assert_eq!(response_json(disable_response).await["status"], "disabled");

    let disabled_trigger_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/automations/automation_toggle_test/trigger")
                .body(Body::from("{}"))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        disabled_trigger_response.status(),
        axum::http::StatusCode::CREATED
    );

    let enable_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/automations/automation_toggle_test/enable")
                .body(Body::from("{}"))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(enable_response.status(), axum::http::StatusCode::OK);
    assert_eq!(response_json(enable_response).await["status"], "enabled");

    let trigger_response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/automations/automation_toggle_test/trigger")
                .body(Body::from("{}"))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(trigger_response.status(), axum::http::StatusCode::CREATED);
}

#[tokio::test]
async fn rejects_manual_automation_trigger_when_workflow_queue_is_overloaded() {
    let store = FakeStore::with_definition("job_hello_python")
        .with_workflow("wf_pipeline")
        .with_workflow_run(queued_workflow_run("workflow_run_existing", "wf_pipeline"));
    let app = test_app_with_admission(
        store.clone(),
        AdmissionConfig {
            max_queued_runs: None,
            max_queued_runs_per_pool: None,
            max_queued_workflow_runs: Some(1),
        },
    );
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/automations")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "automation_overload_test",
                        "name": "Overload test",
                        "workflow_id": "wf_pipeline",
                        "triggers": [{ "name": "manual_ready", "kind": "manual", "config": {} }],
                        "condition": { "trigger": "manual_ready" }
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(create_response.status(), axum::http::StatusCode::CREATED);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/automations/automation_overload_test/trigger")
                .body(Body::from("{}"))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response_json(response).await["code"], "queue_overloaded");
    assert_eq!(
        store
            .workflow_runs
            .lock()
            .expect("workflow runs mutex")
            .len(),
        1
    );
}

#[tokio::test]
async fn updates_existing_automation() {
    let app = test_app(FakeStore::with_definition("job_hello_python").with_workflow("wf_pipeline"));
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/automations")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "automation_update_test",
                        "name": "Before update",
                        "workflow_id": "wf_pipeline",
                        "triggers": [{ "name": "manual_ready", "kind": "manual", "config": {} }],
                        "condition": { "trigger": "manual_ready" }
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(create_response.status(), axum::http::StatusCode::CREATED);

    let update_response = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/v1/automations/automation_update_test")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "After update",
                        "workflow_id": "wf_pipeline",
                        "status": "disabled",
                        "job_input": { "email": "mohripan16@gmail.com" },
                        "triggers": [{
                            "name": "schedule_ready",
                            "kind": "schedule",
                            "config": {
                                "start_at": "2026-06-13T09:00",
                                "interval_seconds": 600,
                                "window_seconds": 60
                            }
                        }],
                        "condition": { "trigger": "schedule_ready" }
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(update_response.status(), axum::http::StatusCode::OK);
    let body = response_json(update_response).await;
    assert_eq!(body["name"], "After update");
    assert_eq!(body["status"], "disabled");
    assert!(body.get("trigger_kind").is_none());
    assert!(body.get("interval_seconds").is_none());
    assert_eq!(body["job_input"]["email"], "mohripan16@gmail.com");
    assert_eq!(body["triggers"][0]["name"], "schedule_ready");
}

#[tokio::test]
async fn deletes_existing_automation() {
    let app = test_app(FakeStore::with_definition("job_hello_python").with_workflow("wf_pipeline"));
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/automations")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "automation_delete_test",
                        "name": "Delete test",
                        "workflow_id": "wf_pipeline",
                        "triggers": [{ "name": "manual_ready", "kind": "manual", "config": {} }],
                        "condition": { "trigger": "manual_ready" }
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(create_response.status(), axum::http::StatusCode::CREATED);

    let delete_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/v1/automations/automation_delete_test")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(delete_response.status(), axum::http::StatusCode::NO_CONTENT);

    let get_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/automations/automation_delete_test")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(get_response.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn creates_and_reads_reusable_python_job_definition() {
    let app = test_app(FakeStore::default());
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/job-definitions")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "job_daily_report",
                        "name": "Daily report",
                        "runtime_image": "python:3.12-slim",
                        "python_script": "print('daily report')",
                        "retry_max_attempts": 2,
                        "retry_delay_seconds": 5
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(create_response.status(), axum::http::StatusCode::CREATED);
    let body = response_json(create_response).await;
    assert_eq!(body["id"], "job_daily_report");
    assert_eq!(body["name"], "Daily report");
    assert_eq!(
        body["bundle_object_key"],
        "bundles/job-definitions/job_daily_report/main.py"
    );

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/job-definitions")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(list_response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(list_response).await["job_definitions"][0]["id"],
        "job_daily_report"
    );

    let fetch_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/job-definitions/job_daily_report")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(fetch_response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(fetch_response).await["id"],
        "job_daily_report"
    );

    let source_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/job-definitions/job_daily_report/source")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let source_status = source_response.status();
    let source_body = response_json(source_response).await;
    assert_eq!(source_status, axum::http::StatusCode::OK, "{source_body}");
    assert_eq!(source_body["python_script"], "print('daily report')");
}

#[tokio::test]
async fn reads_inline_python_command_when_source_object_is_missing() {
    let response = test_app(FakeStore::with_definition("job_inline_python"))
        .oneshot(
            Request::builder()
                .uri("/v1/job-definitions/job_inline_python/source")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    let status = response.status();
    let body = response_json(response).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{body}");
    assert_eq!(body["python_script"], "print('fake')");
}

fn store_with_queued_workflow_run() -> FakeStore {
    FakeStore::with_definition("job_hello_python")
        .with_workflow("wf_locked")
        .with_workflow_run(WorkflowRun::new(
            WorkflowRunId::new("workflow_run_locked").expect("workflow run id"),
            WorkflowId::new("wf_locked").expect("workflow id"),
            None,
            "{}",
            WorkflowRunStatus::Queued,
            0,
            "2026-06-21 12:00:00+00",
        ))
}

#[tokio::test]
async fn editability_reports_workflow_locked_by_queued_run() {
    let response = test_app(store_with_queued_workflow_run())
        .oneshot(
            Request::builder()
                .uri("/v1/workflows/wf_locked/editability")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response_json(response).await["editable"], false);
}

#[tokio::test]
async fn update_workflow_rejects_definition_used_by_queued_run() {
    let response = test_app(store_with_queued_workflow_run())
        .oneshot(Request::builder().method(Method::PUT).uri("/v1/workflows/wf_locked").header("content-type", "application/json").body(Body::from(json!({"name":"Changed","steps":[{"name":"Run job","job_definition_id":"job_hello_python","execution_pool":"mini"}]}).to_string())).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::CONFLICT);
}

#[tokio::test]
async fn update_job_definition_rejects_definition_used_by_queued_workflow_run() {
    let response = test_app(store_with_queued_workflow_run())
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/v1/job-definitions/job_hello_python")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"name":"Changed","python_script":"print('changed')"}).to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::CONFLICT);
}

#[tokio::test]
async fn delete_job_definition_rejects_definition_used_by_workflow() {
    let response =
        test_app(FakeStore::with_definition("job_hello_python").with_workflow("wf_uses_job"))
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/v1/job-definitions/job_hello_python")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::CONFLICT);
    assert_eq!(
        response_json(response).await["code"],
        "job_definition_in_use"
    );
}

#[tokio::test]
async fn topology_returns_workflow_and_pool_route() {
    let response =
        test_app(FakeStore::with_definition("job_hello_python").with_workflow("wf_topology"))
            .oneshot(
                Request::builder()
                    .uri("/v1/topology")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
    let body = response_json(response).await;
    assert!(
        body["edges"]
            .as_array()
            .expect("topology edges")
            .iter()
            .any(|edge| edge["from"] == "workflow:wf_topology" && edge["to"] == "pool:mini")
    );
}

#[tokio::test]
async fn creates_manual_run() {
    let response = test_app(FakeStore::with_definition("job_hello_python"))
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/jobs/runs")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "run_id": "run_api_test",
                        "job_definition_id": "job_hello_python",
                        "host_group": "mini"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    assert_eq!(
        response_json(response).await,
        json!({
            "id": "run_api_test",
            "job_definition_id": "job_hello_python",
            "status": "queued",
            "execution_pool": "mini",
            "host_group": "mini",
            "attempt_count": 0,
            "created_at": "",
            "input": {}
        })
    );
}

#[tokio::test]
async fn rejects_manual_run_when_pool_queue_is_overloaded() {
    let store = FakeStore::with_definition("job_hello_python")
        .with_run(queued_run("run_existing_queued", "mini"));
    let app = test_app_with_admission(
        store.clone(),
        AdmissionConfig {
            max_queued_runs: None,
            max_queued_runs_per_pool: Some(1),
            max_queued_workflow_runs: None,
        },
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/jobs/runs")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "run_id": "run_rejected",
                        "job_definition_id": "job_hello_python",
                        "execution_pool": "mini"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response_json(response).await["code"], "queue_overloaded");
    assert!(
        store
            .runs
            .lock()
            .expect("runs mutex")
            .iter()
            .all(|run| { run.id() != &JobRunId::new("run_rejected").expect("run id") })
    );
}

#[tokio::test]
async fn creates_script_backed_run() {
    let response = test_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/jobs/runs")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "run_id": "run_script_test",
                        "job_definition_id": "script",
                        "execution_pool": "mini",
                        "python_script": "print('from script')"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    let body = response_json(response).await;
    assert_eq!(body["id"], "run_script_test");
    assert_eq!(body["job_definition_id"], "job_definition_run_script_test");
}

#[tokio::test]
async fn rejects_unknown_job_definition() {
    let response = test_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/jobs/runs")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "job_definition_id": "missing",
                        "execution_pool": "mini"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        response_json(response).await["code"],
        json!("unknown_job_definition")
    );
}

#[tokio::test]
async fn rejects_unknown_execution_pool() {
    let response = test_app(FakeStore::with_definition("job_hello_python"))
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/jobs/runs")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "job_definition_id": "job_hello_python",
                        "execution_pool": "gpu"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        response_json(response).await["code"],
        json!("unknown_execution_pool")
    );
}

#[tokio::test]
async fn lists_and_fetches_runs() {
    let run = JobRun::new(
        JobRunId::new("run_listed").expect("valid run id"),
        JobDefinitionId::new("job_hello_python").expect("valid definition id"),
        ExecutionPoolName::new("mini").expect("valid pool"),
    );
    let app = test_app(FakeStore::with_definition("job_hello_python").with_run(run));

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/jobs/runs")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(list_response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(list_response).await["runs"][0]["id"],
        "run_listed"
    );

    let get_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/jobs/runs/run_listed")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(get_response.status(), axum::http::StatusCode::OK);
    assert_eq!(response_json(get_response).await["id"], "run_listed");
}

#[tokio::test]
async fn cancels_run() {
    let run = JobRun::new(
        JobRunId::new("run_cancelled").expect("valid run id"),
        JobDefinitionId::new("job_hello_python").expect("valid definition id"),
        ExecutionPoolName::new("mini").expect("valid pool"),
    );
    let response = test_app(FakeStore::with_definition("job_hello_python").with_run(run))
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/jobs/runs/run_cancelled/cancel")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(response_json(response).await["status"], "cancelled");
}

#[tokio::test]
async fn returns_not_found_for_missing_run() {
    let response = test_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .uri("/v1/jobs/runs/run_missing")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    assert_eq!(
        response_json(response).await["code"],
        json!("job_run_not_found")
    );
}

#[tokio::test]
async fn fetches_run_logs() {
    let run = JobRun::new(
        JobRunId::new("run_with_logs").expect("valid run id"),
        JobDefinitionId::new("job_hello_python").expect("valid definition id"),
        ExecutionPoolName::new("mini").expect("valid pool"),
    );
    let log = JobRunLog::new(run.id().clone(), "hello from logs\n").expect("valid log");
    let response = test_app(
        FakeStore::with_definition("job_hello_python")
            .with_run(run)
            .with_log(log),
    )
    .oneshot(
        Request::builder()
            .uri("/v1/jobs/runs/run_with_logs/logs")
            .body(Body::empty())
            .expect("request"),
    )
    .await
    .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "run_id": "run_with_logs",
            "logs": "hello from logs\n",
            "object_log_available": false
        })
    );
}

#[tokio::test]
async fn fetches_workflow_run_logs_for_step_runs() {
    let workflow_id = WorkflowId::new("wf_logs").expect("workflow id");
    let workflow_run_id = WorkflowRunId::new("workflow_run_logs").expect("workflow run id");
    let first_job_run_id = JobRunId::new("run_step_one").expect("first job run id");
    let second_job_run_id = JobRunId::new("run_step_two").expect("second job run id");
    let workflow_run = WorkflowRun::new(
        workflow_run_id.clone(),
        workflow_id.clone(),
        None,
        "{}",
        WorkflowRunStatus::Running,
        2,
        "2026-06-13 12:00:00+00",
    );
    let first_step_run = WorkflowStepRun::new(
        WorkflowStepRunId::new("workflow_step_run_one").expect("first step run id"),
        workflow_run_id.clone(),
        WorkflowStepId::new("wf_logs_step_1").expect("first step id"),
        Some(first_job_run_id.clone()),
        1,
        WorkflowRunStatus::Succeeded,
    );
    let second_step_run = WorkflowStepRun::new(
        WorkflowStepRunId::new("workflow_step_run_two").expect("second step run id"),
        workflow_run_id.clone(),
        WorkflowStepId::new("wf_logs_step_2").expect("second step id"),
        Some(second_job_run_id.clone()),
        2,
        WorkflowRunStatus::Running,
    );
    let first_log =
        JobRunLog::new(first_job_run_id.clone(), "step one complete\n").expect("first log");
    let log_artifact = JobArtifact::new(
        ArtifactId::new("artifact_stdout_log").expect("artifact id"),
        first_job_run_id,
        None,
        "stdout.log",
        "artifacts/run_step_one/stdout.log",
        "text/plain",
        4096,
        None,
        ArtifactObjectKind::Log,
    )
    .expect("log artifact");

    let response = test_app(
        FakeStore::default()
            .with_workflow_run(workflow_run)
            .with_workflow_step_run(first_step_run)
            .with_workflow_step_run(second_step_run)
            .with_log(first_log)
            .with_artifact(log_artifact),
    )
    .oneshot(
        Request::builder()
            .uri("/v1/workflow-runs/workflow_run_logs/logs")
            .body(Body::empty())
            .expect("request"),
    )
    .await
    .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "workflow_run_id": "workflow_run_logs",
            "workflow_id": "wf_logs",
            "status": "running",
            "entries": [
                {
                    "step_run_id": "workflow_step_run_one",
                    "workflow_step_id": "wf_logs_step_1",
                    "job_run_id": "run_step_one",
                    "position": 1,
                    "status": "succeeded",
                    "logs": "step one complete\n",
                    "object_log_available": true
                },
                {
                    "step_run_id": "workflow_step_run_two",
                    "workflow_step_id": "wf_logs_step_2",
                    "job_run_id": "run_step_two",
                    "position": 2,
                    "status": "running",
                    "logs": "",
                    "object_log_available": false
                }
            ]
        })
    );
}

#[tokio::test]
async fn removes_queued_workflow_run_before_steps_start() {
    let workflow_run = WorkflowRun::new(
        WorkflowRunId::new("workflow_run_remove").expect("workflow run id"),
        WorkflowId::new("wf_remove").expect("workflow id"),
        None,
        "{}",
        WorkflowRunStatus::Queued,
        0,
        "2026-06-13 12:00:00+00",
    );

    let response = test_app(FakeStore::default().with_workflow_run(workflow_run))
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/workflow-runs/workflow_run_remove/remove")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(response_json(response).await["status"], "removed");
}

#[tokio::test]
async fn rejects_removing_workflow_run_after_step_started() {
    let workflow_run_id = WorkflowRunId::new("workflow_run_started").expect("workflow run id");
    let workflow_run = WorkflowRun::new(
        workflow_run_id.clone(),
        WorkflowId::new("wf_started").expect("workflow id"),
        None,
        "{}",
        WorkflowRunStatus::Queued,
        0,
        "2026-06-13 12:00:00+00",
    );
    let step_run = WorkflowStepRun::new(
        WorkflowStepRunId::new("workflow_step_started").expect("step run id"),
        workflow_run_id,
        WorkflowStepId::new("wf_started_step").expect("step id"),
        Some(JobRunId::new("run_started_step").expect("job run id")),
        1,
        WorkflowRunStatus::Running,
    );

    let response = test_app(
        FakeStore::default()
            .with_workflow_run(workflow_run)
            .with_workflow_step_run(step_run),
    )
    .oneshot(
        Request::builder()
            .method(Method::POST)
            .uri("/v1/workflow-runs/workflow_run_started/remove")
            .body(Body::empty())
            .expect("request"),
    )
    .await
    .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(response).await["code"],
        json!("invalid_workflow_run_transition")
    );
}

#[tokio::test]
async fn cancels_running_workflow_run_and_active_step() {
    let workflow_run_id = WorkflowRunId::new("workflow_run_cancel").expect("workflow run id");
    let job_run = JobRun::new(
        JobRunId::new("run_cancel_step").expect("job run id"),
        JobDefinitionId::new("job_hello_python").expect("definition id"),
        ExecutionPoolName::new("mini").expect("pool"),
    );
    let workflow_run = WorkflowRun::new(
        workflow_run_id.clone(),
        WorkflowId::new("wf_cancel").expect("workflow id"),
        None,
        "{}",
        WorkflowRunStatus::Running,
        1,
        "2026-06-13 12:00:00+00",
    );
    let step_run = WorkflowStepRun::new(
        WorkflowStepRunId::new("workflow_step_cancel").expect("step run id"),
        workflow_run_id,
        WorkflowStepId::new("wf_cancel_step").expect("step id"),
        Some(job_run.id().clone()),
        1,
        WorkflowRunStatus::Running,
    );

    let response = test_app(
        FakeStore::with_definition("job_hello_python")
            .with_run(job_run)
            .with_workflow_run(workflow_run)
            .with_workflow_step_run(step_run),
    )
    .oneshot(
        Request::builder()
            .method(Method::POST)
            .uri("/v1/workflow-runs/workflow_run_cancel/cancel")
            .body(Body::empty())
            .expect("request"),
    )
    .await
    .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(response_json(response).await["status"], "cancelled");
}

#[tokio::test]
async fn rejects_cancelling_queued_workflow_run() {
    let workflow_run = WorkflowRun::new(
        WorkflowRunId::new("workflow_run_cancel_queued").expect("workflow run id"),
        WorkflowId::new("wf_cancel_queued").expect("workflow id"),
        None,
        "{}",
        WorkflowRunStatus::Queued,
        0,
        "2026-06-13 12:00:00+00",
    );

    let response = test_app(FakeStore::default().with_workflow_run(workflow_run))
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/workflow-runs/workflow_run_cancel_queued/cancel")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(response).await["code"],
        json!("invalid_workflow_run_transition")
    );
}

#[tokio::test]
async fn returns_not_found_for_missing_run_logs() {
    let run = JobRun::new(
        JobRunId::new("run_without_logs").expect("valid run id"),
        JobDefinitionId::new("job_hello_python").expect("valid definition id"),
        ExecutionPoolName::new("mini").expect("valid pool"),
    );
    let response = test_app(FakeStore::with_definition("job_hello_python").with_run(run))
        .oneshot(
            Request::builder()
                .uri("/v1/jobs/runs/run_without_logs/logs")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    assert_eq!(
        response_json(response).await["code"],
        json!("job_run_logs_not_found")
    );
}

#[tokio::test]
async fn lists_artifacts() {
    let run = JobRun::new(
        JobRunId::new("run_with_artifact").expect("valid run id"),
        JobDefinitionId::new("job_hello_python").expect("valid definition id"),
        ExecutionPoolName::new("mini").expect("valid pool"),
    );
    let artifact = JobArtifact::new(
        ArtifactId::new("artifact_1").expect("artifact id"),
        run.id().clone(),
        None,
        "report.txt",
        "artifacts/run_with_artifact/report.txt",
        "text/plain",
        6,
        None,
        ArtifactObjectKind::Artifact,
    )
    .expect("artifact");
    let response = test_app(
        FakeStore::with_definition("job_hello_python")
            .with_run(run)
            .with_artifact(artifact),
    )
    .oneshot(
        Request::builder()
            .uri("/v1/jobs/runs/run_with_artifact/artifacts")
            .body(Body::empty())
            .expect("request"),
    )
    .await
    .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(response).await["artifacts"][0]["name"],
        "report.txt"
    );
}

#[tokio::test]
async fn downloads_artifact() {
    let run = JobRun::new(
        JobRunId::new("run_with_artifact").expect("valid run id"),
        JobDefinitionId::new("job_hello_python").expect("valid definition id"),
        ExecutionPoolName::new("mini").expect("valid pool"),
    );
    let artifact = JobArtifact::new(
        ArtifactId::new("artifact_1").expect("artifact id"),
        run.id().clone(),
        None,
        "report.txt",
        "artifacts/run_with_artifact/report.txt",
        "text/plain",
        6,
        None,
        ArtifactObjectKind::Artifact,
    )
    .expect("artifact");
    let response = test_app(
        FakeStore::with_definition("job_hello_python")
            .with_run(run)
            .with_artifact(artifact),
    )
    .oneshot(
        Request::builder()
            .uri("/v1/jobs/runs/run_with_artifact/artifacts/artifact_1")
            .body(Body::empty())
            .expect("request"),
    )
    .await
    .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("collect artifact")
        .to_bytes();
    assert_eq!(&bytes[..], b"report");
}

#[tokio::test]
async fn response_body_helper_handles_empty_body() {
    let bytes = to_bytes(Body::empty(), usize::MAX)
        .await
        .expect("empty body");
    assert!(bytes.is_empty());
}
