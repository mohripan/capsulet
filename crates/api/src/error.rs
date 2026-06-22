use std::fmt::Display;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum ApiError {
    #[error("authentication required")]
    Unauthorized,
    #[error("insufficient permission; required role: {0}")]
    Forbidden(&'static str),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("unknown job definition: {0}")]
    UnknownJobDefinition(String),
    #[error("job definition source not found: {0}")]
    JobDefinitionSourceNotFound(String),
    #[error("unknown execution pool: {0}")]
    UnknownExecutionPool(String),
    #[error("workflow not found: {0}")]
    WorkflowNotFound(String),
    #[error("workflow cannot be modified while queued or running executions reference it: {0}")]
    WorkflowLocked(String),
    #[error("workflow run not found: {0}")]
    WorkflowRunNotFound(String),
    #[error("invalid workflow run transition: {0}")]
    InvalidWorkflowRunTransition(String),
    #[error("automation not found: {0}")]
    AutomationNotFound(String),
    #[error("trigger plugin not found: {0}")]
    TriggerPluginNotFound(String),
    #[error("job run not found: {0}")]
    RunNotFound(String),
    #[error("job run logs not found: {0}")]
    RunLogsNotFound(String),
    #[error("job artifact not found: {0}")]
    ArtifactNotFound(String),
    #[error("job artifact object not found: {0}")]
    ArtifactObjectNotFound(String),
    #[error("object storage error: {0}")]
    ObjectStore(String),
    #[error("store error: {0}")]
    Store(String),
}

impl ApiError {
    pub(crate) fn validation(error: String) -> Self {
        Self::Validation(error)
    }

    pub(crate) fn store(error: impl Display) -> Self {
        Self::Store(error.to_string())
    }

    pub(crate) fn object_store(error: impl Display) -> Self {
        Self::ObjectStore(error.to_string())
    }

    const fn status_code(&self) -> StatusCode {
        match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::Validation(_) | Self::InvalidWorkflowRunTransition(_) => StatusCode::BAD_REQUEST,
            Self::WorkflowLocked(_) => StatusCode::CONFLICT,
            Self::UnknownJobDefinition(_) | Self::UnknownExecutionPool(_) => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            Self::JobDefinitionSourceNotFound(_)
            | Self::WorkflowNotFound(_)
            | Self::WorkflowRunNotFound(_)
            | Self::AutomationNotFound(_)
            | Self::TriggerPluginNotFound(_)
            | Self::RunNotFound(_)
            | Self::RunLogsNotFound(_)
            | Self::ArtifactNotFound(_)
            | Self::ArtifactObjectNotFound(_) => StatusCode::NOT_FOUND,
            Self::Store(_) | Self::ObjectStore(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    const fn code(&self) -> &'static str {
        match self {
            Self::Unauthorized => "authentication_required",
            Self::Forbidden(_) => "permission_denied",
            Self::Validation(_) => "validation_error",
            Self::UnknownJobDefinition(_) => "unknown_job_definition",
            Self::JobDefinitionSourceNotFound(_) => "job_definition_source_not_found",
            Self::UnknownExecutionPool(_) => "unknown_execution_pool",
            Self::WorkflowNotFound(_) => "workflow_not_found",
            Self::WorkflowLocked(_) => "workflow_locked",
            Self::WorkflowRunNotFound(_) => "workflow_run_not_found",
            Self::InvalidWorkflowRunTransition(_) => "invalid_workflow_run_transition",
            Self::AutomationNotFound(_) => "automation_not_found",
            Self::TriggerPluginNotFound(_) => "trigger_plugin_not_found",
            Self::RunNotFound(_) => "job_run_not_found",
            Self::RunLogsNotFound(_) => "job_run_logs_not_found",
            Self::ArtifactNotFound(_) => "job_artifact_not_found",
            Self::ArtifactObjectNotFound(_) => "job_artifact_object_not_found",
            Self::ObjectStore(_) => "object_store_error",
            Self::Store(_) => "store_error",
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorResponse {
            code: self.code(),
            message: self.to_string(),
        };

        (self.status_code(), Json(body)).into_response()
    }
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    code: &'static str,
    message: String,
}
