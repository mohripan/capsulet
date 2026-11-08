//! Stable metadata for every public HTTP operation.

use std::{borrow::Cow, collections::BTreeSet, fmt};

use utoipa::openapi::{RefOr, Schema};
use utoipa::{PartialSchema, ToSchema};

/// HTTP methods supported by the public contract registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

impl HttpMethod {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
            Self::Patch => "PATCH",
        }
    }

    #[must_use]
    pub const fn as_lowercase(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Post => "post",
            Self::Put => "put",
            Self::Delete => "delete",
            Self::Patch => "patch",
        }
    }
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Stability level exposed for a public operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stability {
    Experimental,
}

impl Stability {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Experimental => "experimental",
        }
    }
}

use crate::auth::ProjectRole;

macro_rules! permissions {
    ($($variant:ident => ($scope:literal, $resource:ident, $role:expr)),+ $(,)?) => {
        /// Permission required before a handler may execute.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum Permission { $($variant),+ }

        impl Permission {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $scope),+ }
            }

            #[must_use]
            pub const fn minimum_project_role(self) -> Option<ProjectRole> {
                match self { $(Self::$variant => $role),+ }
            }

            #[must_use]
            pub const fn resource(self) -> ResourceKind {
                match self { $(Self::$variant => ResourceKind::$resource),+ }
            }
        }
    };
}

permissions! {
    Public => ("public", Public, None),
    AuthRead => ("auth:read", Identity, None),
    AuthWrite => ("auth:write", Identity, Some(ProjectRole::Admin)),
    AuditRead => ("audit:read", Audit, Some(ProjectRole::Viewer)),
    SystemRead => ("system:read", System, Some(ProjectRole::Viewer)),
    SystemWrite => ("system:write", System, Some(ProjectRole::Admin)),
    JobsRead => ("jobs:read", Jobs, Some(ProjectRole::Viewer)),
    JobsWrite => ("jobs:write", Jobs, Some(ProjectRole::Admin)),
    WorkflowsRead => ("workflows:read", Workflows, Some(ProjectRole::Viewer)),
    WorkflowsWrite => ("workflows:write", Workflows, Some(ProjectRole::Admin)),
    AutomationsRead => ("automations:read", Automations, Some(ProjectRole::Viewer)),
    AutomationsWrite => ("automations:write", Automations, Some(ProjectRole::Admin)),
    JobsRun => ("jobs:run", Jobs, Some(ProjectRole::Operator)),
    JobsCancel => ("jobs:cancel", Jobs, Some(ProjectRole::Operator)),
    WorkflowsOperate => ("workflows:operate", Workflows, Some(ProjectRole::Operator)),
    AutomationsOperate => ("automations:operate", Automations, Some(ProjectRole::Operator)),
    GraphsRead => ("graphs:read", Graphs, Some(ProjectRole::Viewer)),
    GraphsWrite => ("graphs:write", Graphs, Some(ProjectRole::Admin)),
    AgentsRead => ("agents:read", Agents, Some(ProjectRole::Viewer)),
    AgentsWrite => ("agents:write", Agents, Some(ProjectRole::Admin)),
    MemoryRead => ("memory:read", Memory, Some(ProjectRole::Viewer)),
    MemoryWrite => ("memory:write", Memory, Some(ProjectRole::Admin)),
    ConnectorsRead => ("connectors:read", Connectors, Some(ProjectRole::Viewer)),
    ConnectorsWrite => ("connectors:write", Connectors, Some(ProjectRole::Admin)),
    ReviewRead => ("review:read", Review, Some(ProjectRole::Viewer)),
    ReviewWrite => ("review:write", Review, Some(ProjectRole::Admin)),
    AgentsOperate => ("agents:operate", Agents, Some(ProjectRole::Operator)),
    ReasoningOperate => ("reasoning:operate", Reasoning, Some(ProjectRole::Operator)),
    CertificatesRead => ("certificates:read", Certificates, Some(ProjectRole::Viewer)),
    IngestionRead => ("ingestion:read", Ingestion, Some(ProjectRole::Viewer)),
    IngestionOperate => ("ingestion:operate", Ingestion, Some(ProjectRole::Operator)),
    ProjectMembersWrite => ("project-members:write", ProjectMemberships, Some(ProjectRole::Admin)),
    ServiceAccountsWrite => ("service-accounts:write", ServiceAccounts, Some(ProjectRole::Admin)),
}

/// Compatibility name retained for downstream users of the original contract API.
pub type RequiredPermission = Permission;

/// Resource family protected by an endpoint policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResourceKind {
    Unknown,
    Public,
    Identity,
    Audit,
    System,
    Jobs,
    Workflows,
    Automations,
    ExecutionPools,
    Artifacts,
    Logs,
    Graphs,
    Agents,
    Reasoning,
    Certificates,
    Memory,
    Connectors,
    Ingestion,
    Review,
    ProjectMemberships,
    ServiceAccounts,
}

impl ResourceKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Public => "public",
            Self::Identity => "identity",
            Self::Audit => "audit",
            Self::System => "system",
            Self::Jobs => "jobs",
            Self::Workflows => "workflows",
            Self::Automations => "automations",
            Self::ExecutionPools => "execution_pools",
            Self::Artifacts => "artifacts",
            Self::Logs => "logs",
            Self::Graphs => "graphs",
            Self::Agents => "agents",
            Self::Reasoning => "reasoning",
            Self::Certificates => "certificates",
            Self::Memory => "memory",
            Self::Connectors => "connectors",
            Self::Ingestion => "ingestion",
            Self::Review => "review",
            Self::ProjectMemberships => "project_memberships",
            Self::ServiceAccounts => "service_accounts",
        }
    }
}

/// How tenant/project ownership is selected for an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnershipRule {
    Unscoped,
    SelectedProject,
    PathProject,
}

impl OwnershipRule {
    #[must_use]
    pub const fn is_required(self) -> bool {
        !matches!(self, Self::Unscoped)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unscoped => "unscoped",
            Self::SelectedProject => "selected_project",
            Self::PathProject => "path_project",
        }
    }
}

/// Runtime and documentation policy attached to exactly one public operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointPolicy {
    pub permission: Permission,
    pub resource: ResourceKind,
    pub ownership: OwnershipRule,
}

impl EndpointPolicy {
    const fn new(permission: Permission, ownership: OwnershipRule) -> Self {
        Self {
            permission,
            resource: permission.resource(),
            ownership,
        }
    }

    #[must_use]
    pub const fn allows(self, role: ProjectRole, platform_admin: bool) -> bool {
        platform_admin || role.allows(self.permission)
    }
}

/// Whether an operation selects a project context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectContextRule {
    None,
    SelectedProject,
}

impl ProjectContextRule {
    #[must_use]
    pub const fn is_required(self) -> bool {
        matches!(self, Self::SelectedProject)
    }
}

/// Authentication mode for a public operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthenticationMode {
    Anonymous,
    Bearer,
}

impl AuthenticationMode {
    #[must_use]
    pub const fn requires_authentication(self) -> bool {
        matches!(self, Self::Bearer)
    }
}

/// Successful response status declared by a public operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseStatus {
    Ok,
    Created,
    Accepted,
    NoContent,
}

impl ResponseStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "200",
            Self::Created => "201",
            Self::Accepted => "202",
            Self::NoContent => "204",
        }
    }
}

/// Response content types supported by public operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    Json,
    PlainText,
    EventStream,
    Binary,
    OpenApiJson,
}

impl MediaType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Json => "application/json",
            Self::PlainText => "text/plain",
            Self::EventStream => "text/event-stream",
            Self::Binary => "application/octet-stream",
            Self::OpenApiJson => "application/vnd.oai.openapi+json",
        }
    }
}

/// The Rust wire type an endpoint reads or writes, by its schema.
///
/// An endpoint names a type, never a string: its component name, its schema and
/// the schemas it refers to all come from the type's `ToSchema` implementation,
/// so the contract cannot describe a payload the handler does not produce, and a
/// misspelled schema is a compile error rather than an empty object.
#[derive(Clone, Copy)]
pub struct SchemaId {
    name: fn() -> Cow<'static, str>,
    schema: fn() -> RefOr<Schema>,
    dependencies: fn(&mut Vec<(String, RefOr<Schema>)>),
}

impl SchemaId {
    /// The schema of `T`.
    #[must_use]
    pub const fn of<T: ToSchema>() -> Self {
        Self {
            name: <T as ToSchema>::name,
            schema: <T as PartialSchema>::schema,
            dependencies: <T as ToSchema>::schemas,
        }
    }

    /// The stable component name emitted into `OpenAPI`.
    #[must_use]
    pub fn name(self) -> Cow<'static, str> {
        (self.name)()
    }

    /// The type's own schema.
    #[must_use]
    pub fn schema(self) -> RefOr<Schema> {
        (self.schema)()
    }

    /// Every component the type's schema refers to, transitively.
    #[must_use]
    pub fn dependencies(self) -> Vec<(String, RefOr<Schema>)> {
        let mut dependencies = Vec::new();
        (self.dependencies)(&mut dependencies);
        dependencies
    }
}

// Compared and ordered by component name: two handles on one type are the same
// schema, and two types that claim one name are a conflict `generated_openapi`
// refuses.
impl PartialEq for SchemaId {
    fn eq(&self, other: &Self) -> bool {
        self.name() == other.name()
    }
}

impl Eq for SchemaId {}

impl PartialOrd for SchemaId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SchemaId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name().cmp(&other.name())
    }
}

impl fmt::Debug for SchemaId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("SchemaId")
            .field(&self.name())
            .finish()
    }
}

macro_rules! optional_schema {
    (None) => {
        None
    };
    (Some($type:ty)) => {
        Some(SchemaId::of::<$type>())
    };
}

/// Error response shape shared by public endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorContract {
    Json,
}

/// Public HTTP operation metadata shared by authorization and `OpenAPI` generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointContract {
    pub method: HttpMethod,
    pub path: &'static str,
    pub operation_id: &'static str,
    pub stability: Stability,
    pub required_permission: RequiredPermission,
    pub project_context: ProjectContextRule,
    pub policy: EndpointPolicy,
    pub authentication: AuthenticationMode,
    pub request_schema: Option<SchemaId>,
    pub response_schema: SchemaId,
    pub error_contract: ErrorContract,
    pub success_status: ResponseStatus,
    pub response_content_type: MediaType,
}

macro_rules! http_method {
    ("GET") => {
        HttpMethod::Get
    };
    ("POST") => {
        HttpMethod::Post
    };
    ("PUT") => {
        HttpMethod::Put
    };
    ("DELETE") => {
        HttpMethod::Delete
    };
    ("PATCH") => {
        HttpMethod::Patch
    };
}

macro_rules! required_permission {
    ("graphs:read") => {
        RequiredPermission::GraphsRead
    };
    ("graphs:write") => {
        RequiredPermission::GraphsWrite
    };
    ("agents:read") => {
        RequiredPermission::AgentsRead
    };
    ("agents:write") => {
        RequiredPermission::AgentsWrite
    };
    ("connectors:read") => {
        RequiredPermission::ConnectorsRead
    };
    ("connectors:write") => {
        RequiredPermission::ConnectorsWrite
    };
    ("review:read") => {
        RequiredPermission::ReviewRead
    };
    ("review:write") => {
        RequiredPermission::ReviewWrite
    };
    ("agents:operate") => {
        RequiredPermission::AgentsOperate
    };
    ("reasoning:operate") => {
        RequiredPermission::ReasoningOperate
    };
    ("certificates:read") => {
        RequiredPermission::CertificatesRead
    };
    ("ingestion:read") => {
        RequiredPermission::IngestionRead
    };
    ("ingestion:operate") => {
        RequiredPermission::IngestionOperate
    };
    ("project-members:write") => {
        RequiredPermission::ProjectMembersWrite
    };
    ("service-accounts:write") => {
        RequiredPermission::ServiceAccountsWrite
    };
    ("public") => {
        RequiredPermission::Public
    };
    ("auth:read") => {
        RequiredPermission::AuthRead
    };
    ("auth:write") => {
        RequiredPermission::AuthWrite
    };
    ("audit:read") => {
        RequiredPermission::AuditRead
    };
    ("system:read") => {
        RequiredPermission::SystemRead
    };
    ("system:write") => {
        RequiredPermission::SystemWrite
    };
    ("jobs:read") => {
        RequiredPermission::JobsRead
    };
    ("jobs:write") => {
        RequiredPermission::JobsWrite
    };
    ("jobs:run") => {
        RequiredPermission::JobsRun
    };
    ("jobs:cancel") => {
        RequiredPermission::JobsCancel
    };
    ("workflows:read") => {
        RequiredPermission::WorkflowsRead
    };
    ("workflows:write") => {
        RequiredPermission::WorkflowsWrite
    };
    ("workflows:operate") => {
        RequiredPermission::WorkflowsOperate
    };
    ("memory:read") => {
        RequiredPermission::MemoryRead
    };
    ("memory:write") => {
        RequiredPermission::MemoryWrite
    };
    ("automations:read") => {
        RequiredPermission::AutomationsRead
    };
    ("automations:write") => {
        RequiredPermission::AutomationsWrite
    };
    ("automations:operate") => {
        RequiredPermission::AutomationsOperate
    };
}

macro_rules! ownership_rule {
    (path) => {
        OwnershipRule::PathProject
    };
    (true) => {
        OwnershipRule::SelectedProject
    };
    (false) => {
        OwnershipRule::Unscoped
    };
}

macro_rules! project_context_rule {
    (path) => {
        ProjectContextRule::None
    };
    (true) => {
        ProjectContextRule::SelectedProject
    };
    (false) => {
        ProjectContextRule::None
    };
}

macro_rules! authentication_mode {
    (true) => {
        AuthenticationMode::Bearer
    };
    (false) => {
        AuthenticationMode::Anonymous
    };
}

macro_rules! response_status {
    ("200") => {
        ResponseStatus::Ok
    };
    ("201") => {
        ResponseStatus::Created
    };
    ("202") => {
        ResponseStatus::Accepted
    };
    ("204") => {
        ResponseStatus::NoContent
    };
}

macro_rules! media_type {
    ("application/json") => {
        MediaType::Json
    };
    ("text/plain") => {
        MediaType::PlainText
    };
    ("text/event-stream") => {
        MediaType::EventStream
    };
    ("application/octet-stream") => {
        MediaType::Binary
    };
    ("application/vnd.oai.openapi+json") => {
        MediaType::OpenApiJson
    };
}

impl EndpointContract {
    /// Returns path-template parameter names in declaration order.
    #[must_use]
    pub fn path_parameters(&self) -> Vec<&str> {
        self.path
            .split('/')
            .filter_map(|segment| segment.strip_prefix('{')?.strip_suffix('}'))
            .collect()
    }
}

macro_rules! endpoint {
    ($method:tt, $path:literal, $operation:literal, $scope:tt, $project:tt, $auth:tt, $request:ident $(($request_type:ty))?, $response:ty, $status:tt) => {
        EndpointContract {
            method: http_method!($method),
            path: $path,
            operation_id: $operation,
            stability: Stability::Experimental,
            required_permission: required_permission!($scope),
            project_context: project_context_rule!($project),
            policy: EndpointPolicy::new(required_permission!($scope), ownership_rule!($project)),
            authentication: authentication_mode!($auth),
            request_schema: optional_schema!($request $(($request_type))?),
            response_schema: SchemaId::of::<$response>(),
            error_contract: ErrorContract::Json,
            success_status: response_status!($status),
            response_content_type: MediaType::Json,
        }
    };
    ($method:tt, $path:literal, $operation:literal, $scope:tt, $project:tt, $auth:tt, $request:ident $(($request_type:ty))?, $response:ty, $status:tt, $content:tt) => {
        EndpointContract {
            response_content_type: media_type!($content),
            ..endpoint!(
                $method, $path, $operation, $scope, $project, $auth, $request $(($request_type))?, $response, $status
            )
        }
    };
}

const ENDPOINTS: &[EndpointContract] = &[
    endpoint!(
        "GET",
        "/healthz",
        "getHealth",
        "public",
        false,
        false,
        None,
        crate::models::HealthResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/livez",
        "getLiveness",
        "public",
        false,
        false,
        None,
        crate::models::HealthResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/readyz",
        "getReadiness",
        "public",
        false,
        false,
        None,
        crate::models::HealthResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/metrics",
        "getMetrics",
        "public",
        false,
        false,
        None,
        crate::wire_schemas::StringResponse,
        "200",
        "text/plain"
    ),
    endpoint!(
        "GET",
        "/openapi.json",
        "getOpenApi",
        "public",
        false,
        false,
        None,
        crate::wire_schemas::OpenApiDocument,
        "200",
        "application/vnd.oai.openapi+json"
    ),
    endpoint!(
        "POST",
        "/v1/webhooks/{automation_id}/{trigger_name}",
        "ingestWebhook",
        "public",
        false,
        false,
        Some(crate::wire_schemas::WebhookRequest),
        crate::webhooks::WebhookResponse,
        "202"
    ),
    endpoint!(
        "GET",
        "/v1/auth/me",
        "getCurrentPrincipal",
        "auth:read",
        false,
        true,
        None,
        crate::models::PrincipalResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/projects",
        "listProjects",
        "auth:read",
        false,
        true,
        None,
        crate::models::ListProjectsResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/projects/{project_id}/memberships",
        "listProjectMemberships",
        "project-members:write",
        path,
        true,
        None,
        crate::models::ListProjectMembershipsResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/projects/{project_id}/memberships",
        "upsertProjectMembership",
        "project-members:write",
        path,
        true,
        Some(crate::models::UpsertProjectMembershipRequest),
        crate::models::ProjectMembershipResponse,
        "200"
    ),
    endpoint!(
        "DELETE",
        "/v1/projects/{project_id}/memberships/{principal_kind}/{principal_name}",
        "deleteProjectMembership",
        "project-members:write",
        path,
        true,
        None,
        crate::wire_schemas::EmptyResponse,
        "204"
    ),
    endpoint!(
        "GET",
        "/v1/service-accounts",
        "listServiceAccounts",
        "service-accounts:write",
        true,
        true,
        None,
        crate::models::ListServiceAccountsResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/service-accounts",
        "createServiceAccount",
        "service-accounts:write",
        true,
        true,
        Some(crate::models::CreateServiceAccountRequest),
        crate::models::CreateServiceAccountResponse,
        "201"
    ),
    endpoint!(
        "POST",
        "/v1/service-accounts/{id}/revoke",
        "revokeServiceAccount",
        "service-accounts:write",
        true,
        true,
        None,
        crate::wire_schemas::EmptyResponse,
        "204"
    ),
    endpoint!(
        "GET",
        "/v1/job-definitions",
        "listJobDefinitions",
        "system:read",
        true,
        true,
        None,
        crate::models::ListJobDefinitionsResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/job-definitions",
        "createJobDefinition",
        "jobs:write",
        true,
        true,
        Some(crate::models::CreateJobDefinitionRequest),
        crate::models::JobDefinitionResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/job-definitions/{id}",
        "getJobDefinition",
        "system:read",
        true,
        true,
        None,
        crate::models::JobDefinitionResponse,
        "200"
    ),
    endpoint!(
        "PUT",
        "/v1/job-definitions/{id}",
        "updateJobDefinition",
        "jobs:write",
        true,
        true,
        Some(crate::models::CreateJobDefinitionRequest),
        crate::models::JobDefinitionResponse,
        "200"
    ),
    endpoint!(
        "DELETE",
        "/v1/job-definitions/{id}",
        "deleteJobDefinition",
        "jobs:write",
        true,
        true,
        None,
        crate::wire_schemas::EmptyResponse,
        "204"
    ),
    endpoint!(
        "GET",
        "/v1/job-definitions/{id}/source",
        "getJobDefinitionSource",
        "system:read",
        true,
        true,
        None,
        crate::models::JobDefinitionSourceResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/execution-pools",
        "listExecutionPools",
        "system:read",
        true,
        true,
        None,
        crate::models::ListExecutionPoolsResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/audit-events",
        "listAuditEvents",
        "audit:read",
        true,
        true,
        None,
        crate::models::ListAuditEventsResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/host-groups",
        "listHostGroups",
        "system:read",
        true,
        true,
        None,
        crate::models::ListHostGroupsResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/topology",
        "getTopology",
        "system:read",
        true,
        true,
        None,
        crate::models::TopologyResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/workflows",
        "listWorkflows",
        "workflows:read",
        true,
        true,
        None,
        crate::models::ListWorkflowsResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/workflows",
        "createWorkflow",
        "workflows:write",
        true,
        true,
        Some(crate::models::CreateWorkflowRequest),
        crate::models::WorkflowResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/workflows/{id}",
        "getWorkflow",
        "workflows:read",
        true,
        true,
        None,
        crate::models::WorkflowResponse,
        "200"
    ),
    endpoint!(
        "PUT",
        "/v1/workflows/{id}",
        "updateWorkflow",
        "workflows:write",
        true,
        true,
        Some(crate::models::CreateWorkflowRequest),
        crate::models::WorkflowResponse,
        "200"
    ),
    endpoint!(
        "DELETE",
        "/v1/workflows/{id}",
        "deleteWorkflow",
        "workflows:write",
        true,
        true,
        None,
        crate::wire_schemas::EmptyResponse,
        "204"
    ),
    endpoint!(
        "GET",
        "/v1/workflows/{id}/editability",
        "getWorkflowEditability",
        "workflows:read",
        true,
        true,
        None,
        crate::models::WorkflowEditabilityResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/graphs",
        "listGraphs",
        "graphs:read",
        true,
        true,
        None,
        crate::models::ListGraphsResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/graphs",
        "createGraph",
        "graphs:write",
        true,
        true,
        Some(crate::models::CreateGraphRequest),
        crate::models::GraphResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/graphs/{id}",
        "getGraph",
        "graphs:read",
        true,
        true,
        None,
        crate::models::GraphResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/agents",
        "listAgents",
        "agents:read",
        true,
        true,
        None,
        crate::models::ListAgentsResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/agents",
        "createAgent",
        "agents:write",
        true,
        true,
        Some(crate::models::CreateAgentRequest),
        crate::models::AgentResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/agents/{id}",
        "getAgent",
        "agents:read",
        true,
        true,
        None,
        crate::models::AgentResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/agents/{id}/runs",
        "startAgentRun",
        "agents:operate",
        true,
        true,
        Some(crate::models::StartAgentRunRequest),
        crate::models::AgentRunResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/agent-runs",
        "listAgentRuns",
        "agents:read",
        true,
        true,
        None,
        crate::models::ListAgentRunsResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/agent-runs/{id}",
        "getAgentRun",
        "agents:read",
        true,
        true,
        None,
        crate::models::AgentRunResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/agent-runs/{id}/events",
        "getAgentRunEvents",
        "agents:read",
        true,
        true,
        None,
        crate::assurance::IrRunEventsResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/agent-runs/{id}/cancel",
        "cancelAgentRun",
        "agents:operate",
        true,
        true,
        None,
        crate::models::AgentRunResponse,
        "202"
    ),
    endpoint!(
        "POST",
        "/v1/agent-runs/{id}/debug-bundle",
        "exportAgentRunBundle",
        "agents:read",
        true,
        true,
        None,
        crate::models::AgentRunBundleResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/agent-runs/{id}/erase",
        "eraseAgentRunPayloads",
        "agents:write",
        true,
        true,
        None,
        crate::models::AgentRunErasureResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/agent-runs/{id}/decisions",
        "decideAgentRun",
        "agents:operate",
        true,
        true,
        Some(crate::models::DecideAgentRunRequest),
        crate::models::AgentRunResponse,
        "202"
    ),
    endpoint!(
        "POST",
        "/v1/reasoning/ask",
        "askReasoning",
        "reasoning:operate",
        true,
        true,
        Some(crate::reasoning::AskRequest),
        crate::reasoning::AskResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/ir/definitions",
        "registerIrDefinition",
        "memory:write",
        true,
        true,
        Some(crate::assurance::RegisterDefinitionRequest),
        crate::assurance::RegisterDefinitionResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/ir/definitions",
        "listIrDefinitions",
        "memory:read",
        true,
        true,
        None,
        crate::assurance::ListDefinitionVersionsResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/ir/definitions/{digest}",
        "getIrDefinitionVersion",
        "memory:read",
        true,
        true,
        None,
        crate::assurance::DefinitionVersionResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/ir/runs",
        "startIrRun",
        "jobs:run",
        true,
        true,
        Some(crate::assurance::StartIrRunRequest),
        crate::assurance::IrRunResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/ir/runs",
        "listIrRuns",
        "memory:read",
        true,
        true,
        None,
        crate::assurance::ListIrRunsResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/ir/runs/{id}",
        "getIrRun",
        "memory:read",
        true,
        true,
        None,
        crate::assurance::IrRunResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/ir/runs/{id}/events",
        "getIrRunEvents",
        "memory:read",
        true,
        true,
        None,
        crate::assurance::IrRunEventsResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/assurance/certificates",
        "listAssuranceCertificates",
        "certificates:read",
        true,
        true,
        None,
        crate::assurance::ListAssuranceCertificatesResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/assurance/certificates/{id}",
        "getAssuranceCertificate",
        "certificates:read",
        true,
        true,
        None,
        crate::assurance::AssuranceCertificateResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/assurance/certificates/{id}/bundle",
        "getAssuranceCertificateBundle",
        "certificates:read",
        true,
        true,
        None,
        crate::assurance::CertificateBundleResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/reasoning/certificates",
        "listCertificates",
        "certificates:read",
        true,
        true,
        None,
        crate::reasoning::CertificateListResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/memory/sources",
        "listMemorySources",
        "memory:read",
        true,
        true,
        None,
        crate::memory::ListSourcesResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/sources",
        "createMemorySource",
        "memory:write",
        true,
        true,
        Some(crate::memory::CreateSourceRequest),
        crate::memory::SourceResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/memory/sources/{id}",
        "getMemorySource",
        "memory:read",
        true,
        true,
        None,
        crate::memory::SourceResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/memory/evidence",
        "listMemoryEvidence",
        "memory:read",
        true,
        true,
        None,
        crate::memory::ListEvidenceResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/evidence",
        "createMemoryEvidence",
        "memory:write",
        true,
        true,
        Some(crate::memory::CreateEvidenceRequest),
        crate::memory::EvidenceResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/memory/evidence/{id}",
        "getMemoryEvidence",
        "memory:read",
        true,
        true,
        None,
        crate::memory::EvidenceResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/memory/entities",
        "listMemoryEntities",
        "memory:read",
        true,
        true,
        None,
        crate::memory::ListEntitiesResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/entities",
        "createMemoryEntity",
        "memory:write",
        true,
        true,
        Some(crate::memory::CreateEntityRequest),
        crate::memory::EntityResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/memory/entities/{id}",
        "getMemoryEntity",
        "memory:read",
        true,
        true,
        None,
        crate::memory::EntityResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/memory/claims",
        "listMemoryClaims",
        "memory:read",
        true,
        true,
        None,
        crate::memory::ListClaimsResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/claims",
        "createMemoryClaim",
        "memory:write",
        true,
        true,
        Some(crate::memory::CreateClaimRequest),
        crate::memory::ClaimResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/memory/claims/{id}",
        "getMemoryClaim",
        "memory:read",
        true,
        true,
        None,
        crate::memory::ClaimResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/memory/events",
        "listMemoryEvents",
        "memory:read",
        true,
        true,
        None,
        crate::memory::ListEventsResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/events",
        "createMemoryEvent",
        "memory:write",
        true,
        true,
        Some(crate::memory::CreateEventRequest),
        crate::memory::EventResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/memory/events/{id}",
        "getMemoryEvent",
        "memory:read",
        true,
        true,
        None,
        crate::memory::EventResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/memory/relationships",
        "listMemoryRelationships",
        "memory:read",
        true,
        true,
        None,
        crate::memory::ListRelationshipsResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/relationships",
        "createMemoryRelationship",
        "memory:write",
        true,
        true,
        Some(crate::memory::CreateRelationshipRequest),
        crate::memory::RelationshipResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/memory/relationships/{id}",
        "getMemoryRelationship",
        "memory:read",
        true,
        true,
        None,
        crate::memory::RelationshipResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/memory/contracts",
        "listMemoryContracts",
        "memory:read",
        true,
        true,
        None,
        crate::memory::ListMemoryContractsResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/contracts",
        "createMemoryContract",
        "memory:write",
        true,
        true,
        Some(crate::memory::CreateMemoryContractRequest),
        crate::memory::MemoryContractResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/memory/contracts/{id}",
        "getMemoryContract",
        "memory:read",
        true,
        true,
        None,
        crate::memory::MemoryContractResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/memory/subgraphs",
        "listMemorySubgraphs",
        "memory:read",
        true,
        true,
        None,
        crate::memory::ListMemorySubgraphsResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/subgraphs",
        "createMemorySubgraph",
        "memory:write",
        true,
        true,
        Some(crate::memory::CreateMemorySubgraphRequest),
        crate::memory::MemorySubgraphResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/memory/subgraphs/{id}",
        "getMemorySubgraph",
        "memory:read",
        true,
        true,
        None,
        crate::memory::MemorySubgraphResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/subgraphs/{id}/activate",
        "activateMemorySubgraph",
        "memory:write",
        true,
        true,
        Some(crate::memory::ActivateMemorySubgraphRequest),
        crate::memory::MemorySubgraphResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/subgraphs/{id}/members",
        "createMemorySubgraphMember",
        "memory:write",
        true,
        true,
        Some(crate::memory::CreateMemorySubgraphMemberRequest),
        crate::memory::MemorySubgraphMemberResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/memory/canonical-entities",
        "listCanonicalEntities",
        "memory:read",
        true,
        true,
        None,
        crate::memory::ListCanonicalEntitiesResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/canonical-entities",
        "createCanonicalEntity",
        "memory:write",
        true,
        true,
        Some(crate::memory::CreateCanonicalEntityRequest),
        crate::memory::CanonicalEntityResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/memory/entity-resolutions",
        "listEntityResolutions",
        "memory:read",
        true,
        true,
        None,
        crate::memory::ListEntityResolutionsResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/entity-resolutions",
        "createEntityResolution",
        "memory:write",
        true,
        true,
        Some(crate::memory::CreateEntityResolutionRequest),
        crate::memory::EntityResolutionResponse,
        "201"
    ),
    endpoint!(
        "POST",
        "/v1/memory/entity-resolutions/{id}/confirm",
        "confirmEntityResolution",
        "memory:write",
        true,
        true,
        None,
        crate::memory::EntityResolutionResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/entity-resolutions/{id}/reject",
        "rejectEntityResolution",
        "memory:write",
        true,
        true,
        None,
        crate::memory::EntityResolutionResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/memory/conflicts",
        "listClaimConflicts",
        "memory:read",
        true,
        true,
        None,
        crate::memory::ListClaimConflictsResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/conflicts/{id}/resolve",
        "resolveClaimConflict",
        "memory:write",
        true,
        true,
        Some(crate::memory::ResolveClaimConflictRequest),
        crate::memory::ClaimConflictResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/conflicts/{id}/dismiss",
        "dismissClaimConflict",
        "memory:write",
        true,
        true,
        None,
        crate::memory::ClaimConflictResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/summary-traces",
        "createSummaryTrace",
        "memory:write",
        true,
        true,
        Some(crate::memory::CreateSummaryTraceRequest),
        crate::memory::SummaryTraceResponse,
        "201"
    ),
    endpoint!(
        "POST",
        "/v1/memory/entity-graph-attachments",
        "createEntityGraphAttachment",
        "memory:write",
        true,
        true,
        Some(crate::memory::CreateEntityGraphAttachmentRequest),
        crate::memory::EntityGraphAttachmentResponse,
        "201"
    ),
    endpoint!(
        "POST",
        "/v1/memory/subgraph-edges",
        "createSubgraphEdge",
        "memory:write",
        true,
        true,
        Some(crate::memory::CreateSubgraphEdgeRequest),
        crate::memory::SubgraphEdgeResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/ingestion/connectors",
        "listIngestionConnectors",
        "connectors:read",
        true,
        true,
        None,
        crate::ingestion::ListIngestionConnectorsResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/ingestion/connectors",
        "createIngestionConnector",
        "connectors:write",
        true,
        true,
        Some(crate::ingestion::CreateIngestionConnectorRequest),
        crate::ingestion::IngestionConnectorResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/ingestion/connectors/{id}",
        "getIngestionConnector",
        "connectors:read",
        true,
        true,
        None,
        crate::ingestion::IngestionConnectorResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/ingestion/connectors/{id}/runs",
        "runIngestionConnector",
        "ingestion:operate",
        true,
        true,
        None,
        crate::ingestion::IngestionRunWithOutputsResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/ingestion/runs",
        "listIngestionRuns",
        "ingestion:read",
        true,
        true,
        None,
        crate::ingestion::ListIngestionRunsResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/ingestion/runs/{id}",
        "getIngestionRun",
        "ingestion:read",
        true,
        true,
        None,
        crate::ingestion::IngestionRunWithOutputsResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/ingestion/review/claims",
        "listIngestionReviewClaims",
        "review:read",
        true,
        true,
        None,
        crate::ingestion::ListReviewClaimsResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/ingestion/review/claims/{id}/approve",
        "approveIngestionReviewClaim",
        "review:write",
        true,
        true,
        None,
        crate::ingestion::ReviewClaimResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/ingestion/review/claims/{id}/reject",
        "rejectIngestionReviewClaim",
        "review:write",
        true,
        true,
        None,
        crate::ingestion::ReviewClaimResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/automations",
        "listAutomations",
        "automations:read",
        true,
        true,
        None,
        crate::models::ListAutomationsResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/automations",
        "createAutomation",
        "automations:write",
        true,
        true,
        Some(crate::models::CreateAutomationRequest),
        crate::models::AutomationResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/automations/{id}",
        "getAutomation",
        "automations:read",
        true,
        true,
        None,
        crate::models::AutomationResponse,
        "200"
    ),
    endpoint!(
        "PUT",
        "/v1/automations/{id}",
        "updateAutomation",
        "automations:write",
        true,
        true,
        Some(crate::models::CreateAutomationRequest),
        crate::models::AutomationResponse,
        "200"
    ),
    endpoint!(
        "DELETE",
        "/v1/automations/{id}",
        "deleteAutomation",
        "automations:write",
        true,
        true,
        None,
        crate::wire_schemas::EmptyResponse,
        "204"
    ),
    endpoint!(
        "POST",
        "/v1/automations/{id}/enable",
        "enableAutomation",
        "automations:operate",
        true,
        true,
        None,
        crate::models::AutomationResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/automations/{id}/disable",
        "disableAutomation",
        "automations:operate",
        true,
        true,
        None,
        crate::models::AutomationResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/automations/{id}/triggers",
        "listAutomationTriggers",
        "automations:read",
        true,
        true,
        None,
        crate::models::ListAutomationTriggersResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/automations/{id}/trigger",
        "triggerAutomation",
        "automations:operate",
        true,
        true,
        None,
        crate::models::WorkflowRunResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/trigger-plugins",
        "listTriggerPlugins",
        "automations:read",
        true,
        true,
        None,
        crate::models::ListTriggerPluginsResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/trigger-plugins",
        "createTriggerPlugin",
        "automations:write",
        true,
        true,
        Some(crate::models::CreateTriggerPluginRequest),
        crate::models::TriggerPluginResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/trigger-plugins/{id}",
        "getTriggerPlugin",
        "automations:read",
        true,
        true,
        None,
        crate::models::TriggerPluginResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/workflow-runs",
        "listWorkflowRuns",
        "workflows:read",
        true,
        true,
        None,
        crate::models::ListWorkflowRunsResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/events/stream",
        "streamActivityEvents",
        "system:read",
        true,
        true,
        None,
        crate::wire_schemas::EventStreamResponse,
        "200",
        "text/event-stream"
    ),
    endpoint!(
        "GET",
        "/v1/workflow-runs/{id}",
        "getWorkflowRun",
        "workflows:read",
        true,
        true,
        None,
        crate::models::WorkflowRunResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/workflow-runs/{id}/logs",
        "getWorkflowRunLogs",
        "workflows:read",
        true,
        true,
        None,
        crate::models::WorkflowRunLogsResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/workflow-runs/{id}/logs/stream",
        "streamWorkflowRunLogs",
        "workflows:read",
        true,
        true,
        None,
        crate::wire_schemas::EventStreamResponse,
        "200",
        "text/event-stream"
    ),
    endpoint!(
        "POST",
        "/v1/workflow-runs/{id}/remove",
        "removeWorkflowRun",
        "workflows:operate",
        true,
        true,
        None,
        crate::models::WorkflowRunResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/workflow-runs/{id}/cancel",
        "cancelWorkflowRun",
        "workflows:operate",
        true,
        true,
        None,
        crate::models::WorkflowRunResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/workflow-runs/{id}/resume",
        "resumeWorkflowRun",
        "workflows:operate",
        true,
        true,
        None,
        crate::models::WorkflowRunResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/jobs/runs",
        "listJobRuns",
        "jobs:read",
        true,
        true,
        None,
        crate::models::ListRunsResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/jobs/runs",
        "createJobRun",
        "jobs:run",
        true,
        true,
        Some(crate::models::CreateRunRequest),
        crate::models::JobRunResponse,
        "201"
    ),
    endpoint!(
        "GET",
        "/v1/jobs/runs/{id}",
        "getJobRun",
        "jobs:read",
        true,
        true,
        None,
        crate::models::JobRunResponse,
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/jobs/runs/{id}/cancel",
        "cancelJobRun",
        "jobs:cancel",
        true,
        true,
        None,
        crate::models::JobRunResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/jobs/runs/{id}/logs",
        "getJobRunLogs",
        "jobs:read",
        true,
        true,
        None,
        crate::models::JobRunLogsResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/jobs/runs/{id}/logs/stream",
        "streamJobRunLogs",
        "jobs:read",
        true,
        true,
        None,
        crate::wire_schemas::EventStreamResponse,
        "200",
        "text/event-stream"
    ),
    endpoint!(
        "GET",
        "/v1/jobs/runs/{id}/artifacts",
        "listJobArtifacts",
        "jobs:read",
        true,
        true,
        None,
        crate::models::ListArtifactsResponse,
        "200"
    ),
    endpoint!(
        "GET",
        "/v1/jobs/runs/{id}/artifacts/{artifact_id}",
        "downloadJobArtifact",
        "jobs:read",
        true,
        true,
        None,
        crate::wire_schemas::BinaryResponse,
        "200",
        "application/octet-stream"
    ),
];

/// Returns all public operation contracts.
#[must_use]
pub const fn endpoint_contracts() -> &'static [EndpointContract] {
    ENDPOINTS
}

/// Finds a contract by its stable operation identifier.
#[must_use]
pub fn find_operation(operation_id: &str) -> Option<&'static EndpointContract> {
    ENDPOINTS
        .iter()
        .find(|endpoint| endpoint.operation_id == operation_id)
}

/// Finds a contract by concrete request path.
#[must_use]
pub fn find_endpoint(method: &str, concrete_path: &str) -> Option<&'static EndpointContract> {
    ENDPOINTS.iter().find(|endpoint| {
        endpoint.method.as_str() == method && path_matches(endpoint.path, concrete_path)
    })
}

fn path_matches(template: &str, concrete: &str) -> bool {
    let template_segments = template.split('/');
    let concrete_segments = concrete.split('/');
    template_segments
        .zip(concrete_segments)
        .all(|(expected, actual)| {
            (expected.starts_with('{') && expected.ends_with('}') && !actual.is_empty())
                || expected == actual
        })
        && template.split('/').count() == concrete.split('/').count()
}

/// Returns distinct literal path templates for parity reporting.
#[must_use]
pub fn distinct_paths() -> BTreeSet<&'static str> {
    ENDPOINTS.iter().map(|endpoint| endpoint.path).collect()
}

/// Generates the runtime role matrix without a second policy authority.
#[must_use]
pub fn authorization_matrix_markdown() -> String {
    use std::fmt::Write as _;
    let mut document = String::from(
        "# Authorization matrix\n\nGenerated from typed endpoint policies. Regenerate with `cargo run -p capsulet-api --bin export-authorization`.\n\nAuthentication, token scope, and the role in the selected project must all permit an operation.\nDatabase service accounts never grant platform-admin rights; explicit scopes are a ceiling.\nAn unknown role grants nothing. Membership endpoints select their project from the path.\nResource reads with missing or mismatched ownership return a generic `404`; selecting a project\nwithout membership or lacking action permission returns `403`. Webhooks require their own signature.\n\n| Operation | Method | Path | Permission | Ownership | Viewer | Operator | Admin |\n| --- | --- | --- | --- | --- | --- | --- | --- |\n",
    );
    for endpoint in ENDPOINTS {
        let policy = endpoint.policy;
        let allowed = |role| {
            if policy.allows(role, false) {
                "allow"
            } else {
                "deny"
            }
        };
        writeln!(
            &mut document,
            "| `{}` | {} | `{}` | `{}` | {} | {} | {} | {} |",
            endpoint.operation_id,
            endpoint.method,
            endpoint.path,
            policy.permission.as_str(),
            policy.ownership.as_str(),
            allowed(ProjectRole::Viewer),
            allowed(ProjectRole::Operator),
            allowed(ProjectRole::Admin)
        )
        .expect("writing into String cannot fail");
    }
    document
}
