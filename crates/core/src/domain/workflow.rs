use std::fmt::{self, Display};

use super::{
    AutomationId, ExecutionPoolName, JobDefinitionId, JobRunId, WorkflowId, WorkflowRunId,
    WorkflowStepId, WorkflowStepRunId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowStatus {
    Draft,
    Enabled,
    Disabled,
}

impl Display for WorkflowStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Draft => "draft",
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowRunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
}

impl WorkflowRunStatus {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::TimedOut
        )
    }
}

impl Display for WorkflowRunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationStatus {
    Enabled,
    Disabled,
}

impl Display for AutomationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationTriggerKind {
    Manual,
    Interval,
}

impl Display for AutomationTriggerKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Manual => "manual",
            Self::Interval => "interval",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowStep {
    pub id: WorkflowStepId,
    pub workflow_id: WorkflowId,
    pub position: i32,
    pub name: String,
    pub job_definition_id: JobDefinitionId,
    pub execution_pool: ExecutionPoolName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowDefinition {
    pub id: WorkflowId,
    pub name: String,
    pub description: String,
    pub status: WorkflowStatus,
    pub steps: Vec<WorkflowStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Automation {
    pub id: AutomationId,
    pub name: String,
    pub description: String,
    pub workflow_id: WorkflowId,
    pub status: AutomationStatus,
    pub trigger_kind: AutomationTriggerKind,
    pub interval_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowRun {
    pub id: WorkflowRunId,
    pub workflow_id: WorkflowId,
    pub automation_id: Option<AutomationId>,
    pub status: WorkflowRunStatus,
    pub current_step_position: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowStepRun {
    pub id: WorkflowStepRunId,
    pub workflow_run_id: WorkflowRunId,
    pub workflow_step_id: WorkflowStepId,
    pub job_run_id: JobRunId,
    pub position: i32,
    pub status: WorkflowRunStatus,
}
