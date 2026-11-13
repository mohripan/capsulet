//! The HTTP API's tests, run against an in-memory store.
//!
//! `fake` is that store, one module per storage domain. Each other module tests
//! one area of the API through the router, as a client would. What several
//! areas share, the fake and the apps and requests they are built from, is here.

use std::sync::{Arc, Mutex};

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request},
};
use capsulet_application::AgentRunRecord;
use capsulet_core::{
    AgentDefinition, AgentId, AgentRunId, ArtifactId, ArtifactObjectKind, Automation, AutomationId,
    AutomationStatus, AutomationTrigger, CanonicalEntity, Claim, ClaimConflict, ClaimConflictId,
    ClaimId, CustomTriggerPlugin, Entity, EntityGraphAttachment, EntityId, EntityResolution,
    EntityResolutionId, Event, EventId, Evidence, EvidenceId, ExecutionPoolName, GraphDefinition,
    GraphId, IngestionConnector, IngestionConnectorId, IngestionRun, IngestionRunId,
    IngestionRunOutputRecord, JobArtifact, JobDefinition, JobDefinitionId, JobRun, JobRunId,
    JobRunLog, JobRunStatus, JobRunTransition, MemoryContract, MemoryContractId, MemorySubgraph,
    MemorySubgraphMember, Relationship, RelationshipId, Source, SourceId, SubgraphEdge,
    SummaryTrace, WorkflowDefinition, WorkflowId, WorkflowRun, WorkflowRunId, WorkflowRunStatus,
    WorkflowStatus, WorkflowStep, WorkflowStepId, WorkflowStepRun, WorkflowStepRunId,
};
use capsulet_postgres::{
    AdmissionSnapshot, CertificateRecord, DomainPackRecord, IrDefinitionVersion, IrRunRecord,
    NewProjectMembership, ProjectMembershipRecord, ProjectRecord, ReadingReviewRecord,
    ServiceAccountRecord, TriggerEvent,
};
use capsulet_runtime::{RecordedEvent, RunEvent, event::Epoch};
use capsulet_storage::ObjectStore;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use super::{
    AgentStore, AppState, AuthConfig, AutomationStore, IamStore, IngestionStore, IrStore, JobStore,
    MemoryGraphStore, MemoryStore, PackStore, ReasoningStore, StoreBase, WorkflowStore, router,
};
use crate::state::AdmissionConfig;

mod agents;
mod auth;
mod authorization_http;
mod automations;
mod fake;
mod ingestion;
mod ir;
mod jobs;
mod memory;
mod packs;
mod platform;
mod published_contracts;
mod reasoning;
mod workflows;

#[derive(Debug, Clone, Default)]
struct FakeStore {
    omit_ownership: bool,
    audit_events: Arc<Mutex<Vec<FakeAuditEvent>>>,
    service_accounts: Arc<Mutex<Vec<FakeServiceAccount>>>,
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
    reasoning_certificates: Arc<Mutex<Vec<CertificateRecord>>>,
    reading_reviews: Arc<Mutex<Vec<ReadingReviewRecord>>>,
    domain_packs: Arc<Mutex<Vec<DomainPackRecord>>>,
    memory_evidence: Arc<Mutex<Vec<Evidence>>>,
    memory_entities: Arc<Mutex<Vec<Entity>>>,
    memory_claims: Arc<Mutex<Vec<Claim>>>,
    memory_claim_conflicts: Arc<Mutex<Vec<ClaimConflict>>>,
    memory_events: Arc<Mutex<Vec<Event>>>,
    memory_relationships: Arc<Mutex<Vec<Relationship>>>,
    memory_contracts: Arc<Mutex<Vec<MemoryContract>>>,
    memory_subgraphs: Arc<Mutex<Vec<MemorySubgraph>>>,
    memory_subgraph_members: Arc<Mutex<Vec<MemorySubgraphMember>>>,
    memory_canonical_entities: Arc<Mutex<Vec<CanonicalEntity>>>,
    memory_entity_resolutions: Arc<Mutex<Vec<EntityResolution>>>,
    memory_subgraph_edges: Arc<Mutex<Vec<SubgraphEdge>>>,
    memory_summary_traces: Arc<Mutex<Vec<SummaryTrace>>>,
    memory_entity_graph_attachments: Arc<Mutex<Vec<EntityGraphAttachment>>>,
    ingestion_connectors: Arc<Mutex<Vec<IngestionConnector>>>,
    ingestion_runs: Arc<Mutex<Vec<IngestionRun>>>,
    ingestion_run_outputs: Arc<Mutex<Vec<IngestionRunOutputRecord>>>,
    ir_definitions: Arc<Mutex<Vec<IrDefinitionVersion>>>,
    ir_runs: Arc<Mutex<Vec<IrRunRecord>>>,
    ir_run_events: Arc<Mutex<Vec<(String, RecordedEvent)>>>,
    projects: Arc<Mutex<Vec<ProjectRecord>>>,
    project_memberships: Arc<Mutex<Vec<ProjectMembershipRecord>>>,
    ownership: Arc<Mutex<Vec<FakeOwnership>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FakeOwnership {
    resource: String,
    id: String,
    tenant_id: String,
    project_id: String,
}

#[derive(Debug, Clone)]
struct FakeServiceAccount {
    token_hash: [u8; 32],
    record: ServiceAccountRecord,
}

#[derive(Debug, Clone)]
struct FakeAuditEvent {
    event: capsulet_postgres::AuditEvent,
    scope: Option<(String, String)>,
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

impl FakeStore {
    /// Whether a project owns a resource, reading an unrecorded owner as the
    /// default project the way the fake's `resource_project` does.
    fn owns(&self, resource: &str, id: &str, tenant_id: &str, project_id: &str) -> bool {
        self.ownership
            .lock()
            .expect("ownership lock")
            .iter()
            .find(|owner| owner.resource == resource && owner.id == id)
            .map_or(tenant_id == "default" && project_id == "default", |owner| {
                owner.tenant_id == tenant_id && owner.project_id == project_id
            })
    }

    fn store_automation_triggers(
        &self,
        automation_id: &AutomationId,
        triggers: &[AutomationTrigger],
        condition_json: &str,
    ) -> Result<(), String> {
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

    /// Records ownership on first write and refuses a write from another project,
    /// standing in for the scoped upsert's `WHERE` clause.
    fn claim_resource(
        &self,
        resource: &str,
        id: &str,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), String> {
        let mut ownership = self.ownership.lock().map_err(|error| error.to_string())?;
        if let Some(owner) = ownership
            .iter()
            .find(|owner| owner.resource == resource && owner.id == id)
        {
            if owner.tenant_id != tenant_id || owner.project_id != project_id {
                return Err("scope mismatch".to_string());
            }
        } else {
            ownership.push(FakeOwnership {
                resource: resource.to_string(),
                id: id.to_string(),
                tenant_id: tenant_id.to_string(),
                project_id: project_id.to_string(),
            });
        }
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

    /// Whether a project owns a resource, by the same records `resource_project`
    /// answers from: what has no record belongs to the default project.
    fn owned_by(
        &self,
        resource: &str,
        id: &str,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<bool, String> {
        if self.omit_ownership {
            return Ok(false);
        }
        let ownership = self.ownership.lock().map_err(|error| error.to_string())?;
        let owner = ownership
            .iter()
            .find(|ownership| ownership.resource == resource && ownership.id == id)
            .map_or(("default", "default"), |ownership| {
                (ownership.tenant_id.as_str(), ownership.project_id.as_str())
            });
        Ok(owner == (tenant_id, project_id))
    }

    /// Refuses a memory write that names a subgraph outside its own scope, the way the
    /// `PostgreSQL` write does: a subgraph in another project is the same as no subgraph.
    fn require_subgraph_in_scope(
        &self,
        id: &capsulet_core::MemorySubgraphId,
        scope: &capsulet_core::MemoryScope,
    ) -> Result<(), String> {
        let in_scope = self
            .memory_subgraphs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .any(|subgraph| subgraph.id() == id && subgraph.scope() == scope);
        if in_scope {
            Ok(())
        } else {
            Err("scope mismatch".to_string())
        }
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

/// A definition with one pure node.
///
/// Built from the IR's own types rather than hand-written JSON, so the fixture
/// cannot drift from the shape the crate actually reads.
fn ir_definition_body(assurance: capsulet_ir::definition::AssuranceMode) -> Value {
    let identifier = |value: &str| capsulet_ir::Identifier::parse(value).expect("identifier");
    let graph = capsulet_ir::Graph::new(capsulet_ir::GraphBuilder {
        nodes: vec![capsulet_ir::Node {
            id: identifier("prepare"),
            name: "Prepare".to_string(),
            kind: capsulet_ir::NodeKind::PureComputation,
            inputs: vec![],
            outputs: vec![capsulet_ir::OutputPort::new(
                identifier("patch"),
                capsulet_ir::ValueSchema::Text {
                    length: capsulet_ir::value::LengthBounds::new(0, 1_024),
                },
            )],
            capabilities: vec![],
            effects: vec![],
            budget: capsulet_ir::ResourceBudget::deterministic(1_000),
            provider: None,
            sub_workflow: None,
            parameters: std::collections::BTreeMap::new(),
        }],
        ..capsulet_ir::GraphBuilder::default()
    })
    .expect("the fixture graph is valid");

    let definition = capsulet_ir::Definition {
        schema_version: capsulet_ir::Definition::current_schema_version(),
        packs: Vec::new(),
        id: identifier("prepare-release"),
        version: "1".to_string(),
        name: "Prepare release".to_string(),
        assurance,
        capabilities: capsulet_ir::CapabilitySet::empty(),
        budget: capsulet_ir::ResourceBudget::deterministic(600_000),
        graph,
        boundaries: vec![],
        contracts: vec![],
    };
    serde_json::to_value(definition).expect("the fixture definition serializes")
}

async fn send(app: &axum::Router, request: Request<Body>) -> (axum::http::StatusCode, Value) {
    let response = app.clone().oneshot(request).await.expect("response");
    let status = response.status();
    (status, response_json(response).await)
}

/// An authorised request, since the run endpoints all take the same headers.
fn admin_request(method: Method, uri: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(
            "authorization",
            "Bearer admin-token-0123456789-abcdefghijkl",
        )
        .header("content-type", "application/json");
    builder
        .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
        .expect("request")
}

/// Sends `request` and returns its JSON body, failing unless it succeeded.
///
/// The router's conformance check has already held the body to the operation's
/// published schema by the time this sees it; this only insists the call worked.
async fn succeeds(app: &axum::Router, request: Request<Body>) -> Value {
    let label = format!("{} {}", request.method(), request.uri());
    let response = app.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("collect response")
        .to_bytes();
    assert!(
        status.is_success(),
        "{label} answered {status}: {}",
        String::from_utf8_lossy(&bytes)
    );
    if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json response")
    }
}
