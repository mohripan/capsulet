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
    Authority, Automation, AutomationId, AutomationStatus, AutomationTrigger, Claim, ClaimId,
    ClaimPolicySpec, ClaimStatus, CompiledMemoryPolicy, ConditionExpr, Confidence,
    ContradictionRuleSpec, CustomTriggerPlugin, Entity, EntityId, EntityTypeSpec, Event, EventId,
    EventTypeSpec, Evidence, EvidenceId, ExecutionPool, ExecutionPoolName, FieldSpec, FieldType,
    GraphDefinition, GraphError, GraphHyperedge, GraphId, GraphNode, GraphPort,
    GraphTransitionMode, GraphTransitionPolicy, HyperedgeEndpoint, HyperedgeId, JobArtifact,
    JobAttemptId, JobDefinition, JobDefinitionId, JobRun, JobRunId, JobRunLog, JobRunStatus,
    JobRunTransition, MemoryContract, MemoryContractAst, MemoryContractError, MemoryContractId,
    MemoryError, MemoryScope, NodeId, NodeKind, Observation, ObservationId, ParseDomainValueError,
    PortDirection, PortId, PortValueType, RelationTypeSpec, Relationship, RelationshipId,
    ResourceRequirements, RetrievalPolicySpec, RetryPolicy, ReviewPolicySpec, Source, SourceId,
    StateTransitionError, TerminationCondition, TraceEventId, TriggerKind, TriggerName,
    TrustPolicySpec, WorkflowDefinition, WorkflowDependencyPolicy, WorkflowGraph,
    WorkflowGraphError, WorkflowId, WorkflowRun, WorkflowRunId, WorkflowRunStatus, WorkflowStatus,
    WorkflowStep, WorkflowStepDependency, WorkflowStepId, WorkflowStepRun, WorkflowStepRunId,
};
