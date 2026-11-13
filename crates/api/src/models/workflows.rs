//! Workflow definitions, their steps, dependencies and topology, and workflow runs.

use capsulet_core::{
    WorkflowDefinition, WorkflowRun, WorkflowStep, WorkflowStepDependency, WorkflowStepRun,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateWorkflowRequest {
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub steps: Vec<CreateWorkflowStepRequest>,
    pub dependencies: Option<Vec<CreateWorkflowDependencyRequest>>,
    pub deadline_seconds: Option<u64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct WorkflowEditabilityResponse {
    pub(crate) editable: bool,
    pub(crate) reason: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct TopologyResponse {
    pub(crate) nodes: Vec<TopologyNodeResponse>,
    pub(crate) edges: Vec<TopologyEdgeResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct TopologyNodeResponse {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) kind: String,
    pub(crate) status: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct TopologyEdgeResponse {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) label: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateWorkflowStepRequest {
    pub id: Option<String>,
    pub name: String,
    pub job_definition_id: String,
    #[serde(alias = "host_group")]
    pub execution_pool: String,
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateWorkflowDependencyRequest {
    pub from_step_id: String,
    pub to_step_id: String,
    pub policy: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ListWorkflowRunsQuery {
    /// How many to return. Values outside 1 to 500 are clamped into that range.
    #[param(minimum = 1, maximum = 500, default = 100)]
    pub(crate) limit: Option<u16>,
    pub(crate) start_at: Option<String>,
    pub(crate) end_at: Option<String>,
    pub(crate) q: Option<String>,
    pub(crate) state: Option<String>,
    pub(crate) sort: Option<String>,
    pub(crate) direction: Option<String>,
}

impl ListWorkflowRunsQuery {
    pub(crate) const DEFAULT_LIMIT: u16 = 100;
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ListWorkflowsResponse {
    pub(crate) workflows: Vec<WorkflowResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ListWorkflowRunsResponse {
    pub(crate) workflow_runs: Vec<WorkflowRunResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct WorkflowRunLogsResponse {
    pub(crate) workflow_run_id: String,
    pub(crate) workflow_id: String,
    pub(crate) status: String,
    pub(crate) entries: Vec<WorkflowRunLogEntryResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct WorkflowRunLogEntryResponse {
    pub(crate) step_run_id: String,
    pub(crate) workflow_step_id: String,
    pub(crate) job_run_id: Option<String>,
    pub(crate) position: i32,
    pub(crate) status: String,
    pub(crate) logs: String,
    pub(crate) object_log_available: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct WorkflowResponse {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) status: String,
    pub(crate) steps: Vec<WorkflowStepResponse>,
    pub(crate) dependencies: Vec<WorkflowDependencyResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct WorkflowDependencyResponse {
    pub(crate) from_step_id: String,
    pub(crate) to_step_id: String,
    pub(crate) policy: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct WorkflowStepResponse {
    pub(crate) id: String,
    pub(crate) position: i32,
    pub(crate) name: String,
    pub(crate) job_definition_id: String,
    pub(crate) execution_pool: String,
    pub(crate) host_group: String,
    pub(crate) timeout_seconds: Option<u64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct WorkflowRunResponse {
    pub(crate) id: String,
    pub(crate) workflow_id: String,
    pub(crate) automation_id: Option<String>,
    pub(crate) status: String,
    pub(crate) current_step_position: i32,
    pub(crate) created_at: String,
    pub(crate) step_runs: Vec<WorkflowStepRunResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct WorkflowStepRunResponse {
    pub(crate) id: String,
    pub(crate) workflow_step_id: String,
    pub(crate) job_run_id: Option<String>,
    pub(crate) position: i32,
    pub(crate) status: String,
}

impl From<&WorkflowDefinition> for WorkflowResponse {
    fn from(workflow: &WorkflowDefinition) -> Self {
        Self {
            id: workflow.id().as_str().to_string(),
            name: workflow.name().to_string(),
            description: workflow.description().to_string(),
            status: workflow.status().to_string(),
            steps: workflow
                .steps()
                .iter()
                .map(WorkflowStepResponse::from)
                .collect(),
            dependencies: workflow
                .dependencies()
                .iter()
                .map(WorkflowDependencyResponse::from)
                .collect(),
        }
    }
}

impl From<&WorkflowStepDependency> for WorkflowDependencyResponse {
    fn from(dependency: &WorkflowStepDependency) -> Self {
        Self {
            from_step_id: dependency.from_step_id().as_str().to_string(),
            to_step_id: dependency.to_step_id().as_str().to_string(),
            policy: dependency.policy().to_string(),
        }
    }
}

impl From<&WorkflowStep> for WorkflowStepResponse {
    fn from(step: &WorkflowStep) -> Self {
        Self {
            id: step.id().as_str().to_string(),
            position: step.position(),
            name: step.name().to_string(),
            job_definition_id: step.job_definition_id().as_str().to_string(),
            execution_pool: step.execution_pool().as_str().to_string(),
            host_group: step.execution_pool().as_str().to_string(),
            timeout_seconds: step.timeout_seconds(),
        }
    }
}

impl WorkflowRunResponse {
    pub(crate) fn new(run: &WorkflowRun, step_runs: &[WorkflowStepRun]) -> Self {
        Self {
            id: run.id().as_str().to_string(),
            workflow_id: run.workflow_id().as_str().to_string(),
            automation_id: run.automation_id().map(|id| id.as_str().to_string()),
            status: run.status().to_string(),
            current_step_position: run.current_step_position(),
            created_at: run.created_at().to_string(),
            step_runs: step_runs
                .iter()
                .map(WorkflowStepRunResponse::from)
                .collect(),
        }
    }
}

impl From<&WorkflowStepRun> for WorkflowStepRunResponse {
    fn from(step_run: &WorkflowStepRun) -> Self {
        Self {
            id: step_run.id().as_str().to_string(),
            workflow_step_id: step_run.workflow_step_id().as_str().to_string(),
            job_run_id: step_run
                .maybe_job_run_id()
                .map(|job_run_id| job_run_id.as_str().to_string()),
            position: step_run.position(),
            status: step_run.status().to_string(),
        }
    }
}
