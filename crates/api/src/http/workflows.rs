//! Workflow definitions, their topology and limits, and workflow runs with their logs.

use std::{
    collections::{BTreeMap, BTreeSet},
    convert::Infallible,
    time::Duration,
};

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use capsulet_core::{
    ArtifactObjectKind, AutomationId, ExecutionPoolName, JobDefinitionId, WorkflowDefinition,
    WorkflowDependencyPolicy, WorkflowGraph, WorkflowId, WorkflowRun, WorkflowRunId,
    WorkflowRunStatus, WorkflowStatus, WorkflowStep, WorkflowStepDependency, WorkflowStepId,
};
use capsulet_observability::{self as observability};
use capsulet_storage::ObjectStore;

use super::admission::enforce_workflow_admission;
use super::common::{generated_id, is_desc, matches_date_range, query_contains};
use super::identity::{
    ProjectContext, project_context, require_project_role, require_resource_project,
};
use crate::{
    auth::Principal,
    error::ApiError,
    models::{
        CreateWorkflowRequest, ListWorkflowRunsQuery, ListWorkflowRunsResponse,
        ListWorkflowsResponse, TopologyEdgeResponse, TopologyNodeResponse, TopologyResponse,
        WorkflowEditabilityResponse, WorkflowResponse, WorkflowRunLogEntryResponse,
        WorkflowRunLogsResponse, WorkflowRunResponse,
    },
    state::AppState,
    store::ApiStore,
};

pub(super) const MAX_WORKFLOW_STEPS: usize = 256;

pub(super) const MAX_WORKFLOW_DEPENDENCIES: usize = 1_024;

pub(super) const MAX_WORKFLOW_FAN_IN: usize = 64;

pub(super) const MAX_WORKFLOW_FAN_OUT: usize = 64;

pub(super) const MAX_WORKFLOW_DEPTH: usize = 64;

pub(super) async fn create_workflow<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateWorkflowRequest>,
) -> Result<(StatusCode, Json<WorkflowResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, crate::ProjectRole::Operator)?;
    let workflow = build_workflow(&state, &context, request).await?;
    // A foreign id is refused by the scoped upsert, atomically.
    require_workflow_step_projects(&state.store, &workflow, &context).await?;
    state
        .store
        .upsert_workflow(&workflow, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::scoped_store::<S>)?;

    Ok((StatusCode::CREATED, Json(WorkflowResponse::from(&workflow))))
}

pub(super) async fn update_workflow<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
    Json(mut request): Json<CreateWorkflowRequest>,
) -> Result<Json<WorkflowResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, crate::ProjectRole::Operator)?;
    let workflow_id = WorkflowId::new(id.clone()).map_err(ApiError::validation)?;
    require_resource_project(&state.store, "workflows", workflow_id.as_str(), &context).await?;
    if state
        .store
        .workflow_has_active_runs(&workflow_id)
        .await
        .map_err(ApiError::store)?
    {
        return Err(ApiError::WorkflowLocked(id));
    }
    request.id = Some(id);
    let workflow = build_workflow(&state, &context, request).await?;
    require_workflow_step_projects(&state.store, &workflow, &context).await?;
    state
        .store
        .upsert_workflow(&workflow, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::scoped_store::<S>)?;
    Ok(Json(WorkflowResponse::from(&workflow)))
}

pub(super) async fn list_workflows<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListWorkflowsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let workflows = state
        .store
        .list_workflows(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    // Scoped in the query, before the limit.
    let scoped = workflows;
    Ok(Json(ListWorkflowsResponse {
        workflows: scoped.iter().map(WorkflowResponse::from).collect(),
    }))
}

pub(super) async fn get_workflow<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = WorkflowId::new(id).map_err(ApiError::validation)?;
    require_resource_project(&state.store, "workflows", id.as_str(), &context).await?;
    let Some(workflow) = state
        .store
        .find_workflow(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::WorkflowNotFound(id.as_str().to_string()));
    };

    Ok(Json(WorkflowResponse::from(&workflow)))
}

pub(super) async fn delete_workflow<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, crate::ProjectRole::Admin)?;
    let workflow_id = WorkflowId::new(id.clone()).map_err(ApiError::validation)?;
    require_resource_project(&state.store, "workflows", workflow_id.as_str(), &context).await?;
    if state
        .store
        .find_workflow(&workflow_id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
        .is_none()
    {
        return Err(ApiError::WorkflowNotFound(id));
    }
    if state
        .store
        .workflow_has_active_runs(&workflow_id)
        .await
        .map_err(ApiError::store)?
    {
        return Err(ApiError::WorkflowLocked(workflow_id.as_str().to_string()));
    }
    if state
        .store
        .list_automations(&context.tenant_id, &context.project_id, 500)
        .await
        .map_err(ApiError::store)?
        .iter()
        .any(|automation| automation.workflow_id() == &workflow_id)
    {
        return Err(ApiError::WorkflowLocked(workflow_id.as_str().to_string()));
    }
    let deleted = state
        .store
        .delete_workflow(&workflow_id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?;
    if !deleted {
        return Err(ApiError::WorkflowNotFound(workflow_id.as_str().to_string()));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn get_workflow_editability<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowEditabilityResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = WorkflowId::new(id).map_err(ApiError::validation)?;
    require_resource_project(&state.store, "workflows", id.as_str(), &context).await?;
    if state
        .store
        .find_workflow(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
        .is_none()
    {
        return Err(ApiError::WorkflowNotFound(id.as_str().to_string()));
    }
    let locked = state
        .store
        .workflow_has_active_runs(&id)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(WorkflowEditabilityResponse {
        editable: !locked,
        reason: locked.then(|| "Queued or running executions are using this workflow.".to_string()),
    }))
}

pub(super) async fn get_topology<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<TopologyResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let workflows = state
        .store
        .list_workflows(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    let scoped_workflows = workflows;
    let scoped_automations = state
        .store
        .list_automations(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for pool in state.execution_pools.iter() {
        nodes.push(TopologyNodeResponse {
            id: format!("pool:{pool}"),
            label: pool.clone(),
            kind: "pool".to_string(),
            status: "available".to_string(),
        });
    }
    for workflow in &scoped_workflows {
        nodes.push(TopologyNodeResponse {
            id: format!("workflow:{}", workflow.id()),
            label: workflow.name().to_string(),
            kind: "workflow".to_string(),
            status: workflow.status().to_string(),
        });
        let pools: BTreeSet<_> = workflow
            .steps()
            .iter()
            .map(|step| step.execution_pool().as_str())
            .collect();
        edges.extend(pools.into_iter().map(|pool| TopologyEdgeResponse {
            from: format!("workflow:{}", workflow.id()),
            to: format!("pool:{pool}"),
            label: "executes on".to_string(),
        }));
    }
    for automation in &scoped_automations {
        nodes.push(TopologyNodeResponse {
            id: format!("automation:{}", automation.id()),
            label: automation.name().to_string(),
            kind: "automation".to_string(),
            status: automation.status().to_string(),
        });
        edges.push(TopologyEdgeResponse {
            from: format!("automation:{}", automation.id()),
            to: format!("workflow:{}", automation.workflow_id()),
            label: "triggers".to_string(),
        });
    }
    Ok(Json(TopologyResponse { nodes, edges }))
}

pub(super) async fn require_workflow_step_projects<S: ApiStore>(
    store: &S,
    workflow: &WorkflowDefinition,
    context: &ProjectContext,
) -> Result<(), ApiError> {
    for step in workflow.steps() {
        if store
            .find_job_definition(
                step.job_definition_id(),
                &context.tenant_id,
                &context.project_id,
            )
            .await
            .map_err(ApiError::store)?
            .is_none()
        {
            return Err(ApiError::ResourceNotFound);
        }
    }
    Ok(())
}

pub(super) async fn build_workflow<S, O>(
    state: &AppState<S, O>,
    context: &ProjectContext,
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
    if request.steps.len() > MAX_WORKFLOW_STEPS {
        return Err(ApiError::Validation(format!(
            "workflow has {} steps, maximum is {MAX_WORKFLOW_STEPS}",
            request.steps.len()
        )));
    }
    let workflow_id = WorkflowId::new(request.id.unwrap_or_else(|| generated_id("workflow")))
        .map_err(ApiError::validation)?;
    let mut steps = Vec::with_capacity(request.steps.len());
    for (index, step) in request.steps.into_iter().enumerate() {
        steps.push(build_workflow_step(state, context, &workflow_id, index, step).await?);
    }
    let dependencies = workflow_dependencies(request.dependencies, &steps)?;
    if dependencies.len() > MAX_WORKFLOW_DEPENDENCIES {
        return Err(ApiError::Validation(format!(
            "workflow has {} dependencies, maximum is {MAX_WORKFLOW_DEPENDENCIES}",
            dependencies.len()
        )));
    }
    let graph = WorkflowGraph::new(&workflow_id, &steps, &dependencies)
        .map_err(|error| ApiError::validation(error.to_string()))?;
    validate_workflow_graph_limits(&graph, &dependencies)?;

    let deadline_seconds = match request.deadline_seconds {
        Some(0) => {
            return Err(ApiError::Validation(
                "workflow deadline_seconds must be greater than zero".to_string(),
            ));
        }
        value => value,
    };

    Ok(WorkflowDefinition::with_dependencies(
        workflow_id,
        request.name,
        request.description.unwrap_or_default(),
        WorkflowStatus::Enabled,
        steps,
        dependencies,
    )
    .with_deadline_seconds(deadline_seconds))
}

/// The `index`th step of a workflow, refused unless its job definition is in
/// the caller's project and its execution pool is one this installation has.
pub(super) async fn build_workflow_step<S, O>(
    state: &AppState<S, O>,
    context: &ProjectContext,
    workflow_id: &WorkflowId,
    index: usize,
    step: crate::models::CreateWorkflowStepRequest,
) -> Result<WorkflowStep, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let job_definition_id =
        JobDefinitionId::new(step.job_definition_id).map_err(ApiError::validation)?;
    if state
        .store
        .find_job_definition(&job_definition_id, &context.tenant_id, &context.project_id)
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
    let timeout_seconds = match step.timeout_seconds {
        Some(0) => {
            return Err(ApiError::Validation(
                "workflow step timeout_seconds must be greater than zero".to_string(),
            ));
        }
        value => value,
    };
    Ok(WorkflowStep::new(
        WorkflowStepId::new(
            step.id
                .unwrap_or_else(|| format!("{}_step_{position}", workflow_id.as_str())),
        )
        .map_err(ApiError::validation)?,
        workflow_id.clone(),
        position,
        step.name,
        job_definition_id,
        execution_pool,
    )
    .with_timeout_seconds(timeout_seconds))
}

/// The dependencies a workflow declares, or, when it declares none, each step
/// depending on the one before it.
pub(super) fn workflow_dependencies(
    requests: Option<Vec<crate::models::CreateWorkflowDependencyRequest>>,
    steps: &[WorkflowStep],
) -> Result<Vec<WorkflowStepDependency>, ApiError> {
    let Some(requests) = requests else {
        return Ok(steps
            .windows(2)
            .map(|pair| WorkflowStepDependency::new(pair[0].id().clone(), pair[1].id().clone()))
            .collect());
    };
    requests
        .into_iter()
        .map(|dependency| {
            let policy = match dependency.policy.as_deref().unwrap_or("hard") {
                "hard" => WorkflowDependencyPolicy::Hard,
                "soft" => WorkflowDependencyPolicy::Soft,
                "always" => WorkflowDependencyPolicy::Always,
                value => {
                    return Err(ApiError::Validation(format!(
                        "unsupported workflow dependency policy {value}; expected hard, soft, or always"
                    )));
                }
            };
            Ok(WorkflowStepDependency::with_policy(
                WorkflowStepId::new(dependency.from_step_id).map_err(ApiError::validation)?,
                WorkflowStepId::new(dependency.to_step_id).map_err(ApiError::validation)?,
                policy,
            ))
        })
        .collect()
}

pub(super) fn validate_workflow_graph_limits(
    graph: &WorkflowGraph,
    dependencies: &[WorkflowStepDependency],
) -> Result<(), ApiError> {
    let mut fan_in = BTreeMap::<WorkflowStepId, usize>::new();
    let mut fan_out = BTreeMap::<WorkflowStepId, usize>::new();
    for dependency in dependencies {
        let out_count = fan_out
            .entry(dependency.from_step_id().clone())
            .and_modify(|count| *count += 1)
            .or_insert(1);
        if *out_count > MAX_WORKFLOW_FAN_OUT {
            return Err(ApiError::Validation(format!(
                "workflow step {} has fan-out {}, maximum is {MAX_WORKFLOW_FAN_OUT}",
                dependency.from_step_id(),
                out_count
            )));
        }
        let in_count = fan_in
            .entry(dependency.to_step_id().clone())
            .and_modify(|count| *count += 1)
            .or_insert(1);
        if *in_count > MAX_WORKFLOW_FAN_IN {
            return Err(ApiError::Validation(format!(
                "workflow step {} has fan-in {}, maximum is {MAX_WORKFLOW_FAN_IN}",
                dependency.to_step_id(),
                in_count
            )));
        }
    }

    let mut depths = BTreeMap::<WorkflowStepId, usize>::new();
    for step_id in graph.topological_order() {
        let depth = *depths.entry(step_id.clone()).or_insert(1);
        if depth > MAX_WORKFLOW_DEPTH {
            return Err(ApiError::Validation(format!(
                "workflow depth is {depth}, maximum is {MAX_WORKFLOW_DEPTH}"
            )));
        }
        if let Some(children) = graph.outgoing(step_id) {
            for child in children {
                depths
                    .entry(child.clone())
                    .and_modify(|child_depth| *child_depth = (*child_depth).max(depth + 1))
                    .or_insert(depth + 1);
            }
        }
    }

    Ok(())
}

pub(super) fn filter_workflow_runs(
    mut runs: Vec<WorkflowRun>,
    query: &ListWorkflowRunsQuery,
) -> Vec<WorkflowRun> {
    runs.retain(|run| {
        let matches_state = query
            .state
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none_or(|state| run.status().to_string().eq_ignore_ascii_case(state));
        let searchable = format!(
            "{} {} {}",
            run.id().as_str(),
            run.workflow_id().as_str(),
            run.automation_id()
                .map_or("", capsulet_core::AutomationId::as_str)
        );
        matches_state
            && query_contains(&searchable, query.q.as_deref())
            && matches_date_range(
                run.created_at(),
                query.start_at.as_deref(),
                query.end_at.as_deref(),
            )
    });
    let desc = is_desc(query.direction.as_deref());
    match query.sort.as_deref().unwrap_or("created_at") {
        "workflow_run" | "id" => {
            runs.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str()));
        }
        "workflow" | "name" => {
            runs.sort_by(|left, right| {
                left.workflow_id()
                    .as_str()
                    .cmp(right.workflow_id().as_str())
            });
        }
        "state" | "status" => {
            runs.sort_by_key(|run| run.status().to_string());
        }
        "automation" => runs.sort_by(|left, right| {
            left.automation_id()
                .map_or("", capsulet_core::AutomationId::as_str)
                .cmp(
                    right
                        .automation_id()
                        .map_or("", capsulet_core::AutomationId::as_str),
                )
        }),
        _ => runs.sort_by(|left, right| left.created_at().cmp(right.created_at())),
    }
    if desc {
        runs.reverse();
    }
    runs
}

pub(super) async fn trigger_automation<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<WorkflowRunResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, crate::ProjectRole::Operator)?;
    let automation_id = AutomationId::new(id).map_err(ApiError::validation)?;
    require_resource_project(
        &state.store,
        "automations",
        automation_id.as_str(),
        &context,
    )
    .await?;
    let Some(automation) = state
        .store
        .find_automation(&automation_id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::AutomationNotFound(
            automation_id.as_str().to_string(),
        ));
    };
    enforce_workflow_admission(&state, "manual_trigger").await?;
    let run_id = WorkflowRunId::new(generated_id("workflow_run")).map_err(ApiError::validation)?;
    let run = state
        .store
        .create_workflow_run(
            automation.workflow_id(),
            Some(automation.id()),
            &run_id,
            automation.job_input_json(),
            &context.tenant_id,
            &context.project_id,
        )
        .await
        .map_err(ApiError::scoped_store::<S>)?;
    Ok((
        StatusCode::CREATED,
        Json(WorkflowRunResponse::new(&run, &[])),
    ))
}

pub(super) async fn list_workflow_runs<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Query(query): Query<ListWorkflowRunsQuery>,
) -> Result<Json<ListWorkflowRunsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let limit = query
        .limit
        .unwrap_or(crate::models::ListWorkflowRunsQuery::DEFAULT_LIMIT)
        .clamp(1, crate::models::LIST_LIMIT_MAX);
    let runs = state
        .store
        .list_workflow_runs(&context.tenant_id, &context.project_id, i64::from(limit))
        .await
        .map_err(ApiError::store)?;
    let runs = filter_workflow_runs(runs, &query);
    let mut workflow_runs = Vec::with_capacity(runs.len());
    for run in runs {
        let step_runs = state
            .store
            .list_workflow_step_runs(run.id(), &context.tenant_id, &context.project_id)
            .await
            .map_err(ApiError::store)?;
        workflow_runs.push(WorkflowRunResponse::new(&run, &step_runs));
    }
    Ok(Json(ListWorkflowRunsResponse { workflow_runs }))
}

pub(super) async fn get_workflow_run<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowRunResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = WorkflowRunId::new(id).map_err(ApiError::validation)?;
    let Some(run) = state
        .store
        .find_workflow_run(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::WorkflowRunNotFound(id.as_str().to_string()));
    };
    let step_runs = state
        .store
        .list_workflow_step_runs(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(WorkflowRunResponse::new(&run, &step_runs)))
}

pub(super) async fn get_workflow_run_logs<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowRunLogsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = WorkflowRunId::new(id).map_err(ApiError::validation)?;
    Ok(Json(load_workflow_run_logs(&state, &id, &context).await?))
}

pub(super) async fn stream_workflow_run_logs<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Response, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = WorkflowRunId::new(id).map_err(ApiError::validation)?;
    if state
        .store
        .find_workflow_run(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
        .is_none()
    {
        return Err(ApiError::WorkflowRunNotFound(id.as_str().to_string()));
    }
    let stream_state = state.clone();
    let stream_context = context.clone();
    let stream = async_stream::stream! {
        loop {
            match load_workflow_run_logs(&stream_state, &id, &stream_context).await {
                Ok(snapshot) => {
                    let terminal = snapshot.status == "succeeded"
                        || snapshot.status == "failed"
                        || snapshot.status == "cancelled"
                        || snapshot.status == "timed_out";
                    match serde_json::to_string(&snapshot) {
                        Ok(data) => yield Ok::<_, Infallible>(Event::default().event("snapshot").data(data)),
                        Err(error) => {
                            observability::tracing::warn!(%error, workflow_run_id = %id.as_str(), "failed to serialize workflow log snapshot");
                            break;
                        }
                    }
                    if terminal {
                        yield Ok(Event::default().event("done").data(snapshot.status));
                        break;
                    }
                }
                Err(error) => {
                    yield Ok(Event::default().event("error").data(error.to_string()));
                    break;
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    };
    Ok(Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response())
}

pub(super) async fn load_workflow_run_logs<S, O>(
    state: &AppState<S, O>,
    id: &WorkflowRunId,
    context: &ProjectContext,
) -> Result<WorkflowRunLogsResponse, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let Some(run) = state
        .store
        .find_workflow_run(id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::WorkflowRunNotFound(id.as_str().to_string()));
    };

    let step_runs = state
        .store
        .list_workflow_step_runs(id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?;
    let mut entries = Vec::with_capacity(step_runs.len());
    for step_run in step_runs {
        let (log, object_log_available) = if let Some(job_run_id) = step_run.maybe_job_run_id() {
            let log = state
                .store
                .find_run_log(job_run_id, &context.tenant_id, &context.project_id)
                .await
                .map_err(ApiError::store)?;
            let object_log_available = state
                .store
                .list_artifacts(job_run_id, &context.tenant_id, &context.project_id)
                .await
                .map_err(ApiError::store)?
                .iter()
                .any(|artifact| artifact.kind() == ArtifactObjectKind::Log);
            (log, object_log_available)
        } else {
            (None, false)
        };

        entries.push(WorkflowRunLogEntryResponse {
            step_run_id: step_run.id().as_str().to_string(),
            workflow_step_id: step_run.workflow_step_id().as_str().to_string(),
            job_run_id: step_run
                .maybe_job_run_id()
                .map(|job_run_id| job_run_id.as_str().to_string()),
            position: step_run.position(),
            status: step_run.status().to_string(),
            logs: log.map_or_else(String::new, |log| log.text),
            object_log_available,
        });
    }

    Ok(WorkflowRunLogsResponse {
        workflow_run_id: run.id().as_str().to_string(),
        workflow_id: run.workflow_id().as_str().to_string(),
        status: run.status().to_string(),
        entries,
    })
}

pub(super) async fn remove_workflow_run<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowRunResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, crate::ProjectRole::Operator)?;
    let id = WorkflowRunId::new(id).map_err(ApiError::validation)?;
    let Some(run) = state
        .store
        .remove_queued_workflow_run(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::WorkflowRunNotFound(id.as_str().to_string()));
    };
    if run.status() != WorkflowRunStatus::Removed {
        return Err(ApiError::InvalidWorkflowRunTransition(format!(
            "workflow run {} can only be removed while queued and before any node executes",
            id.as_str()
        )));
    }
    let step_runs = state
        .store
        .list_workflow_step_runs(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(WorkflowRunResponse::new(&run, &step_runs)))
}

pub(super) async fn cancel_workflow_run<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowRunResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, crate::ProjectRole::Operator)?;
    let id = WorkflowRunId::new(id).map_err(ApiError::validation)?;
    let Some(run) = state
        .store
        .cancel_running_workflow_run(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::WorkflowRunNotFound(id.as_str().to_string()));
    };
    if run.status() != WorkflowRunStatus::Cancelled {
        return Err(ApiError::InvalidWorkflowRunTransition(format!(
            "workflow run {} can only be cancelled while running",
            id.as_str()
        )));
    }
    let step_runs = state
        .store
        .list_workflow_step_runs(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(WorkflowRunResponse::new(&run, &step_runs)))
}

pub(super) async fn resume_workflow_run<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowRunResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, crate::ProjectRole::Operator)?;
    let id = WorkflowRunId::new(id).map_err(ApiError::validation)?;
    let Some(run) = state
        .store
        .resume_workflow_run(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::WorkflowRunNotFound(id.as_str().to_string()));
    };
    if run.status() != WorkflowRunStatus::Running {
        return Err(ApiError::InvalidWorkflowRunTransition(format!(
            "workflow run {} can only resume after failure or timeout",
            id.as_str()
        )));
    }
    let step_runs = state
        .store
        .list_workflow_step_runs(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(WorkflowRunResponse::new(&run, &step_runs)))
}
