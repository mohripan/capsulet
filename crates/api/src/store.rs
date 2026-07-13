use std::fmt::Display;

use async_trait::async_trait;
use capsulet_application::{
    AgentRunRecord, JobArtifactRepository, JobRunLogRepository, JobRunRepository,
};
use capsulet_core::{
    AgentDefinition, AgentId, AgentRunId, ArtifactId, Automation, AutomationId, AutomationTrigger,
    CanonicalEntity, Claim, ClaimConflict, ClaimConflictId, ClaimId, CustomTriggerPlugin, Entity,
    EntityGraphAttachment, EntityId, EntityResolution, EntityResolutionId, Event, EventId,
    Evidence, EvidenceId, GraphDefinition, GraphId, IngestionConnector, IngestionConnectorId,
    IngestionRun, IngestionRunId, IngestionRunOutputRecord, JobArtifact, JobDefinition,
    JobDefinitionId, JobRun, JobRunId, JobRunLog, MemoryContract, MemoryContractId, MemorySubgraph,
    MemorySubgraphId, MemorySubgraphMember, Relationship, RelationshipId, Source, SourceId,
    SubgraphEdge, SummaryTrace, WorkflowDefinition, WorkflowId, WorkflowRun, WorkflowRunId,
    WorkflowStepRun,
};
use capsulet_postgres::{
    AdmissionSnapshot, AuditEvent, NewProjectMembership, NewServiceAccount, PostgresStore,
    PostgresStoreError, ProjectMembershipRecord, ProjectRecord, ServiceAccountRecord, TriggerEvent,
};

/// Storage operations required by the HTTP API.
#[async_trait]
pub trait ApiStore: Clone + Send + Sync + 'static {
    type Error: Display + Send + Sync + 'static;

    async fn ping(&self) -> Result<(), Self::Error>;
    async fn prometheus_metrics(&self) -> Result<String, Self::Error> {
        Ok(String::new())
    }
    async fn admission_snapshot(
        &self,
        _execution_pool: &str,
    ) -> Result<AdmissionSnapshot, Self::Error> {
        Ok(AdmissionSnapshot::default())
    }
    async fn list_audit_events(&self, _limit: i64) -> Result<Vec<AuditEvent>, Self::Error> {
        Ok(Vec::new())
    }
    async fn list_projects(
        &self,
        _tenant_id: &str,
        _project_ids: &[String],
    ) -> Result<Vec<ProjectRecord>, Self::Error> {
        Ok(Vec::new())
    }
    async fn list_all_projects(&self, _tenant_id: &str) -> Result<Vec<ProjectRecord>, Self::Error> {
        Ok(Vec::new())
    }
    async fn list_project_memberships(
        &self,
        _tenant_id: &str,
        _project_id: &str,
    ) -> Result<Vec<ProjectMembershipRecord>, Self::Error> {
        Ok(Vec::new())
    }
    async fn list_principal_project_memberships(
        &self,
        _tenant_id: &str,
        _principal_name: &str,
    ) -> Result<Vec<ProjectMembershipRecord>, Self::Error> {
        Ok(Vec::new())
    }
    async fn upsert_project_membership(
        &self,
        _membership: &NewProjectMembership,
    ) -> Result<ProjectMembershipRecord, Self::Error> {
        unreachable!("project membership creation is unsupported by this store")
    }
    async fn delete_project_membership(
        &self,
        _tenant_id: &str,
        _project_id: &str,
        _principal_kind: &str,
        _principal_name: &str,
    ) -> Result<bool, Self::Error> {
        Ok(false)
    }
    async fn resource_project(
        &self,
        _resource: &str,
        _id: &str,
    ) -> Result<Option<(String, String)>, Self::Error> {
        Ok(Some(("default".to_string(), "default".to_string())))
    }
    async fn set_resource_project(
        &self,
        _resource: &str,
        _id: &str,
        _tenant_id: &str,
        _project_id: &str,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn authenticate_service_account_hash(
        &self,
        _token_hash: &[u8; 32],
    ) -> Result<Option<ServiceAccountRecord>, Self::Error> {
        Ok(None)
    }
    async fn create_service_account(
        &self,
        _account: &NewServiceAccount,
    ) -> Result<ServiceAccountRecord, Self::Error> {
        unreachable!("service account creation is unsupported by this store")
    }
    async fn list_service_accounts(
        &self,
        _limit: i64,
    ) -> Result<Vec<ServiceAccountRecord>, Self::Error> {
        Ok(Vec::new())
    }
    async fn revoke_service_account(&self, _id: &str) -> Result<bool, Self::Error> {
        Ok(false)
    }
    #[allow(clippy::too_many_arguments)]
    async fn record_audit_event(
        &self,
        _principal: &str,
        _role: &str,
        _method: &str,
        _path: &str,
        _status_code: u16,
        _request_id: Option<&str>,
        _user_agent: Option<&str>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn enqueue_trigger_event(
        &self,
        event: &TriggerEvent,
        idempotency_key: &str,
    ) -> Result<bool, Self::Error>;

    async fn job_definition_exists(&self, id: &JobDefinitionId) -> Result<bool, Self::Error>;
    async fn upsert_job_definition(&self, definition: &JobDefinition) -> Result<(), Self::Error>;
    async fn list_job_definitions(&self, limit: i64) -> Result<Vec<JobDefinition>, Self::Error>;
    async fn find_job_definition(
        &self,
        id: &JobDefinitionId,
    ) -> Result<Option<JobDefinition>, Self::Error>;
    async fn job_definition_has_active_workflow_runs(
        &self,
        id: &JobDefinitionId,
    ) -> Result<bool, Self::Error>;
    async fn job_definition_is_used_by_workflows(
        &self,
        id: &JobDefinitionId,
    ) -> Result<bool, Self::Error>;
    async fn delete_job_definition(&self, id: &JobDefinitionId) -> Result<bool, Self::Error>;
    async fn upsert_workflow(&self, workflow: &WorkflowDefinition) -> Result<(), Self::Error>;
    async fn list_workflows(&self, limit: i64) -> Result<Vec<WorkflowDefinition>, Self::Error>;
    async fn find_workflow(
        &self,
        id: &WorkflowId,
    ) -> Result<Option<WorkflowDefinition>, Self::Error>;
    async fn workflow_has_active_runs(&self, id: &WorkflowId) -> Result<bool, Self::Error>;
    async fn delete_workflow(&self, id: &WorkflowId) -> Result<bool, Self::Error>;
    async fn upsert_graph(&self, graph: &GraphDefinition) -> Result<(), Self::Error>;
    async fn list_graphs(&self, limit: i64) -> Result<Vec<GraphDefinition>, Self::Error>;
    async fn find_graph(&self, id: &GraphId) -> Result<Option<GraphDefinition>, Self::Error>;
    async fn upsert_agent(&self, agent: &AgentDefinition) -> Result<(), Self::Error>;
    async fn list_agents(&self, limit: i64) -> Result<Vec<AgentDefinition>, Self::Error>;
    async fn find_agent(&self, id: &AgentId) -> Result<Option<AgentDefinition>, Self::Error>;
    async fn upsert_agent_run(&self, run: &AgentRunRecord) -> Result<(), Self::Error>;
    async fn list_agent_runs(&self, limit: i64) -> Result<Vec<AgentRunRecord>, Self::Error>;
    async fn find_agent_run(&self, id: &AgentRunId) -> Result<Option<AgentRunRecord>, Self::Error>;
    async fn upsert_memory_source(&self, source: &Source) -> Result<(), Self::Error>;
    async fn list_memory_sources(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Source>, Self::Error>;
    async fn find_memory_source(&self, id: &SourceId) -> Result<Option<Source>, Self::Error>;
    async fn upsert_memory_evidence(&self, evidence: &Evidence) -> Result<(), Self::Error>;
    async fn list_memory_evidence(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Evidence>, Self::Error>;
    async fn find_memory_evidence(&self, id: &EvidenceId) -> Result<Option<Evidence>, Self::Error>;
    async fn upsert_memory_entity(&self, entity: &Entity) -> Result<(), Self::Error>;
    async fn list_memory_entities(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Entity>, Self::Error>;
    async fn find_memory_entity(&self, id: &EntityId) -> Result<Option<Entity>, Self::Error>;
    async fn upsert_memory_claim(&self, claim: &Claim) -> Result<(), Self::Error>;
    async fn list_memory_claims(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Claim>, Self::Error>;
    async fn find_memory_claim(&self, id: &ClaimId) -> Result<Option<Claim>, Self::Error>;
    async fn upsert_memory_claim_conflict(
        &self,
        conflict: &ClaimConflict,
    ) -> Result<(), Self::Error>;
    async fn list_memory_claim_conflicts(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<ClaimConflict>, Self::Error>;
    async fn find_memory_claim_conflict(
        &self,
        id: &ClaimConflictId,
    ) -> Result<Option<ClaimConflict>, Self::Error>;
    async fn upsert_memory_event(&self, event: &Event) -> Result<(), Self::Error>;
    async fn list_memory_events(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Event>, Self::Error>;
    async fn find_memory_event(&self, id: &EventId) -> Result<Option<Event>, Self::Error>;
    async fn upsert_memory_relationship(
        &self,
        relationship: &Relationship,
    ) -> Result<(), Self::Error>;
    async fn list_memory_relationships(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Relationship>, Self::Error>;
    async fn find_memory_relationship(
        &self,
        id: &RelationshipId,
    ) -> Result<Option<Relationship>, Self::Error>;
    async fn upsert_memory_contract(&self, contract: &MemoryContract) -> Result<(), Self::Error>;
    async fn list_memory_contracts(&self, limit: i64) -> Result<Vec<MemoryContract>, Self::Error>;
    async fn find_memory_contract(
        &self,
        id: &MemoryContractId,
    ) -> Result<Option<MemoryContract>, Self::Error>;
    async fn upsert_memory_subgraph(&self, subgraph: &MemorySubgraph) -> Result<(), Self::Error>;
    async fn list_memory_subgraphs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<MemorySubgraph>, Self::Error>;
    async fn find_memory_subgraph(
        &self,
        id: &MemorySubgraphId,
    ) -> Result<Option<MemorySubgraph>, Self::Error>;
    async fn upsert_memory_subgraph_member(
        &self,
        member: &MemorySubgraphMember,
    ) -> Result<(), Self::Error>;
    async fn upsert_memory_canonical_entity(
        &self,
        entity: &CanonicalEntity,
    ) -> Result<(), Self::Error>;
    async fn list_memory_canonical_entities(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<CanonicalEntity>, Self::Error>;
    async fn upsert_memory_entity_resolution(
        &self,
        resolution: &EntityResolution,
    ) -> Result<(), Self::Error>;
    async fn list_memory_entity_resolutions(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<EntityResolution>, Self::Error>;
    async fn find_memory_entity_resolution(
        &self,
        id: &EntityResolutionId,
    ) -> Result<Option<EntityResolution>, Self::Error>;
    async fn upsert_memory_subgraph_edge(&self, edge: &SubgraphEdge) -> Result<(), Self::Error>;
    async fn upsert_memory_summary_trace(&self, trace: &SummaryTrace) -> Result<(), Self::Error>;
    async fn list_memory_summary_traces(
        &self,
        subgraph_id: &MemorySubgraphId,
        summary_claim_id: &ClaimId,
    ) -> Result<Vec<SummaryTrace>, Self::Error>;
    async fn upsert_memory_entity_graph_attachment(
        &self,
        attachment: &EntityGraphAttachment,
    ) -> Result<(), Self::Error>;
    async fn upsert_ingestion_connector(
        &self,
        connector: &IngestionConnector,
    ) -> Result<(), Self::Error>;
    async fn list_ingestion_connectors(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<IngestionConnector>, Self::Error>;
    async fn find_ingestion_connector(
        &self,
        id: &IngestionConnectorId,
    ) -> Result<Option<IngestionConnector>, Self::Error>;
    async fn upsert_ingestion_run(&self, run: &IngestionRun) -> Result<(), Self::Error>;
    async fn list_ingestion_runs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<IngestionRun>, Self::Error>;
    async fn find_ingestion_run(
        &self,
        id: &IngestionRunId,
    ) -> Result<Option<IngestionRun>, Self::Error>;
    async fn upsert_ingestion_run_output(
        &self,
        output: &IngestionRunOutputRecord,
    ) -> Result<(), Self::Error>;
    async fn list_ingestion_run_outputs(
        &self,
        run_id: &IngestionRunId,
    ) -> Result<Vec<IngestionRunOutputRecord>, Self::Error>;
    async fn upsert_automation(&self, automation: &Automation) -> Result<(), Self::Error>;
    async fn list_automations(&self, limit: i64) -> Result<Vec<Automation>, Self::Error>;
    async fn find_automation(&self, id: &AutomationId) -> Result<Option<Automation>, Self::Error>;
    async fn set_automation_status(
        &self,
        id: &AutomationId,
        status: capsulet_core::AutomationStatus,
    ) -> Result<Option<Automation>, Self::Error>;
    async fn delete_automation(&self, id: &AutomationId) -> Result<bool, Self::Error>;
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
    async fn find_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
    ) -> Result<Option<WorkflowRun>, Self::Error>;
    async fn remove_queued_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
    ) -> Result<Option<WorkflowRun>, Self::Error>;
    async fn cancel_running_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
    ) -> Result<Option<WorkflowRun>, Self::Error>;
    async fn resume_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
    ) -> Result<Option<WorkflowRun>, Self::Error>;
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

    async fn ping(&self) -> Result<(), Self::Error> {
        self.ping().await
    }

    async fn prometheus_metrics(&self) -> Result<String, Self::Error> {
        PostgresStore::prometheus_metrics(self).await
    }

    async fn admission_snapshot(
        &self,
        execution_pool: &str,
    ) -> Result<AdmissionSnapshot, Self::Error> {
        PostgresStore::admission_snapshot(self, execution_pool).await
    }

    async fn list_audit_events(&self, limit: i64) -> Result<Vec<AuditEvent>, Self::Error> {
        PostgresStore::list_audit_events(self, limit).await
    }

    async fn list_projects(
        &self,
        tenant_id: &str,
        project_ids: &[String],
    ) -> Result<Vec<ProjectRecord>, Self::Error> {
        PostgresStore::list_projects(self, tenant_id, project_ids).await
    }

    async fn list_all_projects(&self, tenant_id: &str) -> Result<Vec<ProjectRecord>, Self::Error> {
        PostgresStore::list_all_projects(self, tenant_id).await
    }

    async fn list_project_memberships(
        &self,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<ProjectMembershipRecord>, Self::Error> {
        PostgresStore::list_project_memberships(self, tenant_id, project_id).await
    }

    async fn list_principal_project_memberships(
        &self,
        tenant_id: &str,
        principal_name: &str,
    ) -> Result<Vec<ProjectMembershipRecord>, Self::Error> {
        PostgresStore::list_principal_project_memberships(self, tenant_id, principal_name).await
    }

    async fn upsert_project_membership(
        &self,
        membership: &NewProjectMembership,
    ) -> Result<ProjectMembershipRecord, Self::Error> {
        PostgresStore::upsert_project_membership(self, membership).await
    }

    async fn delete_project_membership(
        &self,
        tenant_id: &str,
        project_id: &str,
        principal_kind: &str,
        principal_name: &str,
    ) -> Result<bool, Self::Error> {
        PostgresStore::delete_project_membership(
            self,
            tenant_id,
            project_id,
            principal_kind,
            principal_name,
        )
        .await
    }

    async fn resource_project(
        &self,
        resource: &str,
        id: &str,
    ) -> Result<Option<(String, String)>, Self::Error> {
        PostgresStore::resource_project(self, resource, id).await
    }

    async fn set_resource_project(
        &self,
        resource: &str,
        id: &str,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error> {
        PostgresStore::set_resource_project(self, resource, id, tenant_id, project_id).await
    }

    async fn authenticate_service_account_hash(
        &self,
        token_hash: &[u8; 32],
    ) -> Result<Option<ServiceAccountRecord>, Self::Error> {
        PostgresStore::authenticate_service_account_hash(self, token_hash).await
    }

    async fn create_service_account(
        &self,
        account: &NewServiceAccount,
    ) -> Result<ServiceAccountRecord, Self::Error> {
        PostgresStore::create_service_account(self, account).await
    }

    async fn list_service_accounts(
        &self,
        limit: i64,
    ) -> Result<Vec<ServiceAccountRecord>, Self::Error> {
        PostgresStore::list_service_accounts(self, limit).await
    }

    async fn revoke_service_account(&self, id: &str) -> Result<bool, Self::Error> {
        PostgresStore::revoke_service_account(self, id).await
    }

    async fn record_audit_event(
        &self,
        principal: &str,
        role: &str,
        method: &str,
        path: &str,
        status_code: u16,
        request_id: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<(), Self::Error> {
        PostgresStore::record_audit_event(
            self,
            principal,
            role,
            method,
            path,
            status_code,
            request_id,
            user_agent,
        )
        .await
    }

    async fn enqueue_trigger_event(
        &self,
        event: &TriggerEvent,
        idempotency_key: &str,
    ) -> Result<bool, Self::Error> {
        self.enqueue_trigger_event(event, idempotency_key).await
    }

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

    async fn workflow_has_active_runs(&self, id: &WorkflowId) -> Result<bool, Self::Error> {
        self.workflow_has_active_runs(id).await
    }

    async fn delete_workflow(&self, id: &WorkflowId) -> Result<bool, Self::Error> {
        self.delete_workflow(id).await
    }

    async fn upsert_graph(&self, graph: &GraphDefinition) -> Result<(), Self::Error> {
        self.upsert_graph(graph).await
    }

    async fn list_graphs(&self, limit: i64) -> Result<Vec<GraphDefinition>, Self::Error> {
        self.list_graphs(limit).await
    }

    async fn find_graph(&self, id: &GraphId) -> Result<Option<GraphDefinition>, Self::Error> {
        self.find_graph(id).await
    }

    async fn upsert_agent(&self, agent: &AgentDefinition) -> Result<(), Self::Error> {
        self.upsert_agent(agent).await
    }

    async fn list_agents(&self, limit: i64) -> Result<Vec<AgentDefinition>, Self::Error> {
        self.list_agents(limit).await
    }

    async fn find_agent(&self, id: &AgentId) -> Result<Option<AgentDefinition>, Self::Error> {
        self.find_agent(id).await
    }

    async fn upsert_agent_run(&self, run: &AgentRunRecord) -> Result<(), Self::Error> {
        self.upsert_agent_run(run).await
    }

    async fn list_agent_runs(&self, limit: i64) -> Result<Vec<AgentRunRecord>, Self::Error> {
        self.list_agent_runs(limit).await
    }

    async fn find_agent_run(&self, id: &AgentRunId) -> Result<Option<AgentRunRecord>, Self::Error> {
        self.find_agent_run(id).await
    }

    async fn upsert_memory_source(&self, source: &Source) -> Result<(), Self::Error> {
        self.upsert_memory_source(source).await
    }

    async fn list_memory_sources(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Source>, Self::Error> {
        self.list_memory_sources(tenant_id, project_id, limit).await
    }

    async fn find_memory_source(&self, id: &SourceId) -> Result<Option<Source>, Self::Error> {
        self.find_memory_source(id).await
    }

    async fn upsert_memory_evidence(&self, evidence: &Evidence) -> Result<(), Self::Error> {
        self.upsert_memory_evidence(evidence).await
    }

    async fn list_memory_evidence(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Evidence>, Self::Error> {
        self.list_memory_evidence(tenant_id, project_id, limit)
            .await
    }

    async fn find_memory_evidence(&self, id: &EvidenceId) -> Result<Option<Evidence>, Self::Error> {
        self.find_memory_evidence(id).await
    }

    async fn upsert_memory_entity(&self, entity: &Entity) -> Result<(), Self::Error> {
        self.upsert_memory_entity(entity).await
    }

    async fn list_memory_entities(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Entity>, Self::Error> {
        self.list_memory_entities(tenant_id, project_id, limit)
            .await
    }

    async fn find_memory_entity(&self, id: &EntityId) -> Result<Option<Entity>, Self::Error> {
        self.find_memory_entity(id).await
    }

    async fn upsert_memory_claim(&self, claim: &Claim) -> Result<(), Self::Error> {
        self.upsert_memory_claim(claim).await
    }

    async fn list_memory_claims(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Claim>, Self::Error> {
        self.list_memory_claims(tenant_id, project_id, limit).await
    }

    async fn find_memory_claim(&self, id: &ClaimId) -> Result<Option<Claim>, Self::Error> {
        self.find_memory_claim(id).await
    }

    async fn upsert_memory_claim_conflict(
        &self,
        conflict: &ClaimConflict,
    ) -> Result<(), Self::Error> {
        self.upsert_memory_claim_conflict(conflict).await
    }

    async fn list_memory_claim_conflicts(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<ClaimConflict>, Self::Error> {
        self.list_memory_claim_conflicts(tenant_id, project_id, limit)
            .await
    }

    async fn find_memory_claim_conflict(
        &self,
        id: &ClaimConflictId,
    ) -> Result<Option<ClaimConflict>, Self::Error> {
        self.find_memory_claim_conflict(id).await
    }

    async fn upsert_memory_event(&self, event: &Event) -> Result<(), Self::Error> {
        self.upsert_memory_event(event).await
    }

    async fn list_memory_events(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Event>, Self::Error> {
        self.list_memory_events(tenant_id, project_id, limit).await
    }

    async fn find_memory_event(&self, id: &EventId) -> Result<Option<Event>, Self::Error> {
        self.find_memory_event(id).await
    }

    async fn upsert_memory_relationship(
        &self,
        relationship: &Relationship,
    ) -> Result<(), Self::Error> {
        self.upsert_memory_relationship(relationship).await
    }

    async fn list_memory_relationships(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Relationship>, Self::Error> {
        self.list_memory_relationships(tenant_id, project_id, limit)
            .await
    }

    async fn find_memory_relationship(
        &self,
        id: &RelationshipId,
    ) -> Result<Option<Relationship>, Self::Error> {
        self.find_memory_relationship(id).await
    }

    async fn upsert_memory_contract(&self, contract: &MemoryContract) -> Result<(), Self::Error> {
        self.upsert_memory_contract(contract).await
    }

    async fn list_memory_contracts(&self, limit: i64) -> Result<Vec<MemoryContract>, Self::Error> {
        self.list_memory_contracts(limit).await
    }

    async fn find_memory_contract(
        &self,
        id: &MemoryContractId,
    ) -> Result<Option<MemoryContract>, Self::Error> {
        self.find_memory_contract(id).await
    }

    async fn upsert_memory_subgraph(&self, subgraph: &MemorySubgraph) -> Result<(), Self::Error> {
        self.upsert_memory_subgraph(subgraph).await
    }

    async fn list_memory_subgraphs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<MemorySubgraph>, Self::Error> {
        self.list_memory_subgraphs(tenant_id, project_id, limit)
            .await
    }

    async fn find_memory_subgraph(
        &self,
        id: &MemorySubgraphId,
    ) -> Result<Option<MemorySubgraph>, Self::Error> {
        self.find_memory_subgraph(id).await
    }

    async fn upsert_memory_subgraph_member(
        &self,
        member: &MemorySubgraphMember,
    ) -> Result<(), Self::Error> {
        self.upsert_memory_subgraph_member(member).await
    }

    async fn upsert_memory_canonical_entity(
        &self,
        entity: &CanonicalEntity,
    ) -> Result<(), Self::Error> {
        self.upsert_memory_canonical_entity(entity).await
    }

    async fn list_memory_canonical_entities(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<CanonicalEntity>, Self::Error> {
        self.list_memory_canonical_entities(tenant_id, project_id, limit)
            .await
    }

    async fn upsert_memory_entity_resolution(
        &self,
        resolution: &EntityResolution,
    ) -> Result<(), Self::Error> {
        self.upsert_memory_entity_resolution(resolution).await
    }

    async fn list_memory_entity_resolutions(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<EntityResolution>, Self::Error> {
        self.list_memory_entity_resolutions(tenant_id, project_id, limit)
            .await
    }

    async fn find_memory_entity_resolution(
        &self,
        id: &EntityResolutionId,
    ) -> Result<Option<EntityResolution>, Self::Error> {
        self.find_memory_entity_resolution(id).await
    }

    async fn upsert_memory_subgraph_edge(&self, edge: &SubgraphEdge) -> Result<(), Self::Error> {
        self.upsert_memory_subgraph_edge(edge).await
    }

    async fn upsert_memory_summary_trace(&self, trace: &SummaryTrace) -> Result<(), Self::Error> {
        self.upsert_memory_summary_trace(trace).await
    }

    async fn list_memory_summary_traces(
        &self,
        subgraph_id: &MemorySubgraphId,
        summary_claim_id: &ClaimId,
    ) -> Result<Vec<SummaryTrace>, Self::Error> {
        self.list_memory_summary_traces(subgraph_id, summary_claim_id)
            .await
    }

    async fn upsert_memory_entity_graph_attachment(
        &self,
        attachment: &EntityGraphAttachment,
    ) -> Result<(), Self::Error> {
        self.upsert_memory_entity_graph_attachment(attachment).await
    }

    async fn upsert_ingestion_connector(
        &self,
        connector: &IngestionConnector,
    ) -> Result<(), Self::Error> {
        self.upsert_ingestion_connector(connector).await
    }

    async fn list_ingestion_connectors(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<IngestionConnector>, Self::Error> {
        self.list_ingestion_connectors(tenant_id, project_id, limit)
            .await
    }

    async fn find_ingestion_connector(
        &self,
        id: &IngestionConnectorId,
    ) -> Result<Option<IngestionConnector>, Self::Error> {
        self.find_ingestion_connector(id).await
    }

    async fn upsert_ingestion_run(&self, run: &IngestionRun) -> Result<(), Self::Error> {
        self.upsert_ingestion_run(run).await
    }

    async fn list_ingestion_runs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<IngestionRun>, Self::Error> {
        self.list_ingestion_runs(tenant_id, project_id, limit).await
    }

    async fn find_ingestion_run(
        &self,
        id: &IngestionRunId,
    ) -> Result<Option<IngestionRun>, Self::Error> {
        self.find_ingestion_run(id).await
    }

    async fn upsert_ingestion_run_output(
        &self,
        output: &IngestionRunOutputRecord,
    ) -> Result<(), Self::Error> {
        self.upsert_ingestion_run_output(output).await
    }

    async fn list_ingestion_run_outputs(
        &self,
        run_id: &IngestionRunId,
    ) -> Result<Vec<IngestionRunOutputRecord>, Self::Error> {
        self.list_ingestion_run_outputs(run_id).await
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

    async fn set_automation_status(
        &self,
        id: &AutomationId,
        status: capsulet_core::AutomationStatus,
    ) -> Result<Option<Automation>, Self::Error> {
        self.set_automation_status(id, status).await
    }

    async fn delete_automation(&self, id: &AutomationId) -> Result<bool, Self::Error> {
        self.delete_automation(id).await
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

    async fn find_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
    ) -> Result<Option<WorkflowRun>, Self::Error> {
        self.find_workflow_run(workflow_run_id).await
    }

    async fn remove_queued_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
    ) -> Result<Option<WorkflowRun>, Self::Error> {
        self.remove_queued_workflow_run(workflow_run_id).await
    }

    async fn cancel_running_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
    ) -> Result<Option<WorkflowRun>, Self::Error> {
        self.cancel_running_workflow_run(workflow_run_id).await
    }

    async fn resume_workflow_run(
        &self,
        workflow_run_id: &WorkflowRunId,
    ) -> Result<Option<WorkflowRun>, Self::Error> {
        self.resume_workflow_run(workflow_run_id).await
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
        JobArtifactRepository::save_artifact(self, artifact).await
    }
}
