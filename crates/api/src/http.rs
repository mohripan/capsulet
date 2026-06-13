use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use capsulet_core::{
    ArtifactId, ArtifactObjectKind, AutomationId, CreateManualRunCommand, ExecutionPoolName,
    JobArtifact, JobDefinition, JobDefinitionId, JobRunId, RetryPolicy, WorkflowDefinition,
    WorkflowId, WorkflowRunId, WorkflowStatus, WorkflowStep, WorkflowStepId,
};
use capsulet_storage::{ObjectStore, run_object_key};
use serde_json::Value;

use crate::{
    error::ApiError,
    models::{
        ArtifactResponse, CreateJobDefinitionRequest, CreateRunRequest, CreateWorkflowRequest,
        ExecutionPoolResponse, HealthResponse, HostGroupResponse, JobDefinitionResponse,
        JobRunLogsResponse, JobRunResponse, ListArtifactsResponse, ListExecutionPoolsResponse,
        ListHostGroupsResponse, ListJobDefinitionsQuery, ListJobDefinitionsResponse, ListRunsQuery,
        ListRunsResponse, ListWorkflowRunsResponse, ListWorkflowsResponse, WorkflowResponse,
        WorkflowRunResponse,
    },
    state::AppState,
    store::ApiStore,
};
pub fn router<S, O>(state: AppState<S, O>) -> Router
where
    S: ApiStore,
    O: ObjectStore,
{
    Router::new()
        .route("/healthz", get(healthz))
        .route(
            "/v1/job-definitions",
            post(create_job_definition).get(list_job_definitions),
        )
        .route(
            "/v1/job-definitions/{id}",
            get(get_job_definition)
                .put(update_job_definition)
                .delete(delete_job_definition),
        )
        .route("/v1/execution-pools", get(list_execution_pools))
        .route("/v1/host-groups", get(list_host_groups))
        .route("/v1/workflows", post(create_workflow).get(list_workflows))
        .route("/v1/workflows/{id}", get(get_workflow))
        .route(
            "/v1/automations",
            post(crate::automations::create_automation).get(crate::automations::list_automations),
        )
        .route(
            "/v1/automations/{id}",
            get(crate::automations::get_automation),
        )
        .route(
            "/v1/automations/{id}/triggers",
            get(crate::automations::list_automation_triggers),
        )
        .route("/v1/automations/{id}/trigger", post(trigger_automation))
        .route(
            "/v1/trigger-plugins",
            post(crate::automations::create_trigger_plugin)
                .get(crate::automations::list_trigger_plugins),
        )
        .route(
            "/v1/trigger-plugins/{id}",
            get(crate::automations::get_trigger_plugin),
        )
        .route("/v1/workflow-runs", get(list_workflow_runs))
        .route("/v1/jobs/runs", post(create_run).get(list_runs))
        .route("/v1/jobs/runs/{id}", get(get_run))
        .route("/v1/jobs/runs/{id}/cancel", post(cancel_run))
        .route("/v1/jobs/runs/{id}/logs", get(get_run_logs))
        .route("/v1/jobs/runs/{id}/artifacts", get(list_artifacts))
        .route(
            "/v1/jobs/runs/{id}/artifacts/{artifact_id}",
            get(download_artifact),
        )
        .with_state(state)
}

async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn list_execution_pools<S, O>(
    State(state): State<AppState<S, O>>,
) -> Json<ListExecutionPoolsResponse>
where
    S: ApiStore,
    O: ObjectStore,
{
    Json(ListExecutionPoolsResponse {
        execution_pools: state
            .execution_pools
            .iter()
            .enumerate()
            .map(|(index, name)| ExecutionPoolResponse {
                name: name.clone(),
                description: if index == 0 {
                    "Default execution pool".to_string()
                } else {
                    "Configured execution pool".to_string()
                },
                is_default: index == 0,
                host_group: name.clone(),
            })
            .collect(),
    })
}

async fn list_host_groups<S, O>(State(state): State<AppState<S, O>>) -> Json<ListHostGroupsResponse>
where
    S: ApiStore,
    O: ObjectStore,
{
    Json(ListHostGroupsResponse {
        host_groups: state
            .execution_pools
            .iter()
            .enumerate()
            .map(|(index, name)| HostGroupResponse {
                name: name.clone(),
                description: if index == 0 {
                    "Default host group".to_string()
                } else {
                    "Configured host group".to_string()
                },
                is_default: index == 0,
                execution_pool: name.clone(),
                host_count: None,
            })
            .collect(),
    })
}

async fn create_job_definition<S, O>(
    State(state): State<AppState<S, O>>,
    Json(request): Json<CreateJobDefinitionRequest>,
) -> Result<(StatusCode, Json<JobDefinitionResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let definition = build_python_job_definition(&state, request).await?;
    state
        .store
        .upsert_job_definition(&definition)
        .await
        .map_err(ApiError::store)?;

    Ok((
        StatusCode::CREATED,
        Json(JobDefinitionResponse::from(&definition)),
    ))
}

async fn update_job_definition<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
    Json(mut request): Json<CreateJobDefinitionRequest>,
) -> Result<Json<JobDefinitionResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    request.id = Some(id);
    let definition = build_python_job_definition(&state, request).await?;
    state
        .store
        .upsert_job_definition(&definition)
        .await
        .map_err(ApiError::store)?;

    Ok(Json(JobDefinitionResponse::from(&definition)))
}

async fn list_job_definitions<S, O>(
    State(state): State<AppState<S, O>>,
    Query(query): Query<ListJobDefinitionsQuery>,
) -> Result<Json<ListJobDefinitionsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let definitions = state
        .store
        .list_job_definitions(i64::from(limit))
        .await
        .map_err(ApiError::store)?;

    Ok(Json(ListJobDefinitionsResponse {
        job_definitions: definitions
            .iter()
            .map(JobDefinitionResponse::from)
            .collect(),
    }))
}

async fn get_job_definition<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<JobDefinitionResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = JobDefinitionId::new(id).map_err(ApiError::validation)?;
    let Some(definition) = state
        .store
        .find_job_definition(&id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::UnknownJobDefinition(id.as_str().to_string()));
    };

    Ok(Json(JobDefinitionResponse::from(&definition)))
}

async fn delete_job_definition<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = JobDefinitionId::new(id).map_err(ApiError::validation)?;
    let deleted = state
        .store
        .delete_job_definition(&id)
        .await
        .map_err(ApiError::store)?;
    if !deleted {
        return Err(ApiError::UnknownJobDefinition(id.as_str().to_string()));
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn build_python_job_definition<S, O>(
    state: &AppState<S, O>,
    request: CreateJobDefinitionRequest,
) -> Result<JobDefinition, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = JobDefinitionId::new(request.id.unwrap_or_else(|| generated_id("job_definition")))
        .map_err(ApiError::validation)?;
    let name = request.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::Validation(
            "job definition name cannot be empty".to_string(),
        ));
    }
    let script = request.python_script;
    if script.trim().is_empty() {
        return Err(ApiError::Validation(
            "python script cannot be empty".to_string(),
        ));
    }
    let retry_policy = RetryPolicy {
        max_attempts: request.retry_max_attempts.unwrap_or(1),
        delay_seconds: request.retry_delay_seconds.unwrap_or(0),
    };
    let runtime_image = request
        .runtime_image
        .unwrap_or_else(|| "python:3.12-slim".to_string());
    let object_key = format!("bundles/job-definitions/{}/main.py", id.as_str());
    state
        .object_store
        .put(&object_key, script.into_bytes())
        .await
        .map_err(ApiError::object_store)?;

    JobDefinition::new(
        id,
        name,
        runtime_image,
        vec![
            "python".to_string(),
            "/capsulet/workspace/main.py".to_string(),
        ],
        object_key,
        "{}",
        retry_policy,
    )
    .map_err(ApiError::validation)
}

async fn create_workflow<S, O>(
    State(state): State<AppState<S, O>>,
    Json(request): Json<CreateWorkflowRequest>,
) -> Result<(StatusCode, Json<WorkflowResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let workflow = build_workflow(&state, request).await?;
    state
        .store
        .upsert_workflow(&workflow)
        .await
        .map_err(ApiError::store)?;

    Ok((StatusCode::CREATED, Json(WorkflowResponse::from(&workflow))))
}

async fn list_workflows<S, O>(
    State(state): State<AppState<S, O>>,
) -> Result<Json<ListWorkflowsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let workflows = state
        .store
        .list_workflows(100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListWorkflowsResponse {
        workflows: workflows.iter().map(WorkflowResponse::from).collect(),
    }))
}

async fn get_workflow<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = WorkflowId::new(id).map_err(ApiError::validation)?;
    let Some(workflow) = state
        .store
        .find_workflow(&id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::WorkflowNotFound(id.as_str().to_string()));
    };

    Ok(Json(WorkflowResponse::from(&workflow)))
}

async fn build_workflow<S, O>(
    state: &AppState<S, O>,
    request: CreateWorkflowRequest,
) -> Result<WorkflowDefinition, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    if request.steps.is_empty() {
        return Err(ApiError::Validation(
            "workflow must contain at least one step".to_string(),
        ));
    }
    let workflow_id = WorkflowId::new(request.id.unwrap_or_else(|| generated_id("workflow")))
        .map_err(ApiError::validation)?;
    let mut steps = Vec::with_capacity(request.steps.len());
    for (index, step) in request.steps.into_iter().enumerate() {
        let job_definition_id =
            JobDefinitionId::new(step.job_definition_id).map_err(ApiError::validation)?;
        if state
            .store
            .find_job_definition(&job_definition_id)
            .await
            .map_err(ApiError::store)?
            .is_none()
        {
            return Err(ApiError::UnknownJobDefinition(
                job_definition_id.as_str().to_string(),
            ));
        }
        let execution_pool =
            ExecutionPoolName::new(step.execution_pool).map_err(ApiError::validation)?;
        if !state.knows_pool(execution_pool.as_str()) {
            return Err(ApiError::UnknownExecutionPool(
                execution_pool.as_str().to_string(),
            ));
        }
        let position = i32::try_from(index + 1)
            .map_err(|_| ApiError::Validation("too many workflow steps".to_string()))?;
        steps.push(WorkflowStep {
            id: WorkflowStepId::new(format!("{}_step_{position}", workflow_id.as_str()))
                .map_err(ApiError::validation)?,
            workflow_id: workflow_id.clone(),
            position,
            name: step.name,
            job_definition_id,
            execution_pool,
        });
    }

    Ok(WorkflowDefinition {
        id: workflow_id,
        name: request.name,
        description: request.description.unwrap_or_default(),
        status: WorkflowStatus::Enabled,
        steps,
    })
}

pub(crate) fn json_from_string(value: &str) -> Result<Value, ApiError> {
    serde_json::from_str(value).map_err(|error| ApiError::Validation(error.to_string()))
}

async fn trigger_automation<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<WorkflowRunResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let automation_id = AutomationId::new(id).map_err(ApiError::validation)?;
    let Some(automation) = state
        .store
        .find_automation(&automation_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::AutomationNotFound(
            automation_id.as_str().to_string(),
        ));
    };
    let run_id = WorkflowRunId::new(generated_id("workflow_run")).map_err(ApiError::validation)?;
    let run = state
        .store
        .create_workflow_run(&automation.workflow_id, Some(&automation.id), &run_id)
        .await
        .map_err(ApiError::store)?;
    Ok((
        StatusCode::CREATED,
        Json(WorkflowRunResponse::new(&run, &[])),
    ))
}

async fn list_workflow_runs<S, O>(
    State(state): State<AppState<S, O>>,
) -> Result<Json<ListWorkflowRunsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let runs = state
        .store
        .list_workflow_runs(100)
        .await
        .map_err(ApiError::store)?;
    let mut workflow_runs = Vec::with_capacity(runs.len());
    for run in &runs {
        let step_runs = state
            .store
            .list_workflow_step_runs(&run.id)
            .await
            .map_err(ApiError::store)?;
        workflow_runs.push(WorkflowRunResponse::new(run, &step_runs));
    }
    Ok(Json(ListWorkflowRunsResponse { workflow_runs }))
}

async fn create_run<S, O>(
    State(state): State<AppState<S, O>>,
    Json(request): Json<CreateRunRequest>,
) -> Result<(StatusCode, Json<JobRunResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let execution_pool =
        ExecutionPoolName::new(request.execution_pool).map_err(ApiError::validation)?;

    if !state.knows_pool(execution_pool.as_str()) {
        return Err(ApiError::UnknownExecutionPool(
            execution_pool.as_str().to_string(),
        ));
    }

    let run_id = match request.run_id {
        Some(value) => JobRunId::new(value).map_err(ApiError::validation)?,
        None => JobRunId::new(generated_run_id()).map_err(ApiError::validation)?,
    };
    let (job_definition_id, bundle_metadata) = if let Some(script) = request.python_script {
        create_script_definition(&state, &run_id, script).await?
    } else {
        let job_definition_id =
            JobDefinitionId::new(request.job_definition_id).map_err(ApiError::validation)?;
        let exists = state
            .store
            .job_definition_exists(&job_definition_id)
            .await
            .map_err(ApiError::store)?;
        if !exists {
            return Err(ApiError::UnknownJobDefinition(
                job_definition_id.as_str().to_string(),
            ));
        }
        (job_definition_id, None)
    };

    let run = CreateManualRunCommand {
        run_id,
        job_definition_id,
        execution_pool,
    }
    .into_job_run();

    state.store.save_run(&run).await.map_err(ApiError::store)?;
    if let Some(metadata) = bundle_metadata {
        state
            .store
            .save_artifact(&metadata)
            .await
            .map_err(ApiError::store)?;
    }

    Ok((StatusCode::CREATED, Json(JobRunResponse::from(&run))))
}

async fn create_script_definition<S, O>(
    state: &AppState<S, O>,
    run_id: &JobRunId,
    script: String,
) -> Result<(JobDefinitionId, Option<JobArtifact>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    if script.trim().is_empty() {
        return Err(ApiError::Validation(
            "python script cannot be empty".to_string(),
        ));
    }
    let object_key = run_object_key(run_id, ArtifactObjectKind::Bundle, "main.py")
        .map_err(ApiError::object_store)?;
    let size_bytes = u64::try_from(script.len())
        .map_err(|_| ApiError::Validation("python script is too large".to_string()))?;
    state
        .object_store
        .put(&object_key, script.into_bytes())
        .await
        .map_err(ApiError::object_store)?;
    let job_definition_id = JobDefinitionId::new(format!("job_definition_{}", run_id.as_str()))
        .map_err(ApiError::validation)?;
    let definition = JobDefinition::new(
        job_definition_id.clone(),
        format!("Script {}", run_id.as_str()),
        "python:3.12-slim",
        vec![
            "python".to_string(),
            "/capsulet/workspace/main.py".to_string(),
        ],
        object_key.clone(),
        "{}",
        RetryPolicy::no_retry(),
    )
    .map_err(ApiError::validation)?;
    state
        .store
        .upsert_job_definition(&definition)
        .await
        .map_err(ApiError::store)?;
    let metadata = JobArtifact::new(
        ArtifactId::new(format!("bundle_{}_main_py", run_id.as_str()))
            .map_err(ApiError::validation)?,
        run_id.clone(),
        None,
        "main.py",
        object_key,
        "text/x-python",
        size_bytes,
        None,
        ArtifactObjectKind::Bundle,
    )
    .map_err(ApiError::validation)?;
    Ok((job_definition_id, Some(metadata)))
}

async fn list_runs<S, O>(
    State(state): State<AppState<S, O>>,
    Query(query): Query<ListRunsQuery>,
) -> Result<Json<ListRunsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let runs = state
        .store
        .list_runs(i64::from(limit))
        .await
        .map_err(ApiError::store)?;

    Ok(Json(ListRunsResponse {
        runs: runs.iter().map(JobRunResponse::from).collect(),
    }))
}

async fn get_run<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<JobRunResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = JobRunId::new(id).map_err(ApiError::validation)?;
    let Some(run) = state.store.find_run(&id).await.map_err(ApiError::store)? else {
        return Err(ApiError::RunNotFound(id.as_str().to_string()));
    };

    Ok(Json(JobRunResponse::from(&run)))
}

async fn get_run_logs<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<JobRunLogsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = JobRunId::new(id).map_err(ApiError::validation)?;
    if state
        .store
        .find_run(&id)
        .await
        .map_err(ApiError::store)?
        .is_none()
    {
        return Err(ApiError::RunNotFound(id.as_str().to_string()));
    }

    let Some(log) = state
        .store
        .find_run_log(&id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::RunLogsNotFound(id.as_str().to_string()));
    };

    let object_log_available = state
        .store
        .list_artifacts(&id)
        .await
        .map_err(ApiError::store)?
        .iter()
        .any(|artifact| artifact.kind == ArtifactObjectKind::Log);

    Ok(Json(JobRunLogsResponse {
        run_id: log.run_id.as_str().to_string(),
        logs: log.text,
        object_log_available,
    }))
}

async fn cancel_run<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<JobRunResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = JobRunId::new(id).map_err(ApiError::validation)?;
    let Some(run) = state.store.cancel_run(&id).await.map_err(ApiError::store)? else {
        return Err(ApiError::RunNotFound(id.as_str().to_string()));
    };

    Ok(Json(JobRunResponse::from(&run)))
}

async fn list_artifacts<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<ListArtifactsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = JobRunId::new(id).map_err(ApiError::validation)?;
    if state
        .store
        .find_run(&id)
        .await
        .map_err(ApiError::store)?
        .is_none()
    {
        return Err(ApiError::RunNotFound(id.as_str().to_string()));
    }

    let artifacts = state
        .store
        .list_artifacts(&id)
        .await
        .map_err(ApiError::store)?;

    Ok(Json(ListArtifactsResponse {
        artifacts: artifacts.iter().map(ArtifactResponse::from).collect(),
    }))
}

async fn download_artifact<S, O>(
    State(state): State<AppState<S, O>>,
    Path((id, artifact_id)): Path<(String, String)>,
) -> Result<Response, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = JobRunId::new(id).map_err(ApiError::validation)?;
    let artifact_id = ArtifactId::new(artifact_id).map_err(ApiError::validation)?;
    if state
        .store
        .find_run(&id)
        .await
        .map_err(ApiError::store)?
        .is_none()
    {
        return Err(ApiError::RunNotFound(id.as_str().to_string()));
    }

    let Some(artifact) = state
        .store
        .find_artifact(&id, &artifact_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::ArtifactNotFound(artifact_id.as_str().to_string()));
    };
    let Some(bytes) = state
        .object_store
        .get(&artifact.object_key)
        .await
        .map_err(ApiError::object_store)?
    else {
        return Err(ApiError::ArtifactObjectNotFound(artifact.object_key));
    };

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, artifact.content_type),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", artifact.name),
            ),
        ],
        bytes,
    )
        .into_response())
}

fn generated_run_id() -> String {
    generated_id("run")
}

pub(crate) fn generated_id(prefix: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    format!("{prefix}_{millis}")
}
