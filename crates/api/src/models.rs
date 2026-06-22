use capsulet_core::{
    Automation, AutomationTrigger, CustomTriggerPlugin, JobArtifact, JobDefinition, JobRun,
    JobRunStatus, WorkflowDefinition, WorkflowRun, WorkflowStep, WorkflowStepDependency,
    WorkflowStepRun,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{error::ApiError, http::json_from_string};

#[derive(Debug, Serialize)]
pub(crate) struct HealthResponse {
    pub(crate) status: &'static str,
}

#[derive(Debug, Deserialize)]
pub struct CreateRunRequest {
    pub job_definition_id: String,
    #[serde(alias = "host_group")]
    pub execution_pool: String,
    pub run_id: Option<String>,
    pub python_script: Option<String>,
    pub input: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct CreateJobDefinitionRequest {
    pub id: Option<String>,
    pub name: String,
    pub runtime_image: Option<String>,
    pub python_script: String,
    #[serde(default)]
    pub input_schema: Option<Value>,
    pub retry_max_attempts: Option<u32>,
    pub retry_delay_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkflowRequest {
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub steps: Vec<CreateWorkflowStepRequest>,
    pub dependencies: Option<Vec<CreateWorkflowDependencyRequest>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct JobDefinitionSourceResponse {
    pub(crate) python_script: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkflowEditabilityResponse {
    pub(crate) editable: bool,
    pub(crate) reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TopologyResponse {
    pub(crate) nodes: Vec<TopologyNodeResponse>,
    pub(crate) edges: Vec<TopologyEdgeResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TopologyNodeResponse {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) kind: String,
    pub(crate) status: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct TopologyEdgeResponse {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) label: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkflowStepRequest {
    pub id: Option<String>,
    pub name: String,
    pub job_definition_id: String,
    #[serde(alias = "host_group")]
    pub execution_pool: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateAutomationRequest {
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub workflow_id: String,
    pub status: Option<String>,
    pub trigger_kind: Option<String>,
    pub interval_seconds: Option<i64>,
    pub job_input: Option<Value>,
    pub triggers: Option<Vec<CreateAutomationTriggerRequest>>,
    pub condition: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAutomationTriggerRequest {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub config: Value,
    pub plugin_id: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTriggerPluginRequest {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub runtime_image: String,
    pub command: Vec<String>,
    pub config_schema: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListRunsQuery {
    pub(crate) limit: Option<u16>,
    pub(crate) start_at: Option<String>,
    pub(crate) end_at: Option<String>,
    pub(crate) q: Option<String>,
    pub(crate) state: Option<String>,
    pub(crate) sort: Option<String>,
    pub(crate) direction: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkflowDependencyRequest {
    pub from_step_id: String,
    pub to_step_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListWorkflowRunsQuery {
    pub(crate) limit: Option<u16>,
    pub(crate) start_at: Option<String>,
    pub(crate) end_at: Option<String>,
    pub(crate) q: Option<String>,
    pub(crate) state: Option<String>,
    pub(crate) sort: Option<String>,
    pub(crate) direction: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListJobDefinitionsQuery {
    pub(crate) limit: Option<u16>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListRunsResponse {
    pub(crate) runs: Vec<JobRunResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListJobDefinitionsResponse {
    pub(crate) job_definitions: Vec<JobDefinitionResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListExecutionPoolsResponse {
    pub(crate) execution_pools: Vec<ExecutionPoolResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListHostGroupsResponse {
    pub(crate) host_groups: Vec<HostGroupResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListWorkflowsResponse {
    pub(crate) workflows: Vec<WorkflowResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListAutomationsResponse {
    pub(crate) automations: Vec<AutomationResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListAutomationTriggersResponse {
    pub(crate) triggers: Vec<TriggerResponse>,
    pub(crate) condition: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListTriggerPluginsResponse {
    pub(crate) trigger_plugins: Vec<TriggerPluginResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListWorkflowRunsResponse {
    pub(crate) workflow_runs: Vec<WorkflowRunResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkflowRunLogsResponse {
    pub(crate) workflow_run_id: String,
    pub(crate) workflow_id: String,
    pub(crate) status: String,
    pub(crate) entries: Vec<WorkflowRunLogEntryResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkflowRunLogEntryResponse {
    pub(crate) step_run_id: String,
    pub(crate) workflow_step_id: String,
    pub(crate) job_run_id: String,
    pub(crate) position: i32,
    pub(crate) status: String,
    pub(crate) logs: String,
    pub(crate) object_log_available: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct ExecutionPoolResponse {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) is_default: bool,
    pub(crate) host_group: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct HostGroupResponse {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) is_default: bool,
    pub(crate) execution_pool: String,
    pub(crate) host_count: Option<u32>,
}

#[derive(Debug, Serialize)]
pub(crate) struct JobDefinitionResponse {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) runtime_image: String,
    pub(crate) command: Vec<String>,
    pub(crate) bundle_object_key: String,
    pub(crate) input_schema: Value,
    pub(crate) retry_max_attempts: u32,
    pub(crate) retry_delay_seconds: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkflowResponse {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) status: String,
    pub(crate) steps: Vec<WorkflowStepResponse>,
    pub(crate) dependencies: Vec<WorkflowDependencyResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AuditEventResponse {
    pub(crate) id: i64,
    pub(crate) principal: String,
    pub(crate) role: String,
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) status_code: i32,
    pub(crate) request_id: Option<String>,
    pub(crate) created_at: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListAuditEventsResponse {
    pub(crate) audit_events: Vec<AuditEventResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkflowDependencyResponse {
    pub(crate) from_step_id: String,
    pub(crate) to_step_id: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkflowStepResponse {
    pub(crate) id: String,
    pub(crate) position: i32,
    pub(crate) name: String,
    pub(crate) job_definition_id: String,
    pub(crate) execution_pool: String,
    pub(crate) host_group: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct AutomationResponse {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) workflow_id: String,
    pub(crate) status: String,
    pub(crate) trigger_kind: String,
    pub(crate) interval_seconds: Option<i64>,
    pub(crate) triggers: Vec<TriggerResponse>,
    pub(crate) condition: Value,
    pub(crate) job_input: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct TriggerResponse {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) config: Value,
    pub(crate) plugin_id: Option<String>,
    pub(crate) enabled: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct TriggerPluginResponse {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) runtime_image: String,
    pub(crate) command: Vec<String>,
    pub(crate) config_schema: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkflowRunResponse {
    pub(crate) id: String,
    pub(crate) workflow_id: String,
    pub(crate) automation_id: Option<String>,
    pub(crate) status: String,
    pub(crate) current_step_position: i32,
    pub(crate) created_at: String,
    pub(crate) step_runs: Vec<WorkflowStepRunResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkflowStepRunResponse {
    pub(crate) id: String,
    pub(crate) workflow_step_id: String,
    pub(crate) job_run_id: String,
    pub(crate) position: i32,
    pub(crate) status: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct JobRunResponse {
    pub(crate) id: String,
    pub(crate) job_definition_id: String,
    pub(crate) status: String,
    pub(crate) execution_pool: String,
    pub(crate) host_group: String,
    pub(crate) attempt_count: u32,
    pub(crate) created_at: String,
    pub(crate) input: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct JobRunLogsResponse {
    pub(crate) run_id: String,
    pub(crate) logs: String,
    pub(crate) object_log_available: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListArtifactsResponse {
    pub(crate) artifacts: Vec<ArtifactResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ArtifactResponse {
    pub(crate) id: String,
    pub(crate) run_id: String,
    pub(crate) name: String,
    pub(crate) content_type: String,
    pub(crate) size_bytes: u64,
    pub(crate) kind: String,
}

impl From<&JobArtifact> for ArtifactResponse {
    fn from(artifact: &JobArtifact) -> Self {
        Self {
            id: artifact.id().as_str().to_string(),
            run_id: artifact.run_id().as_str().to_string(),
            name: artifact.name().to_string(),
            content_type: artifact.content_type().to_string(),
            size_bytes: artifact.size_bytes(),
            kind: artifact.kind().as_str().to_string(),
        }
    }
}

impl From<&JobDefinition> for JobDefinitionResponse {
    fn from(definition: &JobDefinition) -> Self {
        Self {
            id: definition.id().as_str().to_string(),
            name: definition.name().to_string(),
            runtime_image: definition.runtime_image().to_string(),
            command: definition.command().to_vec(),
            bundle_object_key: definition.bundle_object_key().to_string(),
            input_schema: json_from_string(definition.input_schema()).unwrap_or_else(|_| json!({})),
            retry_max_attempts: definition.retry_max_attempts(),
            retry_delay_seconds: definition.retry_delay_seconds(),
        }
    }
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
        }
    }
}

impl AutomationResponse {
    pub(crate) fn new(
        automation: &Automation,
        triggers: &[AutomationTrigger],
        condition_json: &str,
    ) -> Result<Self, ApiError> {
        Ok(Self {
            id: automation.id().as_str().to_string(),
            name: automation.name().to_string(),
            description: automation.description().to_string(),
            workflow_id: automation.workflow_id().as_str().to_string(),
            status: automation.status().to_string(),
            trigger_kind: automation.trigger_kind().to_string(),
            interval_seconds: automation.interval_seconds(),
            triggers: triggers.iter().map(TriggerResponse::from).collect(),
            condition: json_from_string(condition_json)?,
            job_input: json_from_string(automation.job_input_json()).unwrap_or_else(|_| json!({})),
        })
    }
}

impl From<&AutomationTrigger> for TriggerResponse {
    fn from(trigger: &AutomationTrigger) -> Self {
        Self {
            name: trigger.name().as_str().to_string(),
            kind: trigger.kind().to_string(),
            config: json_from_string(trigger.config_json()).unwrap_or_else(|_| json!({})),
            plugin_id: trigger.plugin_id().map(str::to_string),
            enabled: trigger.enabled(),
        }
    }
}

impl From<&CustomTriggerPlugin> for TriggerPluginResponse {
    fn from(plugin: &CustomTriggerPlugin) -> Self {
        Self {
            id: plugin.id().to_string(),
            name: plugin.name().to_string(),
            description: plugin.description().to_string(),
            runtime_image: plugin.runtime_image().to_string(),
            command: plugin.command().to_vec(),
            config_schema: json_from_string(plugin.config_schema_json())
                .unwrap_or_else(|_| json!({})),
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
            job_run_id: step_run.job_run_id().as_str().to_string(),
            position: step_run.position(),
            status: step_run.status().to_string(),
        }
    }
}

impl From<&JobRun> for JobRunResponse {
    fn from(run: &JobRun) -> Self {
        Self {
            id: run.id().as_str().to_string(),
            job_definition_id: run.job_definition_id().as_str().to_string(),
            status: status_label(run.status()).to_string(),
            execution_pool: run.execution_pool().as_str().to_string(),
            host_group: run.execution_pool().as_str().to_string(),
            attempt_count: run.attempt_count(),
            created_at: run.created_at().to_string(),
            input: json_from_string(run.input_json()).unwrap_or_else(|_| json!({})),
        }
    }
}

const fn status_label(status: JobRunStatus) -> &'static str {
    match status {
        JobRunStatus::Queued => "queued",
        JobRunStatus::Leased => "leased",
        JobRunStatus::Running => "running",
        JobRunStatus::Succeeded => "succeeded",
        JobRunStatus::Failed => "failed",
        JobRunStatus::Cancelled => "cancelled",
        JobRunStatus::TimedOut => "timed_out",
        JobRunStatus::RetryScheduled => "retry_scheduled",
    }
}
