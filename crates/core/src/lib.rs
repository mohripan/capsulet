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
    AutomationId, ConditionExpr, ExecutionPool, ExecutionPoolName, JobAttemptId, JobDefinition,
    JobDefinitionId, JobRun, JobRunId, JobRunLog, JobRunStatus, ResourceRequirements,
    StateTransitionError, TriggerName,
};
pub use ports::{JobRunLogRepository, JobRunRepository};
