//! Core domain model for Capsulet.
//!
//! This crate intentionally avoids infrastructure and application-service
//! dependencies. It owns the language of the product: automations, trigger
//! conditions, execution pools, job runs, attempts, and workflow graphs.

pub mod component;
pub mod domain;

pub use component::{ComponentDescriptor, ComponentKind};
pub use domain::{
    ActionId, AgentBudget, AgentDefinition, AgentId, AgentRunId, AgentRunStatus,
    AgentStateSnapshot, AgentTerminationPolicy, AgentTraceEvent, ArtifactId, ArtifactObjectKind,
    Authority, Automation, AutomationId, AutomationStatus, AutomationTrigger, CanonicalEntity,
    CanonicalEntityId, Claim, ClaimConflict, ClaimConflictId, ClaimConflictStatus, ClaimId,
    ClaimPolicySpec, ClaimStatus, CompiledMemoryPolicy, ConditionExpr, Confidence,
    ContradictionRuleSpec, CustomTriggerPlugin, Entity, EntityGraphAttachment,
    EntityGraphAttachmentId, EntityGraphAttachmentType, EntityId, EntityResolution,
    EntityResolutionId, EntityResolutionStatus, EntityTypeSpec, Event, EventId, EventTypeSpec,
    Evidence, EvidenceId, ExecutionPool, ExecutionPoolName, FieldSpec, FieldType, GraphDefinition,
    GraphError, GraphHyperedge, GraphId, GraphNode, GraphPort, GraphTransitionMode,
    GraphTransitionPolicy, HyperedgeEndpoint, HyperedgeId, IngestionConnector,
    IngestionConnectorConfig, IngestionConnectorId, IngestionConnectorKind, IngestionError,
    IngestionRun, IngestionRunId, IngestionRunOutput, IngestionRunOutputRecord, IngestionRunStatus,
    JobArtifact, JobAttemptId, JobDefinition, JobDefinitionId, JobRun, JobRunId, JobRunLog,
    JobRunStatus, JobRunTransition, MemoryContract, MemoryContractAst, MemoryContractError,
    MemoryContractId, MemoryError, MemoryGraphError, MemoryMemberId, MemoryMemberKind, MemoryScope,
    MemorySubgraph, MemorySubgraphActivation, MemorySubgraphId, MemorySubgraphMember,
    MemorySubgraphMemberId, MemorySubgraphMemberRole, MemorySubgraphOwner, MemorySubgraphOwnerKind,
    MemorySubgraphPermissions, MemorySubgraphStatus, NodeId, NodeKind, Observation, ObservationId,
    ParseDomainValueError, PortDirection, PortId, PortValueType, RelationTypeSpec, Relationship,
    RelationshipId, ResourceRequirements, RetrievalPolicySpec, RetryPolicy, ReviewPolicySpec,
    Source, SourceId, StateTransitionError, SubgraphEdge, SubgraphEdgeId, SummaryTrace,
    SummaryTraceId, TerminationCondition, TraceEventId, TriggerKind, TriggerName, TrustPolicySpec,
    WorkflowDefinition, WorkflowDependencyPolicy, WorkflowGraph, WorkflowGraphError, WorkflowId,
    WorkflowRun, WorkflowRunId, WorkflowRunStatus, WorkflowStatus, WorkflowStep,
    WorkflowStepDependency, WorkflowStepId, WorkflowStepRun, WorkflowStepRunId,
    run_local_text_ingestion,
};
