use std::collections::{BTreeMap, BTreeSet};

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use capsulet_application::{AgentRunExecution, AgentRunRecord, StartAgentRunCommand};
use capsulet_core::{AgentDefinition, AgentId, AgentRunId, AgentRunStatus, GraphId};
use capsulet_ir::admission::admit;
use capsulet_ir::agent::HUMAN_REVIEWER;
use capsulet_ir::definition::Definition;
use capsulet_ir::id::Identifier;
use capsulet_ir_adapters::from_agent;
use capsulet_postgres::{AgentRunStart, MAX_IR_RUN_VALUE_BYTES};
use capsulet_runtime::Signal;
use capsulet_runtime::values::check_value;
use capsulet_storage::ObjectStore;
use serde_json::{Value, to_string};

use crate::{
    assurance::{IrRunEventResponse, IrRunEventsResponse, recorded_now},
    auth::Principal,
    error::ApiError,
    http::{generated_id, internal::ProjectContext, project_context, require_resource_project},
    models::{
        AgentResponse, AgentRunResponse, CreateAgentRequest, CreateGraphRequest,
        DecideAgentRunRequest, GraphResponse, ListAgentRunsResponse, ListAgentsResponse,
        ListGraphsResponse, StartAgentRunRequest, termination_policy_from_conditions,
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
    let Some(agent) = state
        .store
        .find_agent(&agent_id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::AgentNotFound(agent_id.as_str().to_string()));
    };
    let run_id = AgentRunId::new(
        request
            .id
            .clone()
            .unwrap_or_else(|| generated_id("agent_run")),
    )
    .map_err(ApiError::validation)?;

    // Compiled here rather than when the agent was saved, because what a run executes
    // is pinned by digest at the moment it starts. A refusal names what is wrong with
    // the agent, which is something its author can fix.
    let adapted =
        from_agent(&agent).map_err(|error| ApiError::AgentNotRunnable(error.to_string()))?;
    let admission = admit(&adapted.definition).map_err(|refusal| ApiError::AdmissionRefused {
        code: refusal.code.as_str().to_string(),
        owner: refusal.owner.as_str().to_string(),
        detail: refusal.detail,
    })?;
    let inputs = bind_inputs(&adapted.definition, &request.initial_state)?;

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
        execution: None,
    };
    state
        .store
        .start_agent_run(
            &AgentRunStart {
                run: &record,
                definition: &adapted.definition,
                admission: &admission,
                inputs: &inputs,
                at: recorded_now(),
            },
            &context.tenant_id,
            &context.project_id,
        )
        .await
        .map_err(|error| {
            if S::is_already_exists(&error) {
                ApiError::AgentRunAlreadyExists(record.id.as_str().to_string())
            } else {
                ApiError::scoped_store::<S>(error)
            }
        })?;

    // Read back, so a run that is already moving reports where it actually is.
    let started = state
        .store
        .find_agent_run(&record.id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
        .unwrap_or(record);
    Ok((StatusCode::CREATED, Json(AgentRunResponse::new(&started)?)))
}

/// Takes each value a definition reads from the state a run was started with, and
/// checks it against the port it feeds.
///
/// Refusing here means a bad request gets an answer now, rather than a queued run
/// that fails later for a reason the caller could have been told.
fn bind_inputs(
    definition: &Definition,
    initial_state: &Value,
) -> Result<BTreeMap<String, String>, ApiError> {
    let Some(state) = initial_state.as_object() else {
        return Err(ApiError::Validation(
            "initial_state must be an object".to_string(),
        ));
    };
    let mut inputs = BTreeMap::new();
    for port in definition.graph.inputs() {
        let name = port.id.as_str();
        let value = state.get(name).ok_or_else(|| {
            ApiError::Validation(format!(
                "initial_state is missing `{name}`, which this agent reads"
            ))
        })?;
        check_value(&port.schema, value).map_err(|violation| {
            ApiError::Validation(format!("initial_state.{name} {violation}"))
        })?;
        let encoded = to_string(value).map_err(|error| ApiError::Validation(error.to_string()))?;
        if encoded.len() > MAX_IR_RUN_VALUE_BYTES {
            return Err(ApiError::Validation(format!(
                "initial_state.{name} is larger than the {MAX_IR_RUN_VALUE_BYTES} bytes a run may \
                 store"
            )));
        }
        inputs.insert(name.to_string(), encoded);
    }
    Ok(inputs)
}

/// Reads a run's log.
///
/// The log rather than a summary of it, because every other statement about where a
/// run is comes from folding exactly these events.
pub(crate) async fn get_agent_run_events<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<IrRunEventsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = AgentRunId::new(id).map_err(ApiError::validation)?;
    let run = find_run(&state.store, &id, &context).await?;
    let Some(execution) = run.execution else {
        // A run from before agent runs executed never had a log.
        return Ok(Json(IrRunEventsResponse {
            run_id: id.as_str().to_string(),
            events: Vec::new(),
        }));
    };
    let events = state
        .store
        .load_ir_run_events(
            &context.tenant_id,
            &context.project_id,
            &execution.ir_run_id,
        )
        .await
        .map_err(ApiError::store)?;
    Ok(Json(IrRunEventsResponse {
        run_id: id.as_str().to_string(),
        events: events
            .into_iter()
            .map(|recorded| IrRunEventResponse {
                position: recorded.position,
                epoch: recorded.epoch.0,
                kind: recorded.event.as_str().to_string(),
                recorded_at: recorded.at.epoch_millis(),
                event: recorded.event,
            })
            .collect(),
    }))
}

/// Asks a run to stop.
///
/// A request, delivered to the run's inbox. The worker records it and the decision
/// core picks the next point where stopping is safe, which is never in the middle of
/// an effect. So this answers `202`, and the run reports `cancelled` once it has.
pub(crate) async fn cancel_agent_run<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<AgentRunResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = AgentRunId::new(id).map_err(ApiError::validation)?;
    let run = find_run(&state.store, &id, &context).await?;
    let execution = active_execution(&run)?;
    state
        .store
        .deliver_ir_run_signal(
            &context.tenant_id,
            &context.project_id,
            &execution.ir_run_id,
            &Signal::cancellation(identity_of(&principal)?),
        )
        .await
        .map_err(ApiError::store)?;
    Ok((StatusCode::ACCEPTED, Json(AgentRunResponse::new(&run)?)))
}

/// Answers the gate a run is waiting at.
///
/// Approving waives the failure that raised the gate, for the attempt that follows.
/// Refusing ends the run, after anything it still owes has been compensated.
pub(crate) async fn decide_agent_run<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
    Json(request): Json<DecideAgentRunRequest>,
) -> Result<(StatusCode, Json<AgentRunResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = AgentRunId::new(id).map_err(ApiError::validation)?;
    let run = find_run(&state.store, &id, &context).await?;
    let execution = active_execution(&run)?;
    if run.status != AgentRunStatus::Waiting {
        return Err(ApiError::AgentRunStateConflict(id.as_str().to_string()));
    }

    // Holding `agents:operate` in this project is what the reviewer authority is, and
    // the signal records that as a fact about this moment.
    let authority =
        Identifier::parse(HUMAN_REVIEWER).map_err(|error| ApiError::store(error.to_string()))?;
    let authorities = BTreeSet::from([authority.clone()]);
    let by = identity_of(&principal)?;
    let signal = if request.approved {
        Signal::decision(authority, by, authorities)
    } else {
        Signal::rejection(authority, by, authorities)
    };
    state
        .store
        .deliver_ir_run_signal(
            &context.tenant_id,
            &context.project_id,
            &execution.ir_run_id,
            &signal,
        )
        .await
        .map_err(ApiError::store)?;
    Ok((StatusCode::ACCEPTED, Json(AgentRunResponse::new(&run)?)))
}

async fn find_run<S: ApiStore>(
    store: &S,
    id: &AgentRunId,
    context: &ProjectContext,
) -> Result<AgentRunRecord, ApiError> {
    store
        .find_agent_run(id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
        .ok_or(ApiError::ResourceNotFound)
}

/// The execution behind a run that can still be told something.
fn active_execution(run: &AgentRunRecord) -> Result<&AgentRunExecution, ApiError> {
    let finished = matches!(
        run.status,
        AgentRunStatus::Succeeded
            | AgentRunStatus::Failed
            | AgentRunStatus::Cancelled
            | AgentRunStatus::Stopped
    );
    match &run.execution {
        Some(execution) if !finished => Ok(execution),
        // Finished, or from before agent runs executed: nothing is listening.
        _ => Err(ApiError::AgentRunStateConflict(run.id.as_str().to_string())),
    }
}

/// Who is doing this, in the form a run's log can carry.
fn identity_of(principal: &Principal) -> Result<Identifier, ApiError> {
    let name: String = principal
        .name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || "_-.:/@+".contains(character) {
                character
            } else {
                '-'
            }
        })
        .take(128)
        .collect();
    Identifier::parse(name).map_err(|error| ApiError::store(error.to_string()))
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
