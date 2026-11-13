//! Health, the calling principal, projects and their memberships, service accounts, and the audit log.

use capsulet_postgres::{ProjectMembershipRecord, ProjectRecord, ServiceAccountRecord};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct HealthResponse {
    pub(crate) status: &'static str,
    /// The release this API was built as: the product version in `release.toml`.
    pub(crate) version: &'static str,
}

impl HealthResponse {
    pub(crate) fn new(status: &'static str) -> Self {
        Self {
            status,
            version: env!("CARGO_PKG_VERSION"),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct CreateServiceAccountRequest {
    pub(crate) id: Option<String>,
    pub(crate) name: String,
    pub(crate) role: String,
    #[serde(default)]
    pub(crate) tenant_id: Option<String>,
    #[serde(default)]
    pub(crate) project_id: Option<String>,
    #[serde(default)]
    pub(crate) scopes: Vec<String>,
    pub(crate) expires_at_unix: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ServiceAccountResponse {
    pub id: String,
    pub name: String,
    pub tenant_id: String,
    pub project_id: String,
    pub role: String,
    pub scopes: Vec<String>,
    #[schema(required)]
    pub expires_at: Option<String>,
    #[schema(required)]
    pub revoked_at: Option<String>,
    #[schema(required)]
    pub last_used_at: Option<String>,
    pub created_at: String,
}

impl From<&ServiceAccountRecord> for ServiceAccountResponse {
    fn from(account: &ServiceAccountRecord) -> Self {
        Self {
            id: account.id.clone(),
            name: account.name.clone(),
            tenant_id: account.tenant_id.clone(),
            project_id: account.project_id.clone(),
            role: account.role.clone(),
            scopes: account.scopes.clone(),
            expires_at: account.expires_at.clone(),
            revoked_at: account.revoked_at.clone(),
            last_used_at: account.last_used_at.clone(),
            created_at: account.created_at.clone(),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateServiceAccountResponse {
    pub id: String,
    pub name: String,
    pub tenant_id: String,
    pub project_id: String,
    pub role: String,
    pub scopes: Vec<String>,
    #[schema(required)]
    pub expires_at: Option<String>,
    #[schema(required)]
    pub revoked_at: Option<String>,
    #[schema(required)]
    pub last_used_at: Option<String>,
    pub created_at: String,
    pub token: String,
}

impl CreateServiceAccountResponse {
    pub(crate) fn from_record(account: &ServiceAccountRecord, token: String) -> Self {
        Self {
            id: account.id.clone(),
            name: account.name.clone(),
            tenant_id: account.tenant_id.clone(),
            project_id: account.project_id.clone(),
            role: account.role.clone(),
            scopes: account.scopes.clone(),
            expires_at: account.expires_at.clone(),
            revoked_at: account.revoked_at.clone(),
            last_used_at: account.last_used_at.clone(),
            created_at: account.created_at.clone(),
            token,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ListServiceAccountsResponse {
    pub(crate) service_accounts: Vec<ServiceAccountResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ProjectResponse {
    pub(crate) id: String,
    pub(crate) tenant_id: String,
    pub(crate) name: String,
}

impl From<&ProjectRecord> for ProjectResponse {
    fn from(project: &ProjectRecord) -> Self {
        Self {
            id: project.id.clone(),
            tenant_id: project.tenant_id.clone(),
            name: project.name.clone(),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ListProjectsResponse {
    pub(crate) projects: Vec<ProjectResponse>,
}

/// Creates the project named in the path in the caller's tenant, or renames it.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpsertProjectRequest {
    pub(crate) name: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct UpsertProjectMembershipRequest {
    pub(crate) principal_kind: String,
    pub(crate) principal_name: String,
    pub(crate) role: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ProjectMembershipResponse {
    pub(crate) id: String,
    pub(crate) tenant_id: String,
    pub(crate) project_id: String,
    pub(crate) principal_kind: String,
    pub(crate) principal_name: String,
    pub(crate) role: String,
    pub(crate) created_by: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

impl From<&ProjectMembershipRecord> for ProjectMembershipResponse {
    fn from(membership: &ProjectMembershipRecord) -> Self {
        Self {
            id: membership.id.clone(),
            tenant_id: membership.tenant_id.clone(),
            project_id: membership.project_id.clone(),
            principal_kind: membership.principal_kind.clone(),
            principal_name: membership.principal_name.clone(),
            role: membership.role.clone(),
            created_by: membership.created_by.clone(),
            created_at: membership.created_at.clone(),
            updated_at: membership.updated_at.clone(),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ListProjectMembershipsResponse {
    pub(crate) memberships: Vec<ProjectMembershipResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct AuditEventResponse {
    pub(crate) id: i64,
    pub(crate) principal: String,
    pub(crate) role: String,
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) status_code: i32,
    pub(crate) request_id: Option<String>,
    pub(crate) created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ListAuditEventsResponse {
    pub(crate) audit_events: Vec<AuditEventResponse>,
}

/// Who a request is authenticated as: its role, its projects and its scopes.
#[derive(Debug, Serialize, ToSchema)]
pub struct PrincipalResponse {
    pub name: String,
    pub role: String,
    pub platform_admin: bool,
    pub tenant_id: String,
    pub project_id: String,
    pub project_memberships: Vec<PrincipalMembershipResponse>,
    pub scopes: Vec<String>,
}

/// A project the principal belongs to, and its role there.
#[derive(Debug, Serialize, ToSchema)]
pub struct PrincipalMembershipResponse {
    pub tenant_id: String,
    pub project_id: String,
    pub role: String,
}

impl From<&crate::Principal> for PrincipalResponse {
    fn from(principal: &crate::Principal) -> Self {
        Self {
            name: principal.name.to_string(),
            role: principal.role.as_str().to_string(),
            platform_admin: principal.platform_admin,
            tenant_id: principal.tenant_id.to_string(),
            project_id: principal.project_id.to_string(),
            project_memberships: principal
                .project_memberships
                .iter()
                .map(|membership| PrincipalMembershipResponse {
                    tenant_id: membership.tenant_id.to_string(),
                    project_id: membership.project_id.to_string(),
                    role: membership.role.as_str().to_string(),
                })
                .collect(),
            scopes: principal
                .scopes()
                .iter()
                .map(|scope| AsRef::<str>::as_ref(scope).to_string())
                .collect(),
        }
    }
}
