//! Job definitions, job runs, their logs and artifacts, and the execution pools they run in.

use capsulet_core::{JobArtifact, JobDefinition, JobRun, JobRunStatus};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};

use crate::http::json_from_string;

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateRunRequest {
    pub job_definition_id: String,
    #[serde(alias = "host_group")]
    pub execution_pool: String,
    pub run_id: Option<String>,
    pub python_script: Option<String>,
    #[schema(value_type = Object)]
    pub input: Option<Value>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateJobDefinitionRequest {
    pub id: Option<String>,
    pub name: String,
    pub runtime_image: Option<String>,
    pub python_script: String,
    #[serde(default)]
    pub python_dependencies: Vec<String>,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub input_schema: Option<Value>,
    pub retry_max_attempts: Option<u32>,
    pub retry_delay_seconds: Option<u64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct JobDefinitionSourceResponse {
    pub(crate) python_script: String,
    pub(crate) python_dependencies: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ListRunsQuery {
    /// How many to return. Values outside 1 to 500 are clamped into that range.
    #[param(minimum = 1, maximum = 500, default = 50)]
    pub(crate) limit: Option<u16>,
    pub(crate) start_at: Option<String>,
    pub(crate) end_at: Option<String>,
    pub(crate) q: Option<String>,
    pub(crate) state: Option<String>,
    pub(crate) sort: Option<String>,
    pub(crate) direction: Option<String>,
}

impl ListRunsQuery {
    pub(crate) const DEFAULT_LIMIT: u16 = 50;
}

#[derive(Debug, Deserialize, ToSchema, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ListJobDefinitionsQuery {
    /// How many to return. Values outside 1 to 500 are clamped into that range.
    #[param(minimum = 1, maximum = 500, default = 100)]
    pub(crate) limit: Option<u16>,
}

impl ListJobDefinitionsQuery {
    pub(crate) const DEFAULT_LIMIT: u16 = 100;
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ListRunsResponse {
    pub(crate) runs: Vec<JobRunResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ListJobDefinitionsResponse {
    pub(crate) job_definitions: Vec<JobDefinitionResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ListExecutionPoolsResponse {
    pub(crate) execution_pools: Vec<ExecutionPoolResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ListHostGroupsResponse {
    pub(crate) host_groups: Vec<HostGroupResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ExecutionPoolResponse {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) is_default: bool,
    pub(crate) host_group: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct HostGroupResponse {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) is_default: bool,
    pub(crate) execution_pool: String,
    pub(crate) host_count: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct JobDefinitionResponse {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) runtime_image: String,
    pub(crate) command: Vec<String>,
    pub(crate) python_dependencies: Vec<String>,
    pub(crate) bundle_object_key: String,
    #[schema(value_type = Object)]
    pub(crate) input_schema: Value,
    pub(crate) retry_max_attempts: u32,
    pub(crate) retry_delay_seconds: u64,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct JobRunResponse {
    pub(crate) id: String,
    pub(crate) job_definition_id: String,
    pub(crate) status: String,
    pub(crate) execution_pool: String,
    pub(crate) host_group: String,
    pub(crate) attempt_count: u32,
    pub(crate) created_at: String,
    #[schema(value_type = Object)]
    pub(crate) input: Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct JobRunLogsResponse {
    pub(crate) run_id: String,
    pub(crate) logs: String,
    pub(crate) object_log_available: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ListArtifactsResponse {
    pub(crate) artifacts: Vec<ArtifactResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
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
            python_dependencies: definition.python_dependencies().to_vec(),
            bundle_object_key: definition.bundle_object_key().to_string(),
            input_schema: json_from_string(definition.input_schema()).unwrap_or_else(|_| json!({})),
            retry_max_attempts: definition.retry_max_attempts(),
            retry_delay_seconds: definition.retry_delay_seconds(),
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

pub(super) const fn status_label(status: JobRunStatus) -> &'static str {
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
