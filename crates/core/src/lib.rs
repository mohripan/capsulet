//! Core domain model for Capsulet.
//!
//! This crate intentionally avoids infrastructure and application-service
//! dependencies. It owns the language of the product: automations, trigger
//! conditions, execution pools, job runs, attempts, and workflow graphs.

pub mod component;
pub mod domain;

pub use component::{ComponentDescriptor, ComponentKind};
pub use domain::{
    ArtifactId, ArtifactObjectKind, Automation, AutomationId, AutomationStatus, AutomationTrigger,
    ConditionExpr, CustomTriggerPlugin, ExecutionPool, ExecutionPoolName, JobArtifact,
    JobAttemptId, JobDefinition, JobDefinitionId, JobRun, JobRunId, JobRunLog, JobRunStatus,
    JobRunTransition, ParseDomainValueError, ResourceRequirements, RetryPolicy,
    StateTransitionError, TriggerKind, TriggerName, WorkflowDefinition, WorkflowDependencyPolicy,
    WorkflowGraph, WorkflowGraphError, WorkflowId, WorkflowRun, WorkflowRunId, WorkflowRunStatus,
    WorkflowStatus, WorkflowStep, WorkflowStepDependency, WorkflowStepId, WorkflowStepRun,
    WorkflowStepRunId,
};
