use std::fmt::{self, Display};
use std::str::FromStr;

use super::{
    AutomationId, ExecutionPoolName, JobDefinitionId, JobRunId, ParseDomainValueError, WorkflowId,
    WorkflowRunId, WorkflowStepId, WorkflowStepRunId,
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

impl FromStr for WorkflowStatus {
    type Err = ParseDomainValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "draft" => Ok(Self::Draft),
            "enabled" => Ok(Self::Enabled),
            "disabled" => Ok(Self::Disabled),
            value => Err(ParseDomainValueError::new("workflow status", value)),
        }
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
    Skipped,
}

impl WorkflowRunStatus {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Removed
                | Self::Succeeded
                | Self::Failed
                | Self::Cancelled
                | Self::TimedOut
                | Self::Skipped
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
            Self::Skipped => "skipped",
        })
    }
}

impl FromStr for WorkflowRunStatus {
    type Err = ParseDomainValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "removed" => Ok(Self::Removed),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "timed_out" => Ok(Self::TimedOut),
            "skipped" => Ok(Self::Skipped),
            value => Err(ParseDomainValueError::new("workflow run status", value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorkflowDependencyPolicy {
    Hard,
    Soft,
    Always,
}

impl Display for WorkflowDependencyPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Hard => "hard",
            Self::Soft => "soft",
            Self::Always => "always",
        })
    }
}

impl FromStr for WorkflowDependencyPolicy {
    type Err = ParseDomainValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "hard" => Ok(Self::Hard),
            "soft" => Ok(Self::Soft),
            "always" => Ok(Self::Always),
            value => Err(ParseDomainValueError::new(
                "workflow dependency policy",
                value,
            )),
        }
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
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkflowStepDependency {
    from_step_id: WorkflowStepId,
    to_step_id: WorkflowStepId,
    policy: WorkflowDependencyPolicy,
}

impl WorkflowStepDependency {
    #[must_use]
    pub const fn new(from_step_id: WorkflowStepId, to_step_id: WorkflowStepId) -> Self {
        Self {
            from_step_id,
            to_step_id,
            policy: WorkflowDependencyPolicy::Hard,
        }
    }

    #[must_use]
    pub const fn with_policy(
        from_step_id: WorkflowStepId,
        to_step_id: WorkflowStepId,
        policy: WorkflowDependencyPolicy,
    ) -> Self {
        Self {
            from_step_id,
            to_step_id,
            policy,
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

    #[must_use]
    pub const fn policy(&self) -> WorkflowDependencyPolicy {
        self.policy
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
            timeout_seconds: None,
        }
    }

    #[must_use]
    pub const fn with_timeout_seconds(mut self, timeout_seconds: Option<u64>) -> Self {
        self.timeout_seconds = timeout_seconds;
        self
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

    #[must_use]
    pub const fn timeout_seconds(&self) -> Option<u64> {
        self.timeout_seconds
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
    deadline_seconds: Option<u64>,
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
            deadline_seconds: None,
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
            deadline_seconds: None,
        }
    }

    #[must_use]
    pub const fn with_deadline_seconds(mut self, deadline_seconds: Option<u64>) -> Self {
        self.deadline_seconds = deadline_seconds;
        self
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

    #[must_use]
    pub const fn deadline_seconds(&self) -> Option<u64> {
        self.deadline_seconds
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
    job_run_id: Option<JobRunId>,
    position: i32,
    status: WorkflowRunStatus,
}

impl WorkflowStepRun {
    #[must_use]
    pub fn new(
        id: WorkflowStepRunId,
        workflow_run_id: WorkflowRunId,
        workflow_step_id: WorkflowStepId,
        job_run_id: Option<JobRunId>,
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

    /// Returns the job run assigned to this step.
    ///
    /// # Panics
    ///
    /// Panics when the step has not been assigned a job run yet. Use
    /// [`Self::maybe_job_run_id`] when reading queued or skipped steps.
    #[must_use]
    pub const fn job_run_id(&self) -> &JobRunId {
        self.job_run_id
            .as_ref()
            .expect("workflow step run has no job run")
    }

    #[must_use]
    pub const fn maybe_job_run_id(&self) -> Option<&JobRunId> {
        self.job_run_id.as_ref()
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
