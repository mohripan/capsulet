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
    Removed,
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
            Self::Removed | Self::Succeeded | Self::Failed | Self::Cancelled | Self::TimedOut
        )
    }
}

impl Display for WorkflowRunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Removed => "removed",
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
    id: WorkflowStepId,
    workflow_id: WorkflowId,
    position: i32,
    name: String,
    job_definition_id: JobDefinitionId,
    execution_pool: ExecutionPoolName,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkflowStepDependency {
    from_step_id: WorkflowStepId,
    to_step_id: WorkflowStepId,
}

impl WorkflowStepDependency {
    #[must_use]
    pub const fn new(from_step_id: WorkflowStepId, to_step_id: WorkflowStepId) -> Self {
        Self {
            from_step_id,
            to_step_id,
        }
    }

    #[must_use]
    pub const fn from_step_id(&self) -> &WorkflowStepId {
        &self.from_step_id
    }

    #[must_use]
    pub const fn to_step_id(&self) -> &WorkflowStepId {
        &self.to_step_id
    }
}

impl WorkflowStep {
    #[must_use]
    pub fn new(
        id: WorkflowStepId,
        workflow_id: WorkflowId,
        position: i32,
        name: impl Into<String>,
        job_definition_id: JobDefinitionId,
        execution_pool: ExecutionPoolName,
    ) -> Self {
        Self {
            id,
            workflow_id,
            position,
            name: name.into(),
            job_definition_id,
            execution_pool,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &WorkflowStepId {
        &self.id
    }

    #[must_use]
    pub const fn workflow_id(&self) -> &WorkflowId {
        &self.workflow_id
    }

    #[must_use]
    pub const fn position(&self) -> i32 {
        self.position
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn job_definition_id(&self) -> &JobDefinitionId {
        &self.job_definition_id
    }

    #[must_use]
    pub const fn execution_pool(&self) -> &ExecutionPoolName {
        &self.execution_pool
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowDefinition {
    id: WorkflowId,
    name: String,
    description: String,
    status: WorkflowStatus,
    steps: Vec<WorkflowStep>,
    dependencies: Vec<WorkflowStepDependency>,
}

impl WorkflowDefinition {
    #[must_use]
    pub fn new(
        id: WorkflowId,
        name: impl Into<String>,
        description: impl Into<String>,
        status: WorkflowStatus,
        steps: Vec<WorkflowStep>,
    ) -> Self {
        let dependencies = steps
            .windows(2)
            .map(|pair| WorkflowStepDependency::new(pair[0].id().clone(), pair[1].id().clone()))
            .collect();
        Self {
            id,
            name: name.into(),
            description: description.into(),
            status,
            steps,
            dependencies,
        }
    }

    #[must_use]
    pub fn with_dependencies(
        id: WorkflowId,
        name: impl Into<String>,
        description: impl Into<String>,
        status: WorkflowStatus,
        steps: Vec<WorkflowStep>,
        dependencies: Vec<WorkflowStepDependency>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            description: description.into(),
            status,
            steps,
            dependencies,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &WorkflowId {
        &self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    #[must_use]
    pub const fn status(&self) -> WorkflowStatus {
        self.status
    }

    #[must_use]
    pub fn steps(&self) -> &[WorkflowStep] {
        &self.steps
    }

    #[must_use]
    pub fn dependencies(&self) -> &[WorkflowStepDependency] {
        &self.dependencies
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutomationSettings {
    status: AutomationStatus,
    trigger_kind: AutomationTriggerKind,
    interval_seconds: Option<i64>,
}

impl AutomationSettings {
    #[must_use]
    pub const fn new(
        status: AutomationStatus,
        trigger_kind: AutomationTriggerKind,
        interval_seconds: Option<i64>,
    ) -> Self {
        Self {
            status,
            trigger_kind,
            interval_seconds,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Automation {
    id: AutomationId,
    name: String,
    description: String,
    workflow_id: WorkflowId,
    job_input_json: String,
    status: AutomationStatus,
    trigger_kind: AutomationTriggerKind,
    interval_seconds: Option<i64>,
}

impl Automation {
    #[must_use]
    pub fn new(
        id: AutomationId,
        name: impl Into<String>,
        description: impl Into<String>,
        workflow_id: WorkflowId,
        job_input_json: impl Into<String>,
        settings: AutomationSettings,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            description: description.into(),
            workflow_id,
            job_input_json: job_input_json.into(),
            status: settings.status,
            trigger_kind: settings.trigger_kind,
            interval_seconds: settings.interval_seconds,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &AutomationId {
        &self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    #[must_use]
    pub const fn workflow_id(&self) -> &WorkflowId {
        &self.workflow_id
    }

    #[must_use]
    pub fn job_input_json(&self) -> &str {
        &self.job_input_json
    }

    #[must_use]
    pub const fn status(&self) -> AutomationStatus {
        self.status
    }

    #[must_use]
    pub const fn trigger_kind(&self) -> AutomationTriggerKind {
        self.trigger_kind
    }

    #[must_use]
    pub const fn interval_seconds(&self) -> Option<i64> {
        self.interval_seconds
    }

    #[must_use]
    pub const fn with_status(mut self, status: AutomationStatus) -> Self {
        self.status = status;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowRun {
    id: WorkflowRunId,
    workflow_id: WorkflowId,
    automation_id: Option<AutomationId>,
    input_json: String,
    status: WorkflowRunStatus,
    current_step_position: i32,
    created_at: String,
}

impl WorkflowRun {
    #[must_use]
    pub fn new(
        id: WorkflowRunId,
        workflow_id: WorkflowId,
        automation_id: Option<AutomationId>,
        input_json: impl Into<String>,
        status: WorkflowRunStatus,
        current_step_position: i32,
        created_at: impl Into<String>,
    ) -> Self {
        Self {
            id,
            workflow_id,
            automation_id,
            input_json: input_json.into(),
            status,
            current_step_position,
            created_at: created_at.into(),
        }
    }

    #[must_use]
    pub const fn id(&self) -> &WorkflowRunId {
        &self.id
    }

    #[must_use]
    pub const fn workflow_id(&self) -> &WorkflowId {
        &self.workflow_id
    }

    #[must_use]
    pub const fn automation_id(&self) -> Option<&AutomationId> {
        self.automation_id.as_ref()
    }

    #[must_use]
    pub fn input_json(&self) -> &str {
        &self.input_json
    }

    #[must_use]
    pub const fn status(&self) -> WorkflowRunStatus {
        self.status
    }

    #[must_use]
    pub const fn current_step_position(&self) -> i32 {
        self.current_step_position
    }

    #[must_use]
    pub fn created_at(&self) -> &str {
        &self.created_at
    }

    #[must_use]
    pub const fn with_status(mut self, status: WorkflowRunStatus) -> Self {
        self.status = status;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowStepRun {
    id: WorkflowStepRunId,
    workflow_run_id: WorkflowRunId,
    workflow_step_id: WorkflowStepId,
    job_run_id: JobRunId,
    position: i32,
    status: WorkflowRunStatus,
}

impl WorkflowStepRun {
    #[must_use]
    pub fn new(
        id: WorkflowStepRunId,
        workflow_run_id: WorkflowRunId,
        workflow_step_id: WorkflowStepId,
        job_run_id: JobRunId,
        position: i32,
        status: WorkflowRunStatus,
    ) -> Self {
        Self {
            id,
            workflow_run_id,
            workflow_step_id,
            job_run_id,
            position,
            status,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &WorkflowStepRunId {
        &self.id
    }

    #[must_use]
    pub const fn workflow_run_id(&self) -> &WorkflowRunId {
        &self.workflow_run_id
    }

    #[must_use]
    pub const fn workflow_step_id(&self) -> &WorkflowStepId {
        &self.workflow_step_id
    }

    #[must_use]
    pub const fn job_run_id(&self) -> &JobRunId {
        &self.job_run_id
    }

    #[must_use]
    pub const fn position(&self) -> i32 {
        self.position
    }

    #[must_use]
    pub const fn status(&self) -> WorkflowRunStatus {
        self.status
    }

    #[must_use]
    pub const fn with_status(mut self, status: WorkflowRunStatus) -> Self {
        self.status = status;
        self
    }
}
