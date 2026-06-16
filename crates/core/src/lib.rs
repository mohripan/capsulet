//! Core domain and application contracts for Capsulet.
//!
//! This crate intentionally avoids infrastructure dependencies. It owns the
//! language of the product: automations, trigger conditions, execution pools,
//! job runs, attempts, and application command/query shapes.

pub mod application;
pub mod component;
pub mod domain;
pub mod ports;

pub use application::{CreateManualRunCommand, JobRunSummary};
pub use component::{ComponentDescriptor, ComponentKind};
pub use domain::{
    ArtifactId, ArtifactObjectKind, Automation, AutomationId, AutomationSettings, AutomationStatus,
    AutomationTrigger, AutomationTriggerKind, ConditionExpr, CustomTriggerPlugin, ExecutionPool,
    ExecutionPoolName, JobArtifact, JobAttemptId, JobDefinition, JobDefinitionId, JobRun, JobRunId,
    JobRunLog, JobRunStatus, ResourceRequirements, RetryPolicy, StateTransitionError, TriggerKind,
    TriggerName, WorkflowDefinition, WorkflowId, WorkflowRun, WorkflowRunId, WorkflowRunStatus,
    WorkflowStatus, WorkflowStep, WorkflowStepId, WorkflowStepRun, WorkflowStepRunId,
};
pub use ports::{JobArtifactRepository, JobRunLogRepository, JobRunRepository};
