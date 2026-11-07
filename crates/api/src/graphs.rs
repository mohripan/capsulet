use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use capsulet_application::{AgentRunRecord, StartAgentRunCommand};
use capsulet_core::{AgentDefinition, AgentId, AgentRunId, AgentRunStatus, GraphId};
use capsulet_storage::ObjectStore;
use serde_json::to_string;

use crate::{
    auth::Principal,
    error::ApiError,
    http::{generated_id, project_context, require_resource_project},
    models::{
        AgentResponse, AgentRunResponse, CreateAgentRequest, CreateGraphRequest, GraphResponse,
        ListAgentRunsResponse, ListAgentsResponse, ListGraphsResponse, StartAgentRunRequest,
        termination_policy_from_conditions,
    },
    state::AppState,
    store::ApiStore,
};

pub(crate) async fn create_graph<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateGraphRequest>,
) -> Result<(StatusCode, Json<GraphResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = request.id.clone().unwrap_or_else(|| generated_id("graph"));
    let graph = request.into_graph(id)?;
    // The upsert is the ownership check: an id another project holds matches no
    // row, and the graph and its owner are written together.
    state
        .store
        .upsert_graph(&graph, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::scoped_store::<S>)?;
    Ok((StatusCode::CREATED, Json(GraphResponse::from(&graph))))
}

pub(crate) async fn list_graphs<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListGraphsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let graphs = state
        .store
        .list_graphs(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListGraphsResponse {
        graphs: graphs.iter().map(GraphResponse::from).collect(),
    }))
}

pub(crate) async fn get_graph<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<GraphResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = GraphId::new(id).map_err(ApiError::validation)?;
    require_resource_project(&state.store, "graphs", id.as_str(), &context).await?;
    let Some(graph) = state
        .store
        .find_graph(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::GraphNotFound(id.as_str().to_string()));
    };
    Ok(Json(GraphResponse::from(&graph)))
}

pub(crate) async fn create_agent<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateAgentRequest>,
) -> Result<(StatusCode, Json<AgentResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let graph_id = GraphId::new(request.graph_id.clone()).map_err(ApiError::validation)?;
    require_resource_project(&state.store, "graphs", graph_id.as_str(), &context).await?;
    let Some(graph) = state
        .store
        .find_graph(&graph_id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::GraphNotFound(graph_id.as_str().to_string()));
    };
    let id = request.id.clone().unwrap_or_else(|| generated_id("agent"));
    let agent = AgentDefinition::new(
        AgentId::new(id).map_err(ApiError::validation)?,
        request.name,
        graph,
        Some(request.budget.into_budget()?),
        Some(termination_policy_from_conditions(
            request.termination_conditions,
        )?),
    )
    .map_err(|error| ApiError::Validation(error.to_string()))?;
    state
        .store
        .upsert_agent(&agent, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::scoped_store::<S>)?;
    Ok((StatusCode::CREATED, Json(AgentResponse::from(&agent))))
}

pub(crate) async fn list_agents<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListAgentsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let agents = state
        .store
        .list_agents(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListAgentsResponse {
        agents: agents.iter().map(AgentResponse::from).collect(),
    }))
}

pub(crate) async fn get_agent<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<AgentResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = AgentId::new(id).map_err(ApiError::validation)?;
    require_resource_project(&state.store, "agents", id.as_str(), &context).await?;
    let Some(agent) = state
        .store
        .find_agent(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::AgentNotFound(id.as_str().to_string()));
    };
    Ok(Json(AgentResponse::from(&agent)))
}

pub(crate) async fn start_agent_run<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(agent_id): Path<String>,
    Json(request): Json<StartAgentRunRequest>,
) -> Result<(StatusCode, Json<AgentRunResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let agent_id = AgentId::new(agent_id).map_err(ApiError::validation)?;
    require_resource_project(&state.store, "agents", agent_id.as_str(), &context).await?;
    if state
        .store
        .find_agent(&agent_id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
        .is_none()
    {
        return Err(ApiError::AgentNotFound(agent_id.as_str().to_string()));
    }
    let run_id = AgentRunId::new(
        request
            .id
            .clone()
            .unwrap_or_else(|| generated_id("agent_run")),
    )
    .map_err(ApiError::validation)?;
    let state_json = to_string(&request.initial_state)
        .map_err(|error| ApiError::Validation(error.to_string()))?;
    let command = StartAgentRunCommand {
        run_id: run_id.clone(),
        agent_id: agent_id.clone(),
        initial_state_json: state_json.clone(),
    };
    let record = AgentRunRecord {
        id: command.run_id,
        agent_id: command.agent_id,
        status: AgentRunStatus::Queued,
        state_version: 0,
        state_json,
    };
    // Insert only: an id that is already taken is a conflict rather than a reset
    // of a run that may be under way.
    state
        .store
        .create_agent_run(&record, &context.tenant_id, &context.project_id)
        .await
        .map_err(|error| {
            if S::is_already_exists(&error) {
                ApiError::AgentRunAlreadyExists(record.id.as_str().to_string())
            } else {
                ApiError::scoped_store::<S>(error)
            }
        })?;
    Ok((StatusCode::CREATED, Json(AgentRunResponse::new(&record)?)))
}

pub(crate) async fn list_agent_runs<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListAgentRunsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let runs = state
        .store
        .list_agent_runs(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListAgentRunsResponse {
        agent_runs: runs
            .iter()
            .map(AgentRunResponse::new)
            .collect::<Result<Vec<_>, _>>()?,
    }))
}

pub(crate) async fn get_agent_run<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<AgentRunResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = AgentRunId::new(id).map_err(ApiError::validation)?;
    require_resource_project(&state.store, "agent_runs", id.as_str(), &context).await?;
    let Some(run) = state
        .store
        .find_agent_run(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::AgentRunNotFound(id.as_str().to_string()));
    };
    Ok(Json(AgentRunResponse::new(&run)?))
}
