//! Who is calling and in which project: bearer authentication, the project a request is about, and the roles and scopes it needs.

use std::sync::Arc;

use axum::{
    Extension, Json,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::Response,
};
use capsulet_observability::{self as observability};
use capsulet_postgres::{AuditScope, NewAuditEvent};
use capsulet_storage::ObjectStore;

use super::router::RequestId;
use crate::{
    auth::{Principal, ProjectMembership, ProjectRole, Role, token_digest},
    error::ApiError,
    state::AppState,
    store::ApiStore,
};

#[derive(Debug, Clone)]
pub(crate) struct ProjectContext {
    pub(crate) tenant_id: String,
    pub(crate) project_id: String,
    pub(crate) role: ProjectRole,
}

pub(super) async fn current_principal(
    Extension(principal): Extension<Principal>,
) -> Json<crate::models::PrincipalResponse> {
    Json(crate::models::PrincipalResponse::from(&principal))
}

pub(crate) fn project_context(
    headers: &HeaderMap,
    principal: &Principal,
) -> Result<ProjectContext, ApiError> {
    let requested_project = headers
        .get("x-capsulet-project-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(principal.project_id.as_ref());
    let tenant_id = principal.tenant_id.to_string();
    if principal.platform_admin {
        return Ok(ProjectContext {
            tenant_id,
            project_id: requested_project.to_string(),
            role: ProjectRole::Admin,
        });
    }
    let Some(membership) = principal.project_memberships.iter().find(|membership| {
        membership.tenant_id.as_ref() == tenant_id
            && membership.project_id.as_ref() == requested_project
    }) else {
        return Err(ApiError::Forbidden("project:access"));
    };
    Ok(ProjectContext {
        tenant_id,
        project_id: requested_project.to_string(),
        role: membership.role,
    })
}

pub(crate) fn require_project_role(
    context: &ProjectContext,
    required: ProjectRole,
) -> Result<(), ApiError> {
    if context.role >= required {
        Ok(())
    } else {
        Err(ApiError::Forbidden("project:role"))
    }
}

pub(super) fn require_project_admin(
    principal: &Principal,
    tenant_id: &str,
    project_id: &str,
) -> Result<(), ApiError> {
    if principal.platform_admin {
        return Ok(());
    }
    let allowed = principal.project_memberships.iter().any(|membership| {
        membership.tenant_id.as_ref() == tenant_id
            && membership.project_id.as_ref() == project_id
            && membership.role == ProjectRole::Admin
    });
    if allowed {
        Ok(())
    } else {
        Err(ApiError::Forbidden("project:admin"))
    }
}

pub(crate) async fn require_resource_project<S: ApiStore>(
    store: &S,
    resource: &str,
    id: &str,
    context: &ProjectContext,
) -> Result<(), ApiError> {
    let Some((tenant_id, project_id)) = store
        .resource_project(resource, id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::ResourceNotFound);
    };
    if tenant_id == context.tenant_id && project_id == context.project_id {
        Ok(())
    } else {
        Err(ApiError::ResourceNotFound)
    }
}

pub(super) async fn require_auth<S, O>(
    State(state): State<AppState<S, O>>,
    endpoint: &'static crate::EndpointContract,
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
    let unauthenticated_event = NewAuditEvent {
        principal: "anonymous",
        role: "unauthenticated",
        method: method.as_str(),
        path: &path,
        status_code: StatusCode::UNAUTHORIZED.as_u16(),
        request_id: request_id.as_deref(),
        user_agent: user_agent.as_deref(),
        scope: None,
    };
    if state.auth.enabled() && token.is_empty() {
        audit_auth_failure(&state, &unauthenticated_event).await;
        return Err(ApiError::Unauthorized);
    }
    let Some(principal) = authenticate_request_token(&state, token).await? else {
        audit_auth_failure(&state, &unauthenticated_event).await;
        return Err(ApiError::Unauthorized);
    };
    let required_scope = endpoint.policy.permission.as_str();
    let context = endpoint_project_context(endpoint, &request, &principal)?;
    let project_allowed = context.as_ref().is_none_or(|context| {
        endpoint
            .policy
            .allows(context.role, principal.platform_admin)
    });
    let audit_scope = context.as_ref().map(|context| AuditScope {
        tenant_id: &context.tenant_id,
        project_id: &context.project_id,
    });
    if !project_allowed || !principal.has_scope(required_scope) {
        audit_auth_failure(
            &state,
            &NewAuditEvent {
                principal: &principal.name,
                role: principal.role.as_str(),
                method: method.as_str(),
                path: &path,
                status_code: StatusCode::FORBIDDEN.as_u16(),
                request_id: request_id.as_deref(),
                user_agent: user_agent.as_deref(),
                scope: audit_scope,
            },
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
            .record_audit_event(&NewAuditEvent {
                principal: &principal.name,
                role: principal.role.as_str(),
                method: method.as_str(),
                path: &path,
                status_code: response.status().as_u16(),
                request_id: request_id.as_deref(),
                user_agent: user_agent.as_deref(),
                scope: audit_scope,
            })
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

pub(super) fn endpoint_project_context(
    endpoint: &crate::EndpointContract,
    request: &Request,
    principal: &Principal,
) -> Result<Option<ProjectContext>, ApiError> {
    if !endpoint.policy.ownership.is_required() {
        return Ok(None);
    }
    let mut headers = request.headers().clone();
    if endpoint.policy.ownership == crate::OwnershipRule::PathProject {
        let project = endpoint
            .path
            .split('/')
            .zip(request.uri().path().split('/'))
            .find_map(|(template, value)| (template == "{project_id}").then_some(value))
            .ok_or_else(|| ApiError::MissingEndpointContract(request.uri().path().to_string()))?;
        headers.insert(
            "x-capsulet-project-id",
            HeaderValue::from_str(project)
                .map_err(|_| ApiError::Validation("invalid project id".to_string()))?,
        );
    }
    project_context(&headers, principal).map(Some)
}

pub(super) async fn authenticate_request_token<S, O>(
    state: &AppState<S, O>,
    token: &str,
) -> Result<Option<Principal>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    if let Some(principal) = state.auth.authenticate(token) {
        return hydrate_project_memberships(state, principal)
            .await
            .map(Some);
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
    let principal = Principal::service_account(
        account.name,
        role,
        account.tenant_id,
        account.project_id,
        account.scopes,
    );
    hydrate_project_memberships(state, principal)
        .await
        .map(Some)
}

pub(super) async fn hydrate_project_memberships<S, O>(
    state: &AppState<S, O>,
    principal: Principal,
) -> Result<Principal, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let mut memberships: Vec<ProjectMembership> =
        principal.project_memberships.iter().cloned().collect();
    for membership in state
        .store
        .list_principal_project_memberships(
            &principal.tenant_id,
            &principal.name,
            principal.principal_kind(),
        )
        .await
        .map_err(ApiError::store)?
    {
        let exists = memberships.iter().any(|existing| {
            existing.tenant_id.as_ref() == membership.tenant_id
                && existing.project_id.as_ref() == membership.project_id
        });
        // A stored role this build does not recognise grants nothing. Defaulting
        // to the weakest known role would still be inventing a grant nobody made.
        let Some(role) = ProjectRole::parse(&membership.role) else {
            continue;
        };
        if !exists {
            memberships.push(ProjectMembership {
                tenant_id: Arc::from(membership.tenant_id),
                project_id: Arc::from(membership.project_id),
                role,
            });
        }
    }
    Ok(principal.with_project_memberships(memberships))
}

pub(super) async fn audit_auth_failure<S, O>(state: &AppState<S, O>, event: &NewAuditEvent<'_>)
where
    S: ApiStore,
    O: ObjectStore,
{
    if let Err(error) = state.store.record_audit_event(event).await {
        observability::tracing::warn!(
            method = %event.method,
            path = %event.path,
            %error,
            "failed to persist auth failure audit event"
        );
    }
}
