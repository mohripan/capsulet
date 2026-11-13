//! Execution pools, job definitions, and job runs with their logs.

use std::{convert::Infallible, time::Duration};

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use capsulet_application::CreateManualRunCommand;
use capsulet_core::{
    ArtifactId, ArtifactObjectKind, ExecutionPoolName, JobArtifact, JobDefinition, JobDefinitionId,
    JobRun, JobRunId, RetryPolicy,
};
use capsulet_observability::{self as observability};
use capsulet_storage::{ObjectStore, run_object_key};
use sha2::{Digest, Sha256};

use super::admission::enforce_job_run_admission;
use super::common::{
    generated_id, is_desc, matches_date_range, query_contains, valid_json_object_string,
};
use super::identity::{
    ProjectContext, project_context, require_project_role, require_resource_project,
};
use crate::{
    auth::Principal,
    error::ApiError,
    models::{
        CreateJobDefinitionRequest, CreateRunRequest, ExecutionPoolResponse, HostGroupResponse,
        JobDefinitionResponse, JobDefinitionSourceResponse, JobRunLogsResponse, JobRunResponse,
        ListExecutionPoolsResponse, ListHostGroupsResponse, ListJobDefinitionsQuery,
        ListJobDefinitionsResponse, ListRunsQuery, ListRunsResponse,
    },
    state::AppState,
    store::ApiStore,
};

pub(super) async fn list_execution_pools<S, O>(
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

pub(super) async fn list_host_groups<S, O>(
    State(state): State<AppState<S, O>>,
) -> Json<ListHostGroupsResponse>
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

pub(super) async fn create_job_definition<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateJobDefinitionRequest>,
) -> Result<(StatusCode, Json<JobDefinitionResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, crate::ProjectRole::Operator)?;
    // Reject existing cross-project ids before writing a bundle. The scoped upsert below
    // also rejects races, and content-addressed bundles preserve the existing script.
    if let Some(requested_id) = request.id.as_deref() {
        let definition_id =
            JobDefinitionId::new(requested_id.to_string()).map_err(ApiError::validation)?;
        if state
            .store
            .job_definition_exists(&definition_id)
            .await
            .map_err(ApiError::store)?
        {
            require_resource_project(
                &state.store,
                "job_definitions",
                definition_id.as_str(),
                &context,
            )
            .await?;
        }
    }
    let definition = build_python_job_definition(&state, request).await?;
    state
        .store
        .upsert_job_definition(&definition, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::scoped_store::<S>)?;

    Ok((
        StatusCode::CREATED,
        Json(JobDefinitionResponse::from(&definition)),
    ))
}

pub(super) async fn update_job_definition<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
    Json(mut request): Json<CreateJobDefinitionRequest>,
) -> Result<Json<JobDefinitionResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, crate::ProjectRole::Operator)?;
    let definition_id = JobDefinitionId::new(id.clone()).map_err(ApiError::validation)?;
    // A scoped read: the update below then acts on a definition this project owns,
    // and ownership never changes once written.
    if state
        .store
        .find_job_definition(&definition_id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
        .is_none()
    {
        return Err(ApiError::ResourceNotFound);
    }
    if state
        .store
        .job_definition_has_active_workflow_runs(&definition_id)
        .await
        .map_err(ApiError::store)?
    {
        return Err(ApiError::WorkflowLocked(id));
    }
    request.id = Some(id);
    let definition = build_python_job_definition(&state, request).await?;
    state
        .store
        .upsert_job_definition(&definition, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::scoped_store::<S>)?;

    Ok(Json(JobDefinitionResponse::from(&definition)))
}

pub(super) async fn list_job_definitions<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Query(query): Query<ListJobDefinitionsQuery>,
) -> Result<Json<ListJobDefinitionsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let limit = query
        .limit
        .unwrap_or(crate::models::ListJobDefinitionsQuery::DEFAULT_LIMIT)
        .clamp(1, crate::models::LIST_LIMIT_MAX);
    let definitions = state
        .store
        .list_job_definitions(&context.tenant_id, &context.project_id, i64::from(limit))
        .await
        .map_err(ApiError::store)?;
    // Scoped in the query, before the limit.
    let scoped = definitions;

    Ok(Json(ListJobDefinitionsResponse {
        job_definitions: scoped.iter().map(JobDefinitionResponse::from).collect(),
    }))
}

pub(super) async fn get_job_definition<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<JobDefinitionResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = JobDefinitionId::new(id).map_err(ApiError::validation)?;
    require_resource_project(&state.store, "job_definitions", id.as_str(), &context).await?;
    let Some(definition) = state
        .store
        .find_job_definition(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::UnknownJobDefinition(id.as_str().to_string()));
    };

    Ok(Json(JobDefinitionResponse::from(&definition)))
}

pub(super) async fn get_job_definition_source<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<JobDefinitionSourceResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = JobDefinitionId::new(id).map_err(ApiError::validation)?;
    require_resource_project(&state.store, "job_definitions", id.as_str(), &context).await?;
    let Some(definition) = state
        .store
        .find_job_definition(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::UnknownJobDefinition(id.as_str().to_string()));
    };
    let source = state
        .object_store
        .get(definition.bundle_object_key())
        .await
        .map_err(ApiError::object_store)?;
    let python_script = match source {
        Some(source) => String::from_utf8(source)
            .map_err(|_| ApiError::Validation("job definition source is not UTF-8".to_string()))?,
        None => inline_python_source(&definition)
            .ok_or_else(|| ApiError::JobDefinitionSourceNotFound(id.as_str().to_string()))?,
    };
    Ok(Json(JobDefinitionSourceResponse {
        python_script,
        python_dependencies: definition.python_dependencies().to_vec(),
    }))
}

pub(super) fn inline_python_source(definition: &JobDefinition) -> Option<String> {
    let command = definition.command();
    if command.len() == 3 && command[0] == "python" && command[1] == "-c" {
        Some(command[2].clone())
    } else {
        None
    }
}

pub(super) async fn delete_job_definition<S, O>(
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
    let id = JobDefinitionId::new(id).map_err(ApiError::validation)?;
    require_resource_project(&state.store, "job_definitions", id.as_str(), &context).await?;
    if state
        .store
        .job_definition_is_used_by_workflows(&id)
        .await
        .map_err(ApiError::store)?
    {
        return Err(ApiError::JobDefinitionInUse(id.as_str().to_string()));
    }
    let deleted = state
        .store
        .delete_job_definition(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?;
    if !deleted {
        return Err(ApiError::UnknownJobDefinition(id.as_str().to_string()));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn build_python_job_definition<S, O>(
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
    let retry_policy = RetryPolicy::new(
        request.retry_max_attempts.unwrap_or(1),
        request.retry_delay_seconds.unwrap_or(0),
    )
    .map_err(|error| ApiError::validation(error.to_string()))?;
    let runtime_image = request
        .runtime_image
        .unwrap_or_else(|| "python:3.12-slim".to_string());
    let script_digest = Sha256::digest(script.as_bytes())
        .iter()
        .flat_map(|byte| {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            [
                char::from(HEX[usize::from(byte >> 4)]),
                char::from(HEX[usize::from(byte & 15)]),
            ]
        })
        .collect::<String>();
    let object_key = format!(
        "bundles/job-definitions/{}/{script_digest}/main.py",
        id.as_str()
    );
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
        request.python_dependencies,
        object_key,
        &valid_json_object_string(
            &request
                .input_schema
                .unwrap_or_else(|| serde_json::json!({})),
            "job input schema",
        )?,
        retry_policy,
    )
    .map_err(ApiError::validation)
}

pub(super) fn filter_job_runs(mut runs: Vec<JobRun>, query: &ListRunsQuery) -> Vec<JobRun> {
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
            run.job_definition_id().as_str(),
            run.execution_pool().as_str()
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
        "run" | "id" => runs.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str())),
        "job_definition" | "name" => runs.sort_by(|left, right| {
            left.job_definition_id()
                .as_str()
                .cmp(right.job_definition_id().as_str())
        }),
        "state" | "status" => {
            runs.sort_by_key(|run| run.status().to_string());
        }
        "pool" => {
            runs.sort_by(|left, right| {
                left.execution_pool()
                    .as_str()
                    .cmp(right.execution_pool().as_str())
            });
        }
        "attempts" => runs.sort_by_key(JobRun::attempt_count),
        _ => runs.sort_by(|left, right| left.created_at().cmp(right.created_at())),
    }
    if desc {
        runs.reverse();
    }
    runs
}

pub(super) async fn create_run<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateRunRequest>,
) -> Result<(StatusCode, Json<JobRunResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, crate::ProjectRole::Operator)?;
    let execution_pool =
        ExecutionPoolName::new(request.execution_pool).map_err(ApiError::validation)?;

    if !state.knows_pool(execution_pool.as_str()) {
        return Err(ApiError::UnknownExecutionPool(
            execution_pool.as_str().to_string(),
        ));
    }

    let run_id = match request.run_id {
        Some(value) => {
            let run_id = JobRunId::new(value).map_err(ApiError::validation)?;
            // Checked here as well as in the write below, because a script run creates its
            // definition first and a taken id should not leave one behind.
            if state
                .store
                .job_run_exists(&run_id)
                .await
                .map_err(ApiError::store)?
            {
                // Taken here is a conflict. Taken by another project is something this
                // caller has no way to know, so it reads as nothing being there.
                let ours = state
                    .store
                    .find_run(&run_id, &context.tenant_id, &context.project_id)
                    .await
                    .map_err(ApiError::store)?
                    .is_some();
                return Err(if ours {
                    ApiError::RunAlreadyExists(run_id.as_str().to_string())
                } else {
                    ApiError::ResourceNotFound
                });
            }
            run_id
        }
        None => JobRunId::new(generated_run_id()).map_err(ApiError::validation)?,
    };
    let (job_definition_id, bundle_metadata) = if let Some(script) = request.python_script {
        create_script_definition(&state, &context, &run_id, script).await?
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
        // It exists; whether it is this project's is a scoped read, and another
        // project's reads as nothing there.
        if state
            .store
            .find_job_definition(&job_definition_id, &context.tenant_id, &context.project_id)
            .await
            .map_err(ApiError::store)?
            .is_none()
        {
            return Err(ApiError::ResourceNotFound);
        }
        (job_definition_id, None)
    };

    let run = CreateManualRunCommand {
        run_id,
        job_definition_id,
        execution_pool,
        input_json: Some(valid_json_object_string(
            &request.input.unwrap_or_else(|| serde_json::json!({})),
            "run input",
        )?),
    }
    .into_job_run();

    enforce_job_run_admission(&state, run.execution_pool().as_str()).await?;
    // Insert only, with its owner: an id that is taken is a conflict rather than a
    // reset of a run that may be under way, however the race with the check above went.
    let created = state
        .store
        .create_job_run(&run, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::scoped_store::<S>)?;
    if !created {
        return Err(ApiError::RunAlreadyExists(run.id().as_str().to_string()));
    }
    let run = state
        .store
        .find_run(run.id(), &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
        .unwrap_or(run);
    if let Some(metadata) = bundle_metadata {
        state
            .store
            .save_artifact(&metadata)
            .await
            .map_err(ApiError::store)?;
    }

    Ok((StatusCode::CREATED, Json(JobRunResponse::from(&run))))
}

pub(super) async fn create_script_definition<S, O>(
    state: &AppState<S, O>,
    context: &ProjectContext,
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
        Vec::new(),
        object_key.clone(),
        "{}",
        RetryPolicy::no_retry(),
    )
    .map_err(ApiError::validation)?;
    state
        .store
        .upsert_job_definition(&definition, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::scoped_store::<S>)?;
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

pub(super) async fn list_runs<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Query(query): Query<ListRunsQuery>,
) -> Result<Json<ListRunsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let limit = query
        .limit
        .unwrap_or(crate::models::ListRunsQuery::DEFAULT_LIMIT)
        .clamp(1, crate::models::LIST_LIMIT_MAX);
    let runs = state
        .store
        .list_runs(&context.tenant_id, &context.project_id, i64::from(limit))
        .await
        .map_err(ApiError::store)?;
    // Scoped in the query, before the limit.
    let runs = filter_job_runs(runs, &query);
    Ok(Json(ListRunsResponse {
        runs: runs.iter().map(JobRunResponse::from).collect(),
    }))
}

pub(super) async fn get_run<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<JobRunResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = JobRunId::new(id).map_err(ApiError::validation)?;
    let Some(run) = state
        .store
        .find_run(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::RunNotFound(id.as_str().to_string()));
    };

    Ok(Json(JobRunResponse::from(&run)))
}

pub(super) async fn get_run_logs<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<JobRunLogsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = JobRunId::new(id).map_err(ApiError::validation)?;
    Ok(Json(load_run_logs(&state, &id, &context, true).await?))
}

pub(super) async fn stream_run_logs<S, O>(
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
    let id = JobRunId::new(id).map_err(ApiError::validation)?;
    if state
        .store
        .find_run(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
        .is_none()
    {
        return Err(ApiError::RunNotFound(id.as_str().to_string()));
    }
    let stream_state = state.clone();
    let stream_context = context.clone();
    let stream = async_stream::stream! {
        loop {
            let run = match stream_state.store.find_run(&id, &stream_context.tenant_id, &stream_context.project_id).await {
                Ok(Some(run)) => run,
                Ok(None) => {
                    yield Ok::<_, Infallible>(Event::default().event("error").data("run not found"));
                    break;
                }
                Err(error) => {
                    yield Ok(Event::default().event("error").data(error.to_string()));
                    break;
                }
            };
            match load_run_logs(&stream_state, &id, &stream_context, false).await {
                Ok(snapshot) => match serde_json::to_string(&snapshot) {
                    Ok(data) => yield Ok(Event::default().event("snapshot").data(data)),
                    Err(error) => {
                        observability::tracing::warn!(%error, job_run_id = %id.as_str(), "failed to serialize job log snapshot");
                        break;
                    }
                },
                Err(error) => {
                    yield Ok(Event::default().event("error").data(error.to_string()));
                    break;
                }
            }
            if run.status().is_terminal() {
                yield Ok(Event::default().event("done").data(run.status().to_string()));
                break;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    };
    Ok(Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response())
}

pub(super) async fn load_run_logs<S, O>(
    state: &AppState<S, O>,
    id: &JobRunId,
    context: &ProjectContext,
    require_log: bool,
) -> Result<JobRunLogsResponse, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    if state
        .store
        .find_run(id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
        .is_none()
    {
        return Err(ApiError::RunNotFound(id.as_str().to_string()));
    }

    let log = state
        .store
        .find_run_log(id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?;
    if require_log && log.is_none() {
        return Err(ApiError::RunLogsNotFound(id.as_str().to_string()));
    }

    let object_log_available = state
        .store
        .list_artifacts(id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
        .iter()
        .any(|artifact| artifact.kind() == ArtifactObjectKind::Log);

    Ok(JobRunLogsResponse {
        run_id: id.as_str().to_string(),
        logs: log.map_or_else(String::new, |log| log.text),
        object_log_available,
    })
}

pub(super) async fn cancel_run<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<JobRunResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, crate::ProjectRole::Operator)?;
    let id = JobRunId::new(id).map_err(ApiError::validation)?;
    let Some(run) = state
        .store
        .cancel_run(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::RunNotFound(id.as_str().to_string()));
    };

    Ok(Json(JobRunResponse::from(&run)))
}

pub(super) fn generated_run_id() -> String {
    generated_id("run")
}
