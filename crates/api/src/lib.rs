use std::{
    fmt::{self, Display},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use capsulet_core::{
    ArtifactId, ArtifactObjectKind, Automation, AutomationId, AutomationStatus,
    AutomationTriggerKind, CreateManualRunCommand, ExecutionPoolName, JobArtifact, JobDefinition,
    JobDefinitionId, JobRun, JobRunId, JobRunLog, JobRunLogRepository, JobRunRepository,
    JobRunStatus, RetryPolicy, WorkflowDefinition, WorkflowId, WorkflowRun, WorkflowRunId,
    WorkflowStatus, WorkflowStep, WorkflowStepId, WorkflowStepRun,
};
use capsulet_postgres::{PostgresStore, PostgresStoreError};
use capsulet_storage::{ObjectStore, run_object_key};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Shared API state.
#[derive(Clone)]
pub struct AppState<S, O> {
    store: S,
    object_store: O,
    execution_pools: Arc<Vec<String>>,
}

impl<S, O> AppState<S, O> {
    /// Creates API state.
    #[must_use]
    pub fn new(
        store: S,
        object_store: O,
        execution_pools: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            store,
            object_store,
            execution_pools: Arc::new(
                execution_pools
                    .into_iter()
                    .map(|pool| pool.trim().to_string())
                    .filter(|pool| !pool.is_empty())
                    .collect(),
            ),
        }
    }

    fn knows_pool(&self, pool: &str) -> bool {
        self.execution_pools.iter().any(|known| known == pool)
    }
}

/// Storage operations required by the HTTP API.
#[async_trait]
pub trait ApiStore: Clone + Send + Sync + 'static {
    type Error: Display + Send + Sync + 'static;

    async fn job_definition_exists(&self, id: &JobDefinitionId) -> Result<bool, Self::Error>;
    async fn upsert_job_definition(&self, definition: &JobDefinition) -> Result<(), Self::Error>;
    async fn list_job_definitions(&self, limit: i64) -> Result<Vec<JobDefinition>, Self::Error>;
    async fn find_job_definition(
        &self,
        id: &JobDefinitionId,
    ) -> Result<Option<JobDefinition>, Self::Error>;
    async fn delete_job_definition(&self, id: &JobDefinitionId) -> Result<bool, Self::Error>;
    async fn upsert_workflow(&self, workflow: &WorkflowDefinition) -> Result<(), Self::Error>;
    async fn list_workflows(&self, limit: i64) -> Result<Vec<WorkflowDefinition>, Self::Error>;
    async fn find_workflow(
        &self,
        id: &WorkflowId,
    ) -> Result<Option<WorkflowDefinition>, Self::Error>;
    async fn upsert_automation(&self, automation: &Automation) -> Result<(), Self::Error>;
    async fn list_automations(&self, limit: i64) -> Result<Vec<Automation>, Self::Error>;
    async fn find_automation(&self, id: &AutomationId) -> Result<Option<Automation>, Self::Error>;
    async fn create_workflow_run(
        &self,
        workflow_id: &WorkflowId,
        automation_id: Option<&AutomationId>,
        run_id: &WorkflowRunId,
    ) -> Result<WorkflowRun, Self::Error>;
    async fn list_workflow_runs(&self, limit: i64) -> Result<Vec<WorkflowRun>, Self::Error>;
    async fn list_workflow_step_runs(
        &self,
        workflow_run_id: &WorkflowRunId,
    ) -> Result<Vec<WorkflowStepRun>, Self::Error>;
    async fn save_run(&self, run: &JobRun) -> Result<(), Self::Error>;
    async fn list_runs(&self, limit: i64) -> Result<Vec<JobRun>, Self::Error>;
    async fn find_run(&self, id: &JobRunId) -> Result<Option<JobRun>, Self::Error>;
    async fn find_run_log(&self, id: &JobRunId) -> Result<Option<JobRunLog>, Self::Error>;
    async fn cancel_run(&self, id: &JobRunId) -> Result<Option<JobRun>, Self::Error>;
    async fn list_artifacts(&self, id: &JobRunId) -> Result<Vec<JobArtifact>, Self::Error>;
    async fn find_artifact(
        &self,
        run_id: &JobRunId,
        artifact_id: &ArtifactId,
    ) -> Result<Option<JobArtifact>, Self::Error>;
    async fn save_artifact(&self, artifact: &JobArtifact) -> Result<(), Self::Error>;
}

#[async_trait]
impl ApiStore for PostgresStore {
    type Error = PostgresStoreError;

    async fn job_definition_exists(&self, id: &JobDefinitionId) -> Result<bool, Self::Error> {
        self.job_definition_exists(id).await
    }

    async fn save_run(&self, run: &JobRun) -> Result<(), Self::Error> {
        self.save(run).await
    }

    async fn upsert_job_definition(&self, definition: &JobDefinition) -> Result<(), Self::Error> {
        self.upsert_job_definition(definition).await
    }

    async fn list_job_definitions(&self, limit: i64) -> Result<Vec<JobDefinition>, Self::Error> {
        self.list_job_definitions(limit).await
    }

    async fn find_job_definition(
        &self,
        id: &JobDefinitionId,
    ) -> Result<Option<JobDefinition>, Self::Error> {
        self.find_job_definition(id).await
    }

    async fn delete_job_definition(&self, id: &JobDefinitionId) -> Result<bool, Self::Error> {
        self.delete_job_definition(id).await
    }

    async fn upsert_workflow(&self, workflow: &WorkflowDefinition) -> Result<(), Self::Error> {
        self.upsert_workflow(workflow).await
    }

    async fn list_workflows(&self, limit: i64) -> Result<Vec<WorkflowDefinition>, Self::Error> {
        self.list_workflows(limit).await
    }

    async fn find_workflow(
        &self,
        id: &WorkflowId,
    ) -> Result<Option<WorkflowDefinition>, Self::Error> {
        self.find_workflow(id).await
    }

    async fn upsert_automation(&self, automation: &Automation) -> Result<(), Self::Error> {
        self.upsert_automation(automation).await
    }

    async fn list_automations(&self, limit: i64) -> Result<Vec<Automation>, Self::Error> {
        self.list_automations(limit).await
    }

    async fn find_automation(&self, id: &AutomationId) -> Result<Option<Automation>, Self::Error> {
        self.find_automation(id).await
    }

    async fn create_workflow_run(
        &self,
        workflow_id: &WorkflowId,
        automation_id: Option<&AutomationId>,
        run_id: &WorkflowRunId,
    ) -> Result<WorkflowRun, Self::Error> {
        self.create_workflow_run(workflow_id, automation_id, run_id)
            .await
    }

    async fn list_workflow_runs(&self, limit: i64) -> Result<Vec<WorkflowRun>, Self::Error> {
        self.list_workflow_runs(limit).await
    }

    async fn list_workflow_step_runs(
        &self,
        workflow_run_id: &WorkflowRunId,
    ) -> Result<Vec<WorkflowStepRun>, Self::Error> {
        self.list_workflow_step_runs(workflow_run_id).await
    }

    async fn list_runs(&self, limit: i64) -> Result<Vec<JobRun>, Self::Error> {
        self.list_job_runs(limit).await
    }

    async fn find_run(&self, id: &JobRunId) -> Result<Option<JobRun>, Self::Error> {
        self.find_by_id(id).await
    }

    async fn find_run_log(&self, id: &JobRunId) -> Result<Option<JobRunLog>, Self::Error> {
        self.find_log_by_run_id(id).await
    }

    async fn cancel_run(&self, id: &JobRunId) -> Result<Option<JobRun>, Self::Error> {
        self.cancel_run(id).await
    }

    async fn list_artifacts(&self, id: &JobRunId) -> Result<Vec<JobArtifact>, Self::Error> {
        self.list_artifacts(id).await
    }

    async fn find_artifact(
        &self,
        run_id: &JobRunId,
        artifact_id: &ArtifactId,
    ) -> Result<Option<JobArtifact>, Self::Error> {
        self.find_artifact(run_id, artifact_id).await
    }

    async fn save_artifact(&self, artifact: &JobArtifact) -> Result<(), Self::Error> {
        capsulet_core::JobArtifactRepository::save_artifact(self, artifact).await
    }
}

/// Builds the Capsulet API router.
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
        .route("/v1/workflows", post(create_workflow).get(list_workflows))
        .route("/v1/workflows/{id}", get(get_workflow))
        .route(
            "/v1/automations",
            post(create_automation).get(list_automations),
        )
        .route("/v1/automations/{id}", get(get_automation))
        .route("/v1/automations/{id}/trigger", post(trigger_automation))
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

async fn create_automation<S, O>(
    State(state): State<AppState<S, O>>,
    Json(request): Json<CreateAutomationRequest>,
) -> Result<(StatusCode, Json<AutomationResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let automation = build_automation(&state, request).await?;
    state
        .store
        .upsert_automation(&automation)
        .await
        .map_err(ApiError::store)?;

    Ok((
        StatusCode::CREATED,
        Json(AutomationResponse::from(&automation)),
    ))
}

async fn list_automations<S, O>(
    State(state): State<AppState<S, O>>,
) -> Result<Json<ListAutomationsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let automations = state
        .store
        .list_automations(100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListAutomationsResponse {
        automations: automations.iter().map(AutomationResponse::from).collect(),
    }))
}

async fn get_automation<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<AutomationResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = AutomationId::new(id).map_err(ApiError::validation)?;
    let Some(automation) = state
        .store
        .find_automation(&id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::AutomationNotFound(id.as_str().to_string()));
    };
    Ok(Json(AutomationResponse::from(&automation)))
}

async fn build_automation<S, O>(
    state: &AppState<S, O>,
    request: CreateAutomationRequest,
) -> Result<Automation, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let workflow_id = WorkflowId::new(request.workflow_id).map_err(ApiError::validation)?;
    if state
        .store
        .find_workflow(&workflow_id)
        .await
        .map_err(ApiError::store)?
        .is_none()
    {
        return Err(ApiError::WorkflowNotFound(workflow_id.as_str().to_string()));
    }
    let trigger_kind = match request.trigger_kind.as_deref().unwrap_or("manual") {
        "manual" => AutomationTriggerKind::Manual,
        "interval" => AutomationTriggerKind::Interval,
        value => {
            return Err(ApiError::Validation(format!(
                "unsupported automation trigger kind: {value}"
            )));
        }
    };
    if trigger_kind == AutomationTriggerKind::Interval && request.interval_seconds.is_none() {
        return Err(ApiError::Validation(
            "interval automations require interval_seconds".to_string(),
        ));
    }
    Ok(Automation {
        id: AutomationId::new(request.id.unwrap_or_else(|| generated_id("automation")))
            .map_err(ApiError::validation)?,
        name: request.name,
        description: request.description.unwrap_or_default(),
        workflow_id,
        status: AutomationStatus::Enabled,
        trigger_kind,
        interval_seconds: request.interval_seconds,
    })
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

fn generated_id(prefix: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    format!("{prefix}_{millis}")
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Debug, Deserialize)]
pub struct CreateRunRequest {
    pub job_definition_id: String,
    pub execution_pool: String,
    pub run_id: Option<String>,
    pub python_script: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateJobDefinitionRequest {
    pub id: Option<String>,
    pub name: String,
    pub runtime_image: Option<String>,
    pub python_script: String,
    pub retry_max_attempts: Option<u32>,
    pub retry_delay_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkflowRequest {
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub steps: Vec<CreateWorkflowStepRequest>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkflowStepRequest {
    pub name: String,
    pub job_definition_id: String,
    pub execution_pool: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateAutomationRequest {
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub workflow_id: String,
    pub trigger_kind: Option<String>,
    pub interval_seconds: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ListRunsQuery {
    limit: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct ListJobDefinitionsQuery {
    limit: Option<u16>,
}

#[derive(Debug, Serialize)]
struct ListRunsResponse {
    runs: Vec<JobRunResponse>,
}

#[derive(Debug, Serialize)]
struct ListJobDefinitionsResponse {
    job_definitions: Vec<JobDefinitionResponse>,
}

#[derive(Debug, Serialize)]
struct ListExecutionPoolsResponse {
    execution_pools: Vec<ExecutionPoolResponse>,
}

#[derive(Debug, Serialize)]
struct ListWorkflowsResponse {
    workflows: Vec<WorkflowResponse>,
}

#[derive(Debug, Serialize)]
struct ListAutomationsResponse {
    automations: Vec<AutomationResponse>,
}

#[derive(Debug, Serialize)]
struct ListWorkflowRunsResponse {
    workflow_runs: Vec<WorkflowRunResponse>,
}

#[derive(Debug, Serialize)]
struct ExecutionPoolResponse {
    name: String,
    description: String,
    is_default: bool,
}

#[derive(Debug, Serialize)]
struct JobDefinitionResponse {
    id: String,
    name: String,
    runtime_image: String,
    command: Vec<String>,
    bundle_object_key: String,
    retry_max_attempts: u32,
    retry_delay_seconds: u64,
}

#[derive(Debug, Serialize)]
struct WorkflowResponse {
    id: String,
    name: String,
    description: String,
    status: String,
    steps: Vec<WorkflowStepResponse>,
}

#[derive(Debug, Serialize)]
struct WorkflowStepResponse {
    id: String,
    position: i32,
    name: String,
    job_definition_id: String,
    execution_pool: String,
}

#[derive(Debug, Serialize)]
struct AutomationResponse {
    id: String,
    name: String,
    description: String,
    workflow_id: String,
    status: String,
    trigger_kind: String,
    interval_seconds: Option<i64>,
}

#[derive(Debug, Serialize)]
struct WorkflowRunResponse {
    id: String,
    workflow_id: String,
    automation_id: Option<String>,
    status: String,
    current_step_position: i32,
    step_runs: Vec<WorkflowStepRunResponse>,
}

#[derive(Debug, Serialize)]
struct WorkflowStepRunResponse {
    id: String,
    workflow_step_id: String,
    job_run_id: String,
    position: i32,
    status: String,
}

#[derive(Debug, Serialize)]
struct JobRunResponse {
    id: String,
    job_definition_id: String,
    status: String,
    execution_pool: String,
    attempt_count: u32,
}

#[derive(Debug, Serialize)]
struct JobRunLogsResponse {
    run_id: String,
    logs: String,
    object_log_available: bool,
}

#[derive(Debug, Serialize)]
struct ListArtifactsResponse {
    artifacts: Vec<ArtifactResponse>,
}

#[derive(Debug, Serialize)]
struct ArtifactResponse {
    id: String,
    run_id: String,
    name: String,
    content_type: String,
    size_bytes: u64,
    kind: String,
}

impl From<&JobArtifact> for ArtifactResponse {
    fn from(artifact: &JobArtifact) -> Self {
        Self {
            id: artifact.id.as_str().to_string(),
            run_id: artifact.run_id.as_str().to_string(),
            name: artifact.name.clone(),
            content_type: artifact.content_type.clone(),
            size_bytes: artifact.size_bytes,
            kind: artifact.kind.as_str().to_string(),
        }
    }
}

impl From<&JobDefinition> for JobDefinitionResponse {
    fn from(definition: &JobDefinition) -> Self {
        Self {
            id: definition.id.as_str().to_string(),
            name: definition.name.clone(),
            runtime_image: definition.runtime_image.clone(),
            command: definition.command.clone(),
            bundle_object_key: definition.bundle_object_key.clone(),
            retry_max_attempts: definition.retry_max_attempts,
            retry_delay_seconds: definition.retry_delay_seconds,
        }
    }
}

impl From<&WorkflowDefinition> for WorkflowResponse {
    fn from(workflow: &WorkflowDefinition) -> Self {
        Self {
            id: workflow.id.as_str().to_string(),
            name: workflow.name.clone(),
            description: workflow.description.clone(),
            status: workflow.status.to_string(),
            steps: workflow
                .steps
                .iter()
                .map(WorkflowStepResponse::from)
                .collect(),
        }
    }
}

impl From<&WorkflowStep> for WorkflowStepResponse {
    fn from(step: &WorkflowStep) -> Self {
        Self {
            id: step.id.as_str().to_string(),
            position: step.position,
            name: step.name.clone(),
            job_definition_id: step.job_definition_id.as_str().to_string(),
            execution_pool: step.execution_pool.as_str().to_string(),
        }
    }
}

impl From<&Automation> for AutomationResponse {
    fn from(automation: &Automation) -> Self {
        Self {
            id: automation.id.as_str().to_string(),
            name: automation.name.clone(),
            description: automation.description.clone(),
            workflow_id: automation.workflow_id.as_str().to_string(),
            status: automation.status.to_string(),
            trigger_kind: automation.trigger_kind.to_string(),
            interval_seconds: automation.interval_seconds,
        }
    }
}

impl WorkflowRunResponse {
    fn new(run: &WorkflowRun, step_runs: &[WorkflowStepRun]) -> Self {
        Self {
            id: run.id.as_str().to_string(),
            workflow_id: run.workflow_id.as_str().to_string(),
            automation_id: run.automation_id.as_ref().map(|id| id.as_str().to_string()),
            status: run.status.to_string(),
            current_step_position: run.current_step_position,
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
            id: step_run.id.as_str().to_string(),
            workflow_step_id: step_run.workflow_step_id.as_str().to_string(),
            job_run_id: step_run.job_run_id.as_str().to_string(),
            position: step_run.position,
            status: step_run.status.to_string(),
        }
    }
}

impl From<&JobRun> for JobRunResponse {
    fn from(run: &JobRun) -> Self {
        Self {
            id: run.id.as_str().to_string(),
            job_definition_id: run.job_definition_id.as_str().to_string(),
            status: status_label(run.status).to_string(),
            execution_pool: run.execution_pool.as_str().to_string(),
            attempt_count: run.attempt_count,
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

#[derive(Debug, Error)]
enum ApiError {
    #[error("validation error: {0}")]
    Validation(String),
    #[error("unknown job definition: {0}")]
    UnknownJobDefinition(String),
    #[error("unknown execution pool: {0}")]
    UnknownExecutionPool(String),
    #[error("workflow not found: {0}")]
    WorkflowNotFound(String),
    #[error("automation not found: {0}")]
    AutomationNotFound(String),
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
    fn validation(error: String) -> Self {
        Self::Validation(error)
    }

    fn store(error: impl Display) -> Self {
        Self::Store(error.to_string())
    }

    fn object_store(error: impl Display) -> Self {
        Self::ObjectStore(error.to_string())
    }

    const fn status_code(&self) -> StatusCode {
        match self {
            Self::Validation(_) => StatusCode::BAD_REQUEST,
            Self::UnknownJobDefinition(_) | Self::UnknownExecutionPool(_) => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            Self::WorkflowNotFound(_)
            | Self::AutomationNotFound(_)
            | Self::RunNotFound(_)
            | Self::RunLogsNotFound(_)
            | Self::ArtifactNotFound(_)
            | Self::ArtifactObjectNotFound(_) => StatusCode::NOT_FOUND,
            Self::Store(_) | Self::ObjectStore(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    const fn code(&self) -> &'static str {
        match self {
            Self::Validation(_) => "validation_error",
            Self::UnknownJobDefinition(_) => "unknown_job_definition",
            Self::UnknownExecutionPool(_) => "unknown_execution_pool",
            Self::WorkflowNotFound(_) => "workflow_not_found",
            Self::AutomationNotFound(_) => "automation_not_found",
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

impl<O> fmt::Debug for AppState<PostgresStore, O>
where
    O: fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AppState")
            .field("store", &self.store)
            .field("object_store", &self.object_store)
            .field("execution_pools", &self.execution_pools)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::{
        body::{Body, to_bytes},
        http::{Method, Request},
    };
    use capsulet_core::{
        ArtifactId, ArtifactObjectKind, Automation, AutomationId, ExecutionPoolName, JobArtifact,
        JobDefinition, JobDefinitionId, JobRun, JobRunId, JobRunLog, JobRunStatus,
        WorkflowDefinition, WorkflowId, WorkflowRun, WorkflowRunId, WorkflowStepRun,
    };
    use capsulet_storage::ObjectStore;
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use super::{ApiStore, AppState, router};

    #[derive(Debug, Clone, Default)]
    struct FakeStore {
        known_definitions: Arc<Mutex<Vec<String>>>,
        runs: Arc<Mutex<Vec<JobRun>>>,
        logs: Arc<Mutex<Vec<JobRunLog>>>,
        artifacts: Arc<Mutex<Vec<JobArtifact>>>,
        workflows: Arc<Mutex<Vec<WorkflowDefinition>>>,
        automations: Arc<Mutex<Vec<Automation>>>,
        workflow_runs: Arc<Mutex<Vec<WorkflowRun>>>,
    }

    #[derive(Debug, Clone, Default)]
    struct FakeObjectStore {
        objects: ObjectMap,
    }

    type ObjectMap = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

    #[async_trait::async_trait]
    impl ObjectStore for FakeObjectStore {
        type Error = String;

        async fn put(&self, key: &str, bytes: Vec<u8>) -> Result<(), Self::Error> {
            self.objects
                .lock()
                .map_err(|error| error.to_string())?
                .push((key.to_string(), bytes));
            Ok(())
        }

        async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Self::Error> {
            Ok(self
                .objects
                .lock()
                .map_err(|error| error.to_string())?
                .iter()
                .rev()
                .find(|(stored_key, _)| stored_key == key)
                .map(|(_, bytes)| bytes.clone()))
        }

        async fn exists(&self, key: &str) -> Result<bool, Self::Error> {
            Ok(self
                .objects
                .lock()
                .map_err(|error| error.to_string())?
                .iter()
                .any(|(stored_key, _)| stored_key == key))
        }
    }

    #[async_trait::async_trait]
    impl ApiStore for FakeStore {
        type Error = String;

        async fn job_definition_exists(&self, id: &JobDefinitionId) -> Result<bool, Self::Error> {
            Ok(self
                .known_definitions
                .lock()
                .map_err(|error| error.to_string())?
                .iter()
                .any(|known| known == id.as_str()))
        }

        async fn save_run(&self, run: &JobRun) -> Result<(), Self::Error> {
            self.runs
                .lock()
                .map_err(|error| error.to_string())?
                .push(run.clone());
            Ok(())
        }

        async fn upsert_job_definition(
            &self,
            definition: &JobDefinition,
        ) -> Result<(), Self::Error> {
            let mut definitions = self
                .known_definitions
                .lock()
                .map_err(|error| error.to_string())?;
            if !definitions
                .iter()
                .any(|known| known == definition.id.as_str())
            {
                definitions.push(definition.id.as_str().to_string());
            }
            Ok(())
        }

        async fn list_job_definitions(
            &self,
            limit: i64,
        ) -> Result<Vec<JobDefinition>, Self::Error> {
            let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
            Ok(self
                .known_definitions
                .lock()
                .map_err(|error| error.to_string())?
                .iter()
                .take(limit)
                .map(|id| {
                    JobDefinition::new(
                        JobDefinitionId::new(id.clone()).expect("fake definition id"),
                        id.clone(),
                        "python:3.12-slim",
                        vec![
                            "python".to_string(),
                            "-c".to_string(),
                            "print('fake')".to_string(),
                        ],
                        format!("bundles/{id}.py"),
                        "{}",
                        capsulet_core::RetryPolicy::no_retry(),
                    )
                    .expect("fake definition")
                })
                .collect())
        }

        async fn find_job_definition(
            &self,
            id: &JobDefinitionId,
        ) -> Result<Option<JobDefinition>, Self::Error> {
            Ok(self
                .known_definitions
                .lock()
                .map_err(|error| error.to_string())?
                .iter()
                .find(|known| *known == id.as_str())
                .map(|known| {
                    JobDefinition::new(
                        id.clone(),
                        known.clone(),
                        "python:3.12-slim",
                        vec![
                            "python".to_string(),
                            "-c".to_string(),
                            "print('fake')".to_string(),
                        ],
                        format!("bundles/{known}.py"),
                        "{}",
                        capsulet_core::RetryPolicy::no_retry(),
                    )
                    .expect("fake definition")
                }))
        }

        async fn delete_job_definition(&self, id: &JobDefinitionId) -> Result<bool, Self::Error> {
            let mut definitions = self
                .known_definitions
                .lock()
                .map_err(|error| error.to_string())?;
            let initial_len = definitions.len();
            definitions.retain(|known| known != id.as_str());
            Ok(definitions.len() != initial_len)
        }

        async fn upsert_workflow(&self, workflow: &WorkflowDefinition) -> Result<(), Self::Error> {
            let mut workflows = self.workflows.lock().map_err(|error| error.to_string())?;
            workflows.retain(|existing| existing.id != workflow.id);
            workflows.push(workflow.clone());
            Ok(())
        }

        async fn list_workflows(&self, limit: i64) -> Result<Vec<WorkflowDefinition>, Self::Error> {
            let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
            Ok(self
                .workflows
                .lock()
                .map_err(|error| error.to_string())?
                .iter()
                .take(limit)
                .cloned()
                .collect())
        }

        async fn find_workflow(
            &self,
            id: &WorkflowId,
        ) -> Result<Option<WorkflowDefinition>, Self::Error> {
            Ok(self
                .workflows
                .lock()
                .map_err(|error| error.to_string())?
                .iter()
                .find(|workflow| workflow.id == *id)
                .cloned())
        }

        async fn upsert_automation(&self, automation: &Automation) -> Result<(), Self::Error> {
            let mut automations = self.automations.lock().map_err(|error| error.to_string())?;
            automations.retain(|existing| existing.id != automation.id);
            automations.push(automation.clone());
            Ok(())
        }

        async fn list_automations(&self, limit: i64) -> Result<Vec<Automation>, Self::Error> {
            let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
            Ok(self
                .automations
                .lock()
                .map_err(|error| error.to_string())?
                .iter()
                .take(limit)
                .cloned()
                .collect())
        }

        async fn find_automation(
            &self,
            id: &AutomationId,
        ) -> Result<Option<Automation>, Self::Error> {
            Ok(self
                .automations
                .lock()
                .map_err(|error| error.to_string())?
                .iter()
                .find(|automation| automation.id == *id)
                .cloned())
        }

        async fn create_workflow_run(
            &self,
            workflow_id: &WorkflowId,
            automation_id: Option<&AutomationId>,
            run_id: &WorkflowRunId,
        ) -> Result<WorkflowRun, Self::Error> {
            let run = WorkflowRun {
                id: run_id.clone(),
                workflow_id: workflow_id.clone(),
                automation_id: automation_id.cloned(),
                status: capsulet_core::WorkflowRunStatus::Queued,
                current_step_position: 0,
            };
            self.workflow_runs
                .lock()
                .map_err(|error| error.to_string())?
                .push(run.clone());
            Ok(run)
        }

        async fn list_workflow_runs(&self, limit: i64) -> Result<Vec<WorkflowRun>, Self::Error> {
            let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
            Ok(self
                .workflow_runs
                .lock()
                .map_err(|error| error.to_string())?
                .iter()
                .take(limit)
                .cloned()
                .collect())
        }

        async fn list_workflow_step_runs(
            &self,
            _workflow_run_id: &WorkflowRunId,
        ) -> Result<Vec<WorkflowStepRun>, Self::Error> {
            Ok(Vec::new())
        }

        async fn list_runs(&self, limit: i64) -> Result<Vec<JobRun>, Self::Error> {
            let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
            Ok(self
                .runs
                .lock()
                .map_err(|error| error.to_string())?
                .iter()
                .take(limit)
                .cloned()
                .collect())
        }

        async fn find_run(&self, id: &JobRunId) -> Result<Option<JobRun>, Self::Error> {
            Ok(self
                .runs
                .lock()
                .map_err(|error| error.to_string())?
                .iter()
                .find(|run| run.id == *id)
                .cloned())
        }

        async fn find_run_log(&self, id: &JobRunId) -> Result<Option<JobRunLog>, Self::Error> {
            Ok(self
                .logs
                .lock()
                .map_err(|error| error.to_string())?
                .iter()
                .find(|log| log.run_id == *id)
                .cloned())
        }

        async fn cancel_run(&self, id: &JobRunId) -> Result<Option<JobRun>, Self::Error> {
            let mut runs = self.runs.lock().map_err(|error| error.to_string())?;
            let Some(run) = runs.iter_mut().rev().find(|run| run.id == *id) else {
                return Ok(None);
            };
            if !run.status.is_terminal() {
                run.status = JobRunStatus::Cancelled;
            }
            Ok(Some(run.clone()))
        }

        async fn list_artifacts(&self, id: &JobRunId) -> Result<Vec<JobArtifact>, Self::Error> {
            Ok(self
                .artifacts
                .lock()
                .map_err(|error| error.to_string())?
                .iter()
                .filter(|artifact| artifact.run_id == *id)
                .cloned()
                .collect())
        }

        async fn find_artifact(
            &self,
            run_id: &JobRunId,
            artifact_id: &ArtifactId,
        ) -> Result<Option<JobArtifact>, Self::Error> {
            Ok(self
                .artifacts
                .lock()
                .map_err(|error| error.to_string())?
                .iter()
                .find(|artifact| artifact.run_id == *run_id && artifact.id == *artifact_id)
                .cloned())
        }

        async fn save_artifact(&self, artifact: &JobArtifact) -> Result<(), Self::Error> {
            self.artifacts
                .lock()
                .map_err(|error| error.to_string())?
                .push(artifact.clone());
            Ok(())
        }
    }

    impl FakeStore {
        fn with_definition(id: &str) -> Self {
            let store = Self::default();
            store
                .known_definitions
                .lock()
                .expect("definition mutex")
                .push(id.to_string());
            store
        }

        fn with_run(self, run: JobRun) -> Self {
            self.runs.lock().expect("runs mutex").push(run);
            self
        }

        fn with_log(self, log: JobRunLog) -> Self {
            self.logs.lock().expect("logs mutex").push(log);
            self
        }

        fn with_artifact(self, artifact: JobArtifact) -> Self {
            self.artifacts
                .lock()
                .expect("artifacts mutex")
                .push(artifact);
            self
        }
    }

    fn test_app(store: FakeStore) -> axum::Router {
        let object_store = FakeObjectStore::default();
        object_store.objects.lock().expect("objects mutex").push((
            "artifacts/run_with_artifact/report.txt".to_string(),
            b"report".to_vec(),
        ));
        router(AppState::new(
            store,
            object_store,
            ["mini".to_string(), "large".to_string()],
        ))
    }

    async fn response_json(response: axum::response::Response) -> Value {
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("collect response")
            .to_bytes();
        serde_json::from_slice(&bytes).expect("json response")
    }

    #[tokio::test]
    async fn healthz_returns_ok() {
        let response = test_app(FakeStore::default())
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(response_json(response).await, json!({ "status": "ok" }));
    }

    #[tokio::test]
    async fn lists_configured_execution_pools() {
        let response = test_app(FakeStore::default())
            .oneshot(
                Request::builder()
                    .uri("/v1/execution-pools")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(
            response_json(response).await,
            json!({
                "execution_pools": [
                    { "name": "mini", "description": "Default execution pool", "is_default": true },
                    { "name": "large", "description": "Configured execution pool", "is_default": false }
                ]
            })
        );
    }

    #[tokio::test]
    async fn creates_and_reads_reusable_python_job_definition() {
        let app = test_app(FakeStore::default());
        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/job-definitions")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "id": "job_daily_report",
                            "name": "Daily report",
                            "runtime_image": "python:3.12-slim",
                            "python_script": "print('daily report')",
                            "retry_max_attempts": 2,
                            "retry_delay_seconds": 5
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(create_response.status(), axum::http::StatusCode::CREATED);
        let body = response_json(create_response).await;
        assert_eq!(body["id"], "job_daily_report");
        assert_eq!(body["name"], "Daily report");
        assert_eq!(
            body["bundle_object_key"],
            "bundles/job-definitions/job_daily_report/main.py"
        );

        let list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/job-definitions")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(list_response.status(), axum::http::StatusCode::OK);
        assert_eq!(
            response_json(list_response).await["job_definitions"][0]["id"],
            "job_daily_report"
        );

        let fetch_response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/job-definitions/job_daily_report")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(fetch_response.status(), axum::http::StatusCode::OK);
        assert_eq!(
            response_json(fetch_response).await["id"],
            "job_daily_report"
        );
    }

    #[tokio::test]
    async fn creates_manual_run() {
        let response = test_app(FakeStore::with_definition("job_hello_python"))
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/jobs/runs")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "run_id": "run_api_test",
                            "job_definition_id": "job_hello_python",
                            "execution_pool": "mini"
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), axum::http::StatusCode::CREATED);
        assert_eq!(
            response_json(response).await,
            json!({
                "id": "run_api_test",
                "job_definition_id": "job_hello_python",
                "status": "queued",
                "execution_pool": "mini",
                "attempt_count": 0
            })
        );
    }

    #[tokio::test]
    async fn creates_script_backed_run() {
        let response = test_app(FakeStore::default())
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/jobs/runs")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "run_id": "run_script_test",
                            "job_definition_id": "script",
                            "execution_pool": "mini",
                            "python_script": "print('from script')"
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), axum::http::StatusCode::CREATED);
        let body = response_json(response).await;
        assert_eq!(body["id"], "run_script_test");
        assert_eq!(body["job_definition_id"], "job_definition_run_script_test");
    }

    #[tokio::test]
    async fn rejects_unknown_job_definition() {
        let response = test_app(FakeStore::default())
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/jobs/runs")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "job_definition_id": "missing",
                            "execution_pool": "mini"
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(
            response.status(),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            response_json(response).await["code"],
            json!("unknown_job_definition")
        );
    }

    #[tokio::test]
    async fn rejects_unknown_execution_pool() {
        let response = test_app(FakeStore::with_definition("job_hello_python"))
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/jobs/runs")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "job_definition_id": "job_hello_python",
                            "execution_pool": "gpu"
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(
            response.status(),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            response_json(response).await["code"],
            json!("unknown_execution_pool")
        );
    }

    #[tokio::test]
    async fn lists_and_fetches_runs() {
        let run = JobRun::new(
            JobRunId::new("run_listed").expect("valid run id"),
            JobDefinitionId::new("job_hello_python").expect("valid definition id"),
            ExecutionPoolName::new("mini").expect("valid pool"),
        );
        let app = test_app(FakeStore::with_definition("job_hello_python").with_run(run));

        let list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/jobs/runs")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(list_response.status(), axum::http::StatusCode::OK);
        assert_eq!(
            response_json(list_response).await["runs"][0]["id"],
            "run_listed"
        );

        let get_response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/jobs/runs/run_listed")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(get_response.status(), axum::http::StatusCode::OK);
        assert_eq!(response_json(get_response).await["id"], "run_listed");
    }

    #[tokio::test]
    async fn cancels_run() {
        let run = JobRun::new(
            JobRunId::new("run_cancelled").expect("valid run id"),
            JobDefinitionId::new("job_hello_python").expect("valid definition id"),
            ExecutionPoolName::new("mini").expect("valid pool"),
        );
        let response = test_app(FakeStore::with_definition("job_hello_python").with_run(run))
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/jobs/runs/run_cancelled/cancel")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(response_json(response).await["status"], "cancelled");
    }

    #[tokio::test]
    async fn returns_not_found_for_missing_run() {
        let response = test_app(FakeStore::default())
            .oneshot(
                Request::builder()
                    .uri("/v1/jobs/runs/run_missing")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
        assert_eq!(
            response_json(response).await["code"],
            json!("job_run_not_found")
        );
    }

    #[tokio::test]
    async fn fetches_run_logs() {
        let run = JobRun::new(
            JobRunId::new("run_with_logs").expect("valid run id"),
            JobDefinitionId::new("job_hello_python").expect("valid definition id"),
            ExecutionPoolName::new("mini").expect("valid pool"),
        );
        let log = JobRunLog::new(run.id.clone(), "hello from logs\n").expect("valid log");
        let response = test_app(
            FakeStore::with_definition("job_hello_python")
                .with_run(run)
                .with_log(log),
        )
        .oneshot(
            Request::builder()
                .uri("/v1/jobs/runs/run_with_logs/logs")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(
            response_json(response).await,
            json!({
                "run_id": "run_with_logs",
                "logs": "hello from logs\n",
                "object_log_available": false
            })
        );
    }

    #[tokio::test]
    async fn returns_not_found_for_missing_run_logs() {
        let run = JobRun::new(
            JobRunId::new("run_without_logs").expect("valid run id"),
            JobDefinitionId::new("job_hello_python").expect("valid definition id"),
            ExecutionPoolName::new("mini").expect("valid pool"),
        );
        let response = test_app(FakeStore::with_definition("job_hello_python").with_run(run))
            .oneshot(
                Request::builder()
                    .uri("/v1/jobs/runs/run_without_logs/logs")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
        assert_eq!(
            response_json(response).await["code"],
            json!("job_run_logs_not_found")
        );
    }

    #[tokio::test]
    async fn lists_artifacts() {
        let run = JobRun::new(
            JobRunId::new("run_with_artifact").expect("valid run id"),
            JobDefinitionId::new("job_hello_python").expect("valid definition id"),
            ExecutionPoolName::new("mini").expect("valid pool"),
        );
        let artifact = JobArtifact::new(
            ArtifactId::new("artifact_1").expect("artifact id"),
            run.id.clone(),
            None,
            "report.txt",
            "artifacts/run_with_artifact/report.txt",
            "text/plain",
            6,
            None,
            ArtifactObjectKind::Artifact,
        )
        .expect("artifact");
        let response = test_app(
            FakeStore::with_definition("job_hello_python")
                .with_run(run)
                .with_artifact(artifact),
        )
        .oneshot(
            Request::builder()
                .uri("/v1/jobs/runs/run_with_artifact/artifacts")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(
            response_json(response).await["artifacts"][0]["name"],
            "report.txt"
        );
    }

    #[tokio::test]
    async fn downloads_artifact() {
        let run = JobRun::new(
            JobRunId::new("run_with_artifact").expect("valid run id"),
            JobDefinitionId::new("job_hello_python").expect("valid definition id"),
            ExecutionPoolName::new("mini").expect("valid pool"),
        );
        let artifact = JobArtifact::new(
            ArtifactId::new("artifact_1").expect("artifact id"),
            run.id.clone(),
            None,
            "report.txt",
            "artifacts/run_with_artifact/report.txt",
            "text/plain",
            6,
            None,
            ArtifactObjectKind::Artifact,
        )
        .expect("artifact");
        let response = test_app(
            FakeStore::with_definition("job_hello_python")
                .with_run(run)
                .with_artifact(artifact),
        )
        .oneshot(
            Request::builder()
                .uri("/v1/jobs/runs/run_with_artifact/artifacts/artifact_1")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("collect artifact")
            .to_bytes();
        assert_eq!(&bytes[..], b"report");
    }

    #[tokio::test]
    async fn response_body_helper_handles_empty_body() {
        let bytes = to_bytes(Body::empty(), usize::MAX)
            .await
            .expect("empty body");
        assert!(bytes.is_empty());
    }
}
