//! Projects, their memberships, service accounts, and the audit log.

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use capsulet_postgres::{NewProjectMembership, NewServiceAccount};
use capsulet_storage::ObjectStore;
use uuid::Uuid;

use super::common::{generated_id, non_empty_trimmed};
use super::identity::{project_context, require_project_admin};
use crate::{
    auth::{Principal, Role, token_digest},
    error::ApiError,
    models::{
        AuditEventResponse, CreateServiceAccountRequest, CreateServiceAccountResponse,
        ListAuditEventsResponse, ListProjectMembershipsResponse, ListProjectsResponse,
        ListServiceAccountsResponse, ProjectMembershipResponse, ProjectResponse,
        ServiceAccountResponse, UpsertProjectMembershipRequest, UpsertProjectRequest,
    },
    state::AppState,
    store::ApiStore,
};

pub(super) async fn list_projects<S, O>(
    State(state): State<AppState<S, O>>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListProjectsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let projects = if principal.platform_admin {
        state
            .store
            .list_all_projects(principal.tenant_id.as_ref())
            .await
            .map_err(ApiError::store)?
    } else {
        let project_ids = principal
            .project_memberships
            .iter()
            .map(|membership| membership.project_id.to_string())
            .collect::<Vec<_>>();
        state
            .store
            .list_projects(principal.tenant_id.as_ref(), &project_ids)
            .await
            .map_err(ApiError::store)?
    };
    Ok(Json(ListProjectsResponse {
        projects: projects.iter().map(ProjectResponse::from).collect(),
    }))
}

/// Creates the project in the caller's tenant, or renames it. Repeating the
/// same request changes nothing, which is what lets a seed run twice.
pub(super) async fn upsert_project<S, O>(
    State(state): State<AppState<S, O>>,
    Extension(principal): Extension<Principal>,
    Path(project_id): Path<String>,
    Json(request): Json<UpsertProjectRequest>,
) -> Result<Json<ProjectResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let project_id = validate_project_id(&project_id)?;
    require_project_admin(&principal, &principal.tenant_id, &project_id)?;
    let project = capsulet_postgres::ProjectRecord {
        id: project_id,
        tenant_id: principal.tenant_id.to_string(),
        name: non_empty_trimmed(&request.name, "project name")?,
    };
    let stored = state
        .store
        .upsert_project(&project)
        .await
        .map_err(ApiError::store)?
        .ok_or_else(|| ApiError::ProjectUnavailable(project.id.clone()))?;
    Ok(Json(ProjectResponse::from(&stored)))
}

/// A project id travels in paths and in `x-capsulet-project-id`, so it is held
/// to characters that need no escaping in either.
pub(super) fn validate_project_id(value: &str) -> Result<String, ApiError> {
    let valid = (1..=63).contains(&value.len())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric());
    if valid {
        Ok(value.to_string())
    } else {
        Err(ApiError::Validation(
            "a project id is 1 to 63 lowercase letters, digits, '-' or '_', starting with a letter or digit"
                .to_string(),
        ))
    }
}

pub(super) async fn list_project_memberships<S, O>(
    State(state): State<AppState<S, O>>,
    Extension(principal): Extension<Principal>,
    Path(project_id): Path<String>,
) -> Result<Json<ListProjectMembershipsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&HeaderMap::new(), &principal)?;
    let project_id = non_empty_trimmed(&project_id, "project id")?;
    require_project_admin(&principal, &context.tenant_id, &project_id)?;
    let memberships = state
        .store
        .list_project_memberships(&context.tenant_id, &project_id)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListProjectMembershipsResponse {
        memberships: memberships
            .iter()
            .map(ProjectMembershipResponse::from)
            .collect(),
    }))
}

pub(super) async fn upsert_project_membership<S, O>(
    State(state): State<AppState<S, O>>,
    Extension(principal): Extension<Principal>,
    Path(project_id): Path<String>,
    Json(request): Json<UpsertProjectMembershipRequest>,
) -> Result<Json<ProjectMembershipResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&HeaderMap::new(), &principal)?;
    let project_id = non_empty_trimmed(&project_id, "project id")?;
    require_project_admin(&principal, &context.tenant_id, &project_id)?;
    let principal_kind = validate_principal_kind(&request.principal_kind)?;
    let principal_name = non_empty_trimmed(&request.principal_name, "principal name")?;
    let role = validate_project_role(&request.role)?;
    let membership = NewProjectMembership {
        id: format!(
            "{}_{}_{}_{}",
            context.tenant_id, project_id, principal_kind, principal_name
        ),
        tenant_id: context.tenant_id,
        project_id,
        principal_kind,
        principal_name,
        role,
        created_by: principal.name.to_string(),
    };
    let membership = state
        .store
        .upsert_project_membership(&membership)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ProjectMembershipResponse::from(&membership)))
}

pub(super) async fn delete_project_membership<S, O>(
    State(state): State<AppState<S, O>>,
    Extension(principal): Extension<Principal>,
    Path((project_id, principal_kind, principal_name)): Path<(String, String, String)>,
) -> Result<StatusCode, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&HeaderMap::new(), &principal)?;
    let project_id = non_empty_trimmed(&project_id, "project id")?;
    require_project_admin(&principal, &context.tenant_id, &project_id)?;
    let principal_kind = validate_principal_kind(&principal_kind)?;
    let principal_name = non_empty_trimmed(&principal_name, "principal name")?;
    let deleted = state
        .store
        .delete_project_membership(
            &context.tenant_id,
            &project_id,
            &principal_kind,
            &principal_name,
        )
        .await
        .map_err(ApiError::store)?;
    if !deleted {
        return Err(ApiError::Validation(
            "project membership not found".to_string(),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub(super) fn validate_project_role(value: &str) -> Result<String, ApiError> {
    match value {
        "project_viewer" | "project_operator" | "project_admin" => Ok(value.to_string()),
        _ => Err(ApiError::Validation("invalid project role".to_string())),
    }
}

pub(super) fn validate_principal_kind(value: &str) -> Result<String, ApiError> {
    match value {
        "user" | "group" | "service_account" => Ok(value.to_string()),
        _ => Err(ApiError::Validation("invalid principal kind".to_string())),
    }
}

pub(super) async fn list_audit_events<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListAuditEventsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let events = state
        .store
        .list_audit_events(&context.tenant_id, &context.project_id, 100)
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

pub(super) async fn list_service_accounts<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListServiceAccountsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let accounts = state
        .store
        .list_service_accounts(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListServiceAccountsResponse {
        service_accounts: accounts.iter().map(ServiceAccountResponse::from).collect(),
    }))
}

pub(super) async fn create_service_account<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateServiceAccountRequest>,
) -> Result<(StatusCode, Json<CreateServiceAccountResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
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
    let tenant_id = request
        .tenant_id
        .unwrap_or_else(|| context.tenant_id.clone());
    let project_id = request
        .project_id
        .unwrap_or_else(|| context.project_id.clone());
    let tenant_id = non_empty_trimmed(&tenant_id, "tenant id")?;
    let project_id = non_empty_trimmed(&project_id, "project id")?;
    if tenant_id != context.tenant_id || project_id != context.project_id {
        return Err(ApiError::Forbidden("project:access"));
    }
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
        Json(CreateServiceAccountResponse::from_record(&record, token)),
    ))
}

pub(super) async fn revoke_service_account<S, O>(
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
    if state
        .store
        .revoke_service_account(&context.tenant_id, &context.project_id, &id)
        .await
        .map_err(ApiError::store)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::ResourceNotFound)
    }
}

pub(super) fn validate_scope_strings(scopes: Vec<String>) -> Result<Vec<String>, ApiError> {
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

pub(super) fn default_scope_strings(role: Role) -> Vec<String> {
    crate::auth::scope_strings_for_role(role)
}

pub(super) fn generate_service_account_token() -> String {
    format!(
        "cst_{}_{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}
