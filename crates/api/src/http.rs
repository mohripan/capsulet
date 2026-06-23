use std::{
    collections::{BTreeMap, BTreeSet},
    convert::Infallible,
    env,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    Extension, Json, Router,
    extract::{DefaultBodyLimit, Path, Query, Request, State},
    http::{HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use capsulet_core::{
    ArtifactId, ArtifactObjectKind, AutomationId, CreateManualRunCommand, ExecutionPoolName,
    JobArtifact, JobDefinition, JobDefinitionId, JobRun, JobRunId, RetryPolicy, WorkflowDefinition,
    WorkflowDependencyPolicy, WorkflowGraph, WorkflowId, WorkflowRun, WorkflowRunId,
    WorkflowRunStatus, WorkflowStatus, WorkflowStep, WorkflowStepDependency, WorkflowStepId,
};
use capsulet_observability::{self as observability, tracing::Instrument};
use capsulet_postgres::NewServiceAccount;
use capsulet_storage::{ObjectStore, run_object_key};
use serde_json::Value;
use tower::limit::ConcurrencyLimitLayer;
use tower_governor::{
    GovernorLayer, governor::GovernorConfigBuilder, key_extractor::GlobalKeyExtractor,
};
use tower_http::{
    compression::CompressionLayer, cors::CorsLayer, limit::RequestBodyLimitLayer,
    timeout::TimeoutLayer, trace::TraceLayer,
};
use uuid::Uuid;

use crate::{
    auth::{Principal, Role, token_digest},
    error::ApiError,
    models::{
        ArtifactResponse, AuditEventResponse, CreateJobDefinitionRequest, CreateRunRequest,
        CreateServiceAccountRequest, CreateServiceAccountResponse, CreateWorkflowRequest,
        ExecutionPoolResponse, HealthResponse, HostGroupResponse, JobDefinitionResponse,
        JobDefinitionSourceResponse, JobRunLogsResponse, JobRunResponse, ListArtifactsResponse,
        ListAuditEventsResponse, ListExecutionPoolsResponse, ListHostGroupsResponse,
        ListJobDefinitionsQuery, ListJobDefinitionsResponse, ListRunsQuery, ListRunsResponse,
        ListServiceAccountsResponse, ListWorkflowRunsQuery, ListWorkflowRunsResponse,
        ListWorkflowsResponse, ServiceAccountResponse, TopologyEdgeResponse, TopologyNodeResponse,
        TopologyResponse, WorkflowEditabilityResponse, WorkflowResponse,
        WorkflowRunLogEntryResponse, WorkflowRunLogsResponse, WorkflowRunResponse,
    },
    state::AppState,
    store::ApiStore,
};

const MAX_WORKFLOW_STEPS: usize = 256;
const MAX_WORKFLOW_DEPENDENCIES: usize = 1_024;
const MAX_WORKFLOW_FAN_IN: usize = 64;
const MAX_WORKFLOW_FAN_OUT: usize = 64;
const MAX_WORKFLOW_DEPTH: usize = 64;
const DEFAULT_REQUEST_TIMEOUT_SECONDS: u64 = 30;
const DEFAULT_REQUEST_BODY_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const DEFAULT_CONCURRENCY_LIMIT: usize = 256;
const DEFAULT_RATE_LIMIT_PER_SECOND: u64 = 50;
const DEFAULT_RATE_LIMIT_BURST: u32 = 100;

#[derive(Debug, Clone)]
struct RequestId(String);

#[allow(clippy::missing_panics_doc, clippy::too_many_lines)]
pub fn router<S, O>(state: AppState<S, O>) -> Router
where
    S: ApiStore,
    O: ObjectStore,
{
    let rate_limit = Arc::new(
        GovernorConfigBuilder::default()
            .key_extractor(GlobalKeyExtractor)
            .per_second(env_u64(
                "CAPSULET_API_RATE_LIMIT_PER_SECOND",
                DEFAULT_RATE_LIMIT_PER_SECOND,
            ))
            .burst_size(env_u32(
                "CAPSULET_API_RATE_LIMIT_BURST",
                DEFAULT_RATE_LIMIT_BURST,
            ))
            .finish()
            .expect("rate limit configuration is valid"),
    );
    let protected = Router::new()
        .route("/v1/auth/me", get(current_principal))
        .route(
            "/v1/service-accounts",
            post(create_service_account).get(list_service_accounts),
        )
        .route(
            "/v1/service-accounts/{id}/revoke",
            post(revoke_service_account),
        )
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
        .route(
            "/v1/job-definitions/{id}/source",
            get(get_job_definition_source),
        )
        .route("/v1/execution-pools", get(list_execution_pools))
        .route("/v1/audit-events", get(list_audit_events))
        .route("/v1/host-groups", get(list_host_groups))
        .route("/v1/topology", get(get_topology))
        .route("/v1/workflows", post(create_workflow).get(list_workflows))
        .route(
            "/v1/workflows/{id}",
            get(get_workflow)
                .put(update_workflow)
                .delete(delete_workflow),
        )
        .route(
            "/v1/workflows/{id}/editability",
            get(get_workflow_editability),
        )
        .route(
            "/v1/automations",
            post(crate::automations::create_automation).get(crate::automations::list_automations),
        )
        .route(
            "/v1/automations/{id}",
            get(crate::automations::get_automation)
                .put(crate::automations::update_automation)
                .delete(crate::automations::delete_automation),
        )
        .route(
            "/v1/automations/{id}/enable",
            post(crate::automations::enable_automation),
        )
        .route(
            "/v1/automations/{id}/disable",
            post(crate::automations::disable_automation),
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
        .route("/v1/workflow-runs/{id}", get(get_workflow_run))
        .route("/v1/workflow-runs/{id}/logs", get(get_workflow_run_logs))
        .route(
            "/v1/workflow-runs/{id}/logs/stream",
            get(stream_workflow_run_logs),
        )
        .route("/v1/workflow-runs/{id}/remove", post(remove_workflow_run))
        .route("/v1/workflow-runs/{id}/cancel", post(cancel_workflow_run))
        .route("/v1/workflow-runs/{id}/resume", post(resume_workflow_run))
        .route("/v1/jobs/runs", post(create_run).get(list_runs))
        .route("/v1/jobs/runs/{id}", get(get_run))
        .route("/v1/jobs/runs/{id}/cancel", post(cancel_run))
        .route("/v1/jobs/runs/{id}/logs", get(get_run_logs))
        .route("/v1/jobs/runs/{id}/logs/stream", get(stream_run_logs))
        .route("/v1/jobs/runs/{id}/artifacts", get(list_artifacts))
        .route(
            "/v1/jobs/runs/{id}/artifacts/{artifact_id}",
            get(download_artifact),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .route("/healthz", get(healthz::<S, O>))
        .route("/livez", get(livez))
        .route("/readyz", get(readyz::<S, O>))
        .route("/metrics", get(metrics::<S, O>))
        .route("/openapi.json", get(openapi_spec))
        .route(
            "/v1/webhooks/{automation_id}/{trigger_name}",
            post(crate::webhooks::ingest::<S, O>).layer(DefaultBodyLimit::max(1_048_576)),
        )
        .merge(protected)
        .with_state(state)
        .layer(middleware::from_fn(request_context))
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .layer(CorsLayer::permissive())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(env_u64(
                "CAPSULET_API_REQUEST_TIMEOUT_SECONDS",
                DEFAULT_REQUEST_TIMEOUT_SECONDS,
            )),
        ))
        .layer(RequestBodyLimitLayer::new(env_usize(
            "CAPSULET_API_REQUEST_BODY_LIMIT_BYTES",
            DEFAULT_REQUEST_BODY_LIMIT_BYTES,
        )))
        .layer(ConcurrencyLimitLayer::new(env_usize(
            "CAPSULET_API_CONCURRENCY_LIMIT",
            DEFAULT_CONCURRENCY_LIMIT,
        )))
        .layer(GovernorLayer::new(rate_limit))
}

async fn metrics<S, O>(State(state): State<AppState<S, O>>) -> Response
where
    S: ApiStore,
    O: ObjectStore,
{
    match state.store.prometheus_metrics().await {
        Ok(db_body) => {
            let mut body = observability::render_metrics();
            if !body.is_empty() && !body.ends_with('\n') {
                body.push('\n');
            }
            body.push_str(&db_body);
            ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], body).into_response()
        }
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

async fn request_context(mut request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let request_id = observability::request_id(
        request
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok()),
    );
    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));
    let span = observability::tracing::info_span!(
        "http.request",
        request.id = %request_id,
        http.method = %method,
        http.route = %path,
    );
    async move {
        let started = Instant::now();
        let mut response = next.run(request).await;
        let status = response.status();
        observability::record_http_request(
            method.as_str(),
            &path,
            status.as_u16(),
            started.elapsed(),
        );
        if let Ok(value) = HeaderValue::from_str(&request_id) {
            response.headers_mut().insert("x-request-id", value);
        }
        response
    }
    .instrument(span)
    .await
}

async fn openapi_spec() -> Response {
    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "application/vnd.oai.openapi+json; charset=utf-8",
        )],
        include_str!("../openapi.json"),
    )
        .into_response()
}

async fn current_principal(Extension(principal): Extension<Principal>) -> Json<Value> {
    Json(serde_json::json!({
        "name": principal.name,
        "role": principal.role.as_str(),
        "tenant_id": principal.tenant_id,
        "project_id": principal.project_id,
        "scopes": principal.scopes().iter().map(AsRef::as_ref).collect::<Vec<_>>(),
    }))
}

async fn require_auth<S, O>(
    State(state): State<AppState<S, O>>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .map(|id| id.0.clone());
    let user_agent = request
        .headers()
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    if state.auth.enabled() && token.is_empty() {
        audit_auth_failure(
            &state,
            "anonymous",
            "unauthenticated",
            &method,
            &path,
            StatusCode::UNAUTHORIZED,
            request_id.as_deref(),
            user_agent.as_deref(),
        )
        .await;
        return Err(ApiError::Unauthorized);
    }
    let Some(principal) = authenticate_request_token(&state, token).await? else {
        audit_auth_failure(
            &state,
            "anonymous",
            "unauthenticated",
            &method,
            &path,
            StatusCode::UNAUTHORIZED,
            request_id.as_deref(),
            user_agent.as_deref(),
        )
        .await;
        return Err(ApiError::Unauthorized);
    };
    let required_scope = required_scope(request.method(), request.uri().path());
    if !principal.has_scope(required_scope) {
        audit_auth_failure(
            &state,
            &principal.name,
            principal.role.as_str(),
            &method,
            &path,
            StatusCode::FORBIDDEN,
            request_id.as_deref(),
            user_agent.as_deref(),
        )
        .await;
        return Err(ApiError::Forbidden(required_scope));
    }
    request.extensions_mut().insert(principal.clone());
    let response = next.run(request).await;
    if method != Method::GET
        && method != Method::HEAD
        && method != Method::OPTIONS
        && let Err(error) = state
            .store
            .record_audit_event(
                &principal.name,
                principal.role.as_str(),
                method.as_str(),
                &path,
                response.status().as_u16(),
                request_id.as_deref(),
                user_agent.as_deref(),
            )
            .await
    {
        observability::tracing::warn!(
            principal = %principal.name,
            %method,
            %path,
            %error,
            "failed to persist audit event"
        );
    }
    Ok(response)
}

async fn authenticate_request_token<S, O>(
    state: &AppState<S, O>,
    token: &str,
) -> Result<Option<Principal>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    if let Some(principal) = state.auth.authenticate(token) {
        return Ok(Some(principal));
    }
    let token_hash = token_digest(token);
    let Some(account) = state
        .store
        .authenticate_service_account_hash(&token_hash)
        .await
        .map_err(ApiError::store)?
    else {
        return Ok(None);
    };
    let Some(role) = Role::parse(&account.role) else {
        return Ok(None);
    };
    Ok(Some(Principal::service_account(
        account.name,
        role,
        account.tenant_id,
        account.project_id,
        account.scopes,
    )))
}

#[allow(clippy::too_many_arguments)]
async fn audit_auth_failure<S, O>(
    state: &AppState<S, O>,
    principal: &str,
    role: &str,
    method: &Method,
    path: &str,
    status: StatusCode,
    request_id: Option<&str>,
    user_agent: Option<&str>,
) where
    S: ApiStore,
    O: ObjectStore,
{
    if let Err(error) = state
        .store
        .record_audit_event(
            principal,
            role,
            method.as_str(),
            path,
            status.as_u16(),
            request_id,
            user_agent,
        )
        .await
    {
        observability::tracing::warn!(
            %method,
            %path,
            %error,
            "failed to persist auth failure audit event"
        );
    }
}

fn required_scope(method: &Method, path: &str) -> &'static str {
    if path == "/v1/auth/me" {
        return "auth:read";
    }
    if path == "/v1/audit-events" {
        return "audit:read";
    }
    if path.starts_with("/v1/service-accounts") {
        return "auth:write";
    }
    if method == Method::GET || method == Method::HEAD {
        if path.starts_with("/v1/jobs/") {
            return "jobs:read";
        }
        if path.starts_with("/v1/workflows") || path.starts_with("/v1/workflow-runs") {
            return "workflows:read";
        }
        if path.starts_with("/v1/automations") || path.starts_with("/v1/trigger-plugins") {
            return "automations:read";
        }
        return "system:read";
    }
    if path == "/v1/jobs/runs" {
        return "jobs:run";
    }
    if path.starts_with("/v1/jobs/runs/") && path.ends_with("/cancel") {
        return "jobs:cancel";
    }
    if path.starts_with("/v1/workflow-runs/")
        && (path.ends_with("/cancel") || path.ends_with("/resume") || path.ends_with("/remove"))
    {
        return "workflows:operate";
    }
    if path.starts_with("/v1/automations/") && path.ends_with("/trigger") {
        return "automations:operate";
    }
    if path.starts_with("/v1/automations/")
        && (path.ends_with("/enable") || path.ends_with("/disable"))
    {
        return "automations:operate";
    }
    if path.starts_with("/v1/job-definitions") {
        return "jobs:write";
    }
    if path.starts_with("/v1/workflows") {
        return "workflows:write";
    }
    if path.starts_with("/v1/automations") || path.starts_with("/v1/trigger-plugins") {
        return "automations:write";
    }
    "system:write"
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn env_u32(name: &str, default: u32) -> u32 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

async fn list_audit_events<S, O>(
    State(state): State<AppState<S, O>>,
) -> Result<Json<ListAuditEventsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let events = state
        .store
        .list_audit_events(100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListAuditEventsResponse {
        audit_events: events
            .into_iter()
            .map(|event| AuditEventResponse {
                id: event.id,
                principal: event.principal,
                role: event.role,
                method: event.method,
                path: event.path,
                status_code: event.status_code,
                request_id: event.request_id,
                created_at: event.created_at,
            })
            .collect(),
    }))
}

async fn list_service_accounts<S, O>(
    State(state): State<AppState<S, O>>,
) -> Result<Json<ListServiceAccountsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let accounts = state
        .store
        .list_service_accounts(100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListServiceAccountsResponse {
        service_accounts: accounts.iter().map(ServiceAccountResponse::from).collect(),
    }))
}

async fn create_service_account<S, O>(
    State(state): State<AppState<S, O>>,
    Json(request): Json<CreateServiceAccountRequest>,
) -> Result<(StatusCode, Json<CreateServiceAccountResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let name = non_empty_trimmed(&request.name, "service account name")?;
    let role = Role::parse(request.role.trim())
        .ok_or_else(|| ApiError::Validation("unknown service account role".to_string()))?;
    let scopes = if request.scopes.is_empty() {
        default_scope_strings(role)
    } else {
        validate_scope_strings(request.scopes)?
    };
    let id = request
        .id
        .unwrap_or_else(|| generated_id("service_account"));
    let id = non_empty_trimmed(&id, "service account id")?;
    let tenant_id = request.tenant_id.unwrap_or_else(|| "default".to_string());
    let project_id = request.project_id.unwrap_or_else(|| "default".to_string());
    let tenant_id = non_empty_trimmed(&tenant_id, "tenant id")?;
    let project_id = non_empty_trimmed(&project_id, "project id")?;
    let token = generate_service_account_token();
    let account = NewServiceAccount {
        id,
        name,
        tenant_id,
        project_id,
        role: role.as_str().to_string(),
        scopes,
        token_hash: token_digest(&token),
        expires_at_unix: request.expires_at_unix,
    };
    let record = state
        .store
        .create_service_account(&account)
        .await
        .map_err(ApiError::store)?;
    Ok((
        StatusCode::CREATED,
        Json(CreateServiceAccountResponse {
            account: ServiceAccountResponse::from(&record),
            token,
        }),
    ))
}

async fn revoke_service_account<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    if state
        .store
        .revoke_service_account(&id)
        .await
        .map_err(ApiError::store)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::Validation(format!(
            "unknown service account: {id}"
        )))
    }
}

fn non_empty_trimmed(value: &str, label: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ApiError::Validation(format!("{label} cannot be empty")));
    }
    Ok(value.to_string())
}

fn validate_scope_strings(scopes: Vec<String>) -> Result<Vec<String>, ApiError> {
    let mut validated = Vec::with_capacity(scopes.len());
    for scope in scopes {
        let scope = scope.trim();
        if scope.is_empty()
            || !scope.chars().all(|ch| {
                ch.is_ascii_lowercase()
                    || ch.is_ascii_digit()
                    || matches!(ch, ':' | '-' | '*' | '_')
            })
        {
            return Err(ApiError::Validation(format!("invalid scope: {scope}")));
        }
        validated.push(scope.to_string());
    }
    validated.sort();
    validated.dedup();
    Ok(validated)
}

fn default_scope_strings(role: Role) -> Vec<String> {
    match role {
        Role::Viewer => [
            "auth:read",
            "jobs:read",
            "workflows:read",
            "automations:read",
            "system:read",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        Role::Operator => [
            "auth:read",
            "jobs:read",
            "jobs:run",
            "jobs:cancel",
            "workflows:read",
            "workflows:operate",
            "automations:read",
            "automations:operate",
            "system:read",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        Role::Admin => vec!["*".to_string()],
    }
}

fn generate_service_account_token() -> String {
    format!(
        "cst_{}_{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

async fn livez() -> Json<HealthResponse> {
    Json(HealthResponse { status: "alive" })
}

async fn healthz<S, O>(State(state): State<AppState<S, O>>) -> (StatusCode, Json<HealthResponse>)
where
    S: ApiStore,
    O: ObjectStore,
{
    match state.store.ping().await {
        Ok(()) => (StatusCode::OK, Json(HealthResponse { status: "ok" })),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "not_ready",
            }),
        ),
    }
}

async fn readyz<S, O>(State(state): State<AppState<S, O>>) -> (StatusCode, Json<HealthResponse>)
where
    S: ApiStore,
    O: ObjectStore,
{
    match state.store.ping().await {
        Ok(()) => (StatusCode::OK, Json(HealthResponse { status: "ready" })),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "not_ready",
            }),
        ),
    }
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
    let definition_id = JobDefinitionId::new(id.clone()).map_err(ApiError::validation)?;
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

async fn get_job_definition_source<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<JobDefinitionSourceResponse>, ApiError>
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
    let source = state
        .object_store
        .get(definition.bundle_object_key())
        .await
        .map_err(ApiError::object_store)?
        .ok_or_else(|| ApiError::JobDefinitionSourceNotFound(id.as_str().to_string()))?;
    let python_script = String::from_utf8(source)
        .map_err(|_| ApiError::Validation("job definition source is not UTF-8".to_string()))?;
    Ok(Json(JobDefinitionSourceResponse {
        python_script,
        python_dependencies: definition.python_dependencies().to_vec(),
    }))
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
    let retry_policy = RetryPolicy::new(
        request.retry_max_attempts.unwrap_or(1),
        request.retry_delay_seconds.unwrap_or(0),
    )
    .map_err(|error| ApiError::validation(error.to_string()))?;
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

async fn update_workflow<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
    Json(mut request): Json<CreateWorkflowRequest>,
) -> Result<Json<WorkflowResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let workflow_id = WorkflowId::new(id.clone()).map_err(ApiError::validation)?;
    if state
        .store
        .workflow_has_active_runs(&workflow_id)
        .await
        .map_err(ApiError::store)?
    {
        return Err(ApiError::WorkflowLocked(id));
    }
    request.id = Some(id);
    let workflow = build_workflow(&state, request).await?;
    state
        .store
        .upsert_workflow(&workflow)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(WorkflowResponse::from(&workflow)))
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

async fn delete_workflow<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let workflow_id = WorkflowId::new(id.clone()).map_err(ApiError::validation)?;
    if state
        .store
        .find_workflow(&workflow_id)
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
        .list_automations(500)
        .await
        .map_err(ApiError::store)?
        .iter()
        .any(|automation| automation.workflow_id() == &workflow_id)
    {
        return Err(ApiError::WorkflowLocked(workflow_id.as_str().to_string()));
    }
    let deleted = state
        .store
        .delete_workflow(&workflow_id)
        .await
        .map_err(ApiError::store)?;
    if !deleted {
        return Err(ApiError::WorkflowNotFound(workflow_id.as_str().to_string()));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn get_workflow_editability<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowEditabilityResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = WorkflowId::new(id).map_err(ApiError::validation)?;
    if state
        .store
        .find_workflow(&id)
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

async fn get_topology<S, O>(
    State(state): State<AppState<S, O>>,
) -> Result<Json<TopologyResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let workflows = state
        .store
        .list_workflows(100)
        .await
        .map_err(ApiError::store)?;
    let automations = state
        .store
        .list_automations(100)
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
    for workflow in &workflows {
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
    for automation in &automations {
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

#[allow(clippy::too_many_lines)]
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
    if request.steps.len() > MAX_WORKFLOW_STEPS {
        return Err(ApiError::Validation(format!(
            "workflow has {} steps, maximum is {MAX_WORKFLOW_STEPS}",
            request.steps.len()
        )));
    }
    let workflow_id = WorkflowId::new(request.id.unwrap_or_else(|| generated_id("workflow")))
        .map_err(ApiError::validation)?;
    let dependency_requests = request.dependencies;
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
        let timeout_seconds = match step.timeout_seconds {
            Some(0) => {
                return Err(ApiError::Validation(
                    "workflow step timeout_seconds must be greater than zero".to_string(),
                ));
            }
            value => value,
        };
        steps.push(
            WorkflowStep::new(
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
            .with_timeout_seconds(timeout_seconds),
        );
    }

    let dependencies = match dependency_requests {
        Some(dependencies) => dependencies
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
            .collect::<Result<Vec<_>, ApiError>>()?,
        None => steps
            .windows(2)
            .map(|pair| WorkflowStepDependency::new(pair[0].id().clone(), pair[1].id().clone()))
            .collect(),
    };
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

fn validate_workflow_graph_limits(
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

pub(crate) fn json_from_string(value: &str) -> Result<Value, ApiError> {
    serde_json::from_str(value).map_err(|error| ApiError::Validation(error.to_string()))
}

fn normalized_datetime(value: &str) -> String {
    value.replace('T', " ")
}

fn matches_date_range(created_at: &str, start_at: Option<&str>, end_at: Option<&str>) -> bool {
    let created = normalized_datetime(created_at);
    if let Some(start) = start_at.filter(|value| !value.trim().is_empty())
        && created < normalized_datetime(start)
    {
        return false;
    }
    if let Some(end) = end_at.filter(|value| !value.trim().is_empty())
        && created > normalized_datetime(end)
    {
        return false;
    }
    true
}

fn query_contains(value: &str, query: Option<&str>) -> bool {
    let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };
    value.to_lowercase().contains(&query.to_lowercase())
}

fn is_desc(direction: Option<&str>) -> bool {
    !matches!(direction, Some(value) if value.eq_ignore_ascii_case("asc"))
}

fn filter_job_runs(mut runs: Vec<JobRun>, query: &ListRunsQuery) -> Vec<JobRun> {
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

fn filter_workflow_runs(
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
        .create_workflow_run(
            automation.workflow_id(),
            Some(automation.id()),
            &run_id,
            automation.job_input_json(),
        )
        .await
        .map_err(ApiError::store)?;
    Ok((
        StatusCode::CREATED,
        Json(WorkflowRunResponse::new(&run, &[])),
    ))
}

async fn list_workflow_runs<S, O>(
    State(state): State<AppState<S, O>>,
    Query(query): Query<ListWorkflowRunsQuery>,
) -> Result<Json<ListWorkflowRunsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let runs = state
        .store
        .list_workflow_runs(i64::from(limit))
        .await
        .map_err(ApiError::store)?;
    let runs = filter_workflow_runs(runs, &query);
    let mut workflow_runs = Vec::with_capacity(runs.len());
    for run in runs {
        let step_runs = state
            .store
            .list_workflow_step_runs(run.id())
            .await
            .map_err(ApiError::store)?;
        workflow_runs.push(WorkflowRunResponse::new(&run, &step_runs));
    }
    Ok(Json(ListWorkflowRunsResponse { workflow_runs }))
}

async fn get_workflow_run<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowRunResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = WorkflowRunId::new(id).map_err(ApiError::validation)?;
    let Some(run) = state
        .store
        .find_workflow_run(&id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::WorkflowRunNotFound(id.as_str().to_string()));
    };
    let step_runs = state
        .store
        .list_workflow_step_runs(&id)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(WorkflowRunResponse::new(&run, &step_runs)))
}

async fn get_workflow_run_logs<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowRunLogsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = WorkflowRunId::new(id).map_err(ApiError::validation)?;
    Ok(Json(load_workflow_run_logs(&state, &id).await?))
}

async fn stream_workflow_run_logs<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = WorkflowRunId::new(id).map_err(ApiError::validation)?;
    if state
        .store
        .find_workflow_run(&id)
        .await
        .map_err(ApiError::store)?
        .is_none()
    {
        return Err(ApiError::WorkflowRunNotFound(id.as_str().to_string()));
    }
    let stream_state = state.clone();
    let stream = async_stream::stream! {
        loop {
            match load_workflow_run_logs(&stream_state, &id).await {
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

async fn load_workflow_run_logs<S, O>(
    state: &AppState<S, O>,
    id: &WorkflowRunId,
) -> Result<WorkflowRunLogsResponse, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let Some(run) = state
        .store
        .find_workflow_run(id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::WorkflowRunNotFound(id.as_str().to_string()));
    };

    let step_runs = state
        .store
        .list_workflow_step_runs(id)
        .await
        .map_err(ApiError::store)?;
    let mut entries = Vec::with_capacity(step_runs.len());
    for step_run in step_runs {
        let (log, object_log_available) = if let Some(job_run_id) = step_run.maybe_job_run_id() {
            let log = state
                .store
                .find_run_log(job_run_id)
                .await
                .map_err(ApiError::store)?;
            let object_log_available = state
                .store
                .list_artifacts(job_run_id)
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

async fn remove_workflow_run<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowRunResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = WorkflowRunId::new(id).map_err(ApiError::validation)?;
    let Some(run) = state
        .store
        .remove_queued_workflow_run(&id)
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
        .list_workflow_step_runs(&id)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(WorkflowRunResponse::new(&run, &step_runs)))
}

async fn cancel_workflow_run<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowRunResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = WorkflowRunId::new(id).map_err(ApiError::validation)?;
    let Some(run) = state
        .store
        .cancel_running_workflow_run(&id)
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
        .list_workflow_step_runs(&id)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(WorkflowRunResponse::new(&run, &step_runs)))
}

async fn resume_workflow_run<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowRunResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = WorkflowRunId::new(id).map_err(ApiError::validation)?;
    let Some(run) = state
        .store
        .resume_workflow_run(&id)
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
        .list_workflow_step_runs(&id)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(WorkflowRunResponse::new(&run, &step_runs)))
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
        input_json: Some(valid_json_object_string(
            &request.input.unwrap_or_else(|| serde_json::json!({})),
            "run input",
        )?),
    }
    .into_job_run();

    state.store.save_run(&run).await.map_err(ApiError::store)?;
    let run = state
        .store
        .find_run(run.id())
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
        Vec::new(),
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
    let limit = query.limit.unwrap_or(50).clamp(1, 500);
    let runs = state
        .store
        .list_runs(i64::from(limit))
        .await
        .map_err(ApiError::store)?;
    let runs = filter_job_runs(runs, &query);

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
    Ok(Json(load_run_logs(&state, &id, true).await?))
}

async fn stream_run_logs<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError>
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
    let stream_state = state.clone();
    let stream = async_stream::stream! {
        loop {
            let run = match stream_state.store.find_run(&id).await {
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
            match load_run_logs(&stream_state, &id, false).await {
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

async fn load_run_logs<S, O>(
    state: &AppState<S, O>,
    id: &JobRunId,
    require_log: bool,
) -> Result<JobRunLogsResponse, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    if state
        .store
        .find_run(id)
        .await
        .map_err(ApiError::store)?
        .is_none()
    {
        return Err(ApiError::RunNotFound(id.as_str().to_string()));
    }

    let log = state
        .store
        .find_run_log(id)
        .await
        .map_err(ApiError::store)?;
    if require_log && log.is_none() {
        return Err(ApiError::RunLogsNotFound(id.as_str().to_string()));
    }

    let object_log_available = state
        .store
        .list_artifacts(id)
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
        .get(artifact.object_key())
        .await
        .map_err(ApiError::object_store)?
    else {
        return Err(ApiError::ArtifactObjectNotFound(
            artifact.object_key().to_string(),
        ));
    };

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, artifact.content_type().to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", artifact.name()),
            ),
        ],
        bytes,
    )
        .into_response())
}

fn generated_run_id() -> String {
    generated_id("run")
}

pub(crate) fn valid_json_object_string(value: &Value, label: &str) -> Result<String, ApiError> {
    if value.is_object() {
        Ok(value.to_string())
    } else {
        Err(ApiError::Validation(format!(
            "{label} must be a JSON object"
        )))
    }
}

pub(crate) fn generated_id(prefix: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    format!("{prefix}_{millis}")
}
