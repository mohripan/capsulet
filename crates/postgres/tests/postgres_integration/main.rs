//!
//! One module per area of what is tested; what they share is here.

use capsulet_application::{
    AgentRunRecord, AgentTraceRecord, JobRunLogRepository, JobRunRepository,
};

use capsulet_core::{
    AgentBudget, AgentDefinition, AgentId, AgentRunId, AgentRunStatus, AgentTerminationPolicy,
    ArtifactId, ArtifactObjectKind, Authority, Automation, AutomationId, AutomationStatus,
    AutomationTrigger, CanonicalEntity, CanonicalEntityId, Claim, ClaimId, ClaimStatus, Confidence,
    CustomTriggerPlugin, Entity, EntityGraphAttachment, EntityGraphAttachmentId,
    EntityGraphAttachmentType, EntityId, EntityResolution, EntityResolutionId,
    EntityResolutionStatus, Evidence, EvidenceId, ExecutionPoolName, GraphDefinition,
    GraphHyperedge, GraphId, GraphNode, GraphPort, GraphTransitionPolicy, HyperedgeEndpoint,
    HyperedgeId, IngestionConnector, IngestionConnectorConfig, IngestionConnectorId,
    IngestionConnectorKind, IngestionRun, IngestionRunId, IngestionRunOutputRecord, JobArtifact,
    JobDefinition, JobRun, JobRunId, JobRunLog, JobRunTransition, MemoryContract, MemoryContractId,
    MemoryMemberId, MemoryMemberKind, MemoryScope, MemorySubgraph, MemorySubgraphActivation,
    MemorySubgraphId, MemorySubgraphMember, MemorySubgraphMemberId, MemorySubgraphMemberRole,
    MemorySubgraphOwner, MemorySubgraphOwnerKind, MemorySubgraphPermissions, MemorySubgraphStatus,
    NodeId, NodeKind, PortDirection, PortId, PortValueType, Source, SourceId, SubgraphEdge,
    SubgraphEdgeId, SummaryTrace, SummaryTraceId, TriggerKind, TriggerName, WorkflowDefinition,
    WorkflowId, WorkflowStatus, WorkflowStep, WorkflowStepDependency, WorkflowStepId,
};

use capsulet_core::JobRunStatus;

use capsulet_postgres::{PostgresStore, PostgresStoreError};

#[path = "../support/mod.rs"]
mod support;

mod agents;
mod automations;
mod identity;
mod ingestion;
mod jobs;
mod leases;
mod memory;
mod workflows;

use support::{fixture_id as unique_id, required_database_url as database_url};

async fn scoped_store() -> PostgresStore {
    let store = PostgresStore::connect(&database_url())
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");
    store
}
