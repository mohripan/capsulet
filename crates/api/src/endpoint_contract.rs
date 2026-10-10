//! Stable metadata for every public HTTP operation.

use std::{collections::BTreeSet, fmt};

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

/// Identifier for a registered Rust wire schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SchemaId(&'static str);

impl SchemaId {
    const fn new(name: &'static str) -> Self {
        Self(name)
    }

    /// Returns the stable component name emitted into `OpenAPI`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

const fn optional_schema(name: Option<&'static str>) -> Option<SchemaId> {
    match name {
        Some(name) => Some(SchemaId::new(name)),
        None => None,
    }
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
    ($method:tt, $path:literal, $operation:literal, $scope:tt, $project:tt, $auth:tt, $request:expr, $response:literal, $status:tt) => {
        EndpointContract {
            method: http_method!($method),
            path: $path,
            operation_id: $operation,
            stability: Stability::Experimental,
            required_permission: required_permission!($scope),
            project_context: project_context_rule!($project),
            policy: EndpointPolicy::new(required_permission!($scope), ownership_rule!($project)),
            authentication: authentication_mode!($auth),
            request_schema: optional_schema($request),
            response_schema: SchemaId::new($response),
            error_contract: ErrorContract::Json,
            success_status: response_status!($status),
            response_content_type: MediaType::Json,
        }
    };
    ($method:tt, $path:literal, $operation:literal, $scope:tt, $project:tt, $auth:tt, $request:expr, $response:literal, $status:tt, $content:tt) => {
        EndpointContract {
            response_content_type: media_type!($content),
            ..endpoint!(
                $method, $path, $operation, $scope, $project, $auth, $request, $response, $status
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
        "HealthResponse",
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
        "HealthResponse",
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
        "HealthResponse",
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
        "StringResponse",
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
        "OpenApiDocument",
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
        Some("WebhookRequest"),
        "WebhookResponse",
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
        "PrincipalResponse",
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
        "ListProjectsResponse",
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
        "ListProjectMembershipsResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/projects/{project_id}/memberships",
        "upsertProjectMembership",
        "project-members:write",
        path,
        true,
        Some("UpsertProjectMembershipRequest"),
        "ProjectMembershipResponse",
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
        "EmptyResponse",
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
        "ListServiceAccountsResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/service-accounts",
        "createServiceAccount",
        "service-accounts:write",
        true,
        true,
        Some("CreateServiceAccountRequest"),
        "CreateServiceAccountResponse",
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
        "EmptyResponse",
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
        "ListJobDefinitionsResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/job-definitions",
        "createJobDefinition",
        "jobs:write",
        true,
        true,
        Some("CreateJobDefinitionRequest"),
        "JobDefinitionResponse",
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
        "JobDefinitionResponse",
        "200"
    ),
    endpoint!(
        "PUT",
        "/v1/job-definitions/{id}",
        "updateJobDefinition",
        "jobs:write",
        true,
        true,
        Some("CreateJobDefinitionRequest"),
        "JobDefinitionResponse",
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
        "EmptyResponse",
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
        "JobDefinitionSourceResponse",
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
        "ListExecutionPoolsResponse",
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
        "ListAuditEventsResponse",
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
        "ListHostGroupsResponse",
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
        "TopologyResponse",
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
        "ListWorkflowsResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/workflows",
        "createWorkflow",
        "workflows:write",
        true,
        true,
        Some("CreateWorkflowRequest"),
        "WorkflowResponse",
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
        "WorkflowResponse",
        "200"
    ),
    endpoint!(
        "PUT",
        "/v1/workflows/{id}",
        "updateWorkflow",
        "workflows:write",
        true,
        true,
        Some("CreateWorkflowRequest"),
        "WorkflowResponse",
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
        "EmptyResponse",
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
        "WorkflowEditabilityResponse",
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
        "ListGraphsResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/graphs",
        "createGraph",
        "graphs:write",
        true,
        true,
        Some("CreateGraphRequest"),
        "GraphResponse",
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
        "GraphResponse",
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
        "ListAgentsResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/agents",
        "createAgent",
        "agents:write",
        true,
        true,
        Some("CreateAgentRequest"),
        "AgentResponse",
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
        "AgentResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/agents/{id}/runs",
        "startAgentRun",
        "agents:operate",
        true,
        true,
        Some("StartAgentRunRequest"),
        "AgentRunResponse",
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
        "ListAgentRunsResponse",
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
        "AgentRunResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/reasoning/ask",
        "askReasoning",
        "reasoning:operate",
        true,
        true,
        Some("AskRequest"),
        "CertificateResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/ir/definitions",
        "registerIrDefinition",
        "memory:write",
        true,
        true,
        Some("RegisterDefinitionRequest"),
        "RegisterDefinitionResponse",
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
        "ListDefinitionVersionsResponse",
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
        "DefinitionVersionResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/ir/runs",
        "startIrRun",
        "jobs:run",
        true,
        true,
        Some("StartIrRunRequest"),
        "IrRunResponse",
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
        "ListIrRunsResponse",
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
        "IrRunResponse",
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
        "IrRunEventsResponse",
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
        "ListAssuranceCertificatesResponse",
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
        "AssuranceCertificateResponse",
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
        "CertificateBundleResponse",
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
        "ListCertificatesResponse",
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
        "ListMemorySourcesResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/sources",
        "createMemorySource",
        "memory:write",
        true,
        true,
        Some("CreateMemorySourceRequest"),
        "MemorySourceResponse",
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
        "MemorySourceResponse",
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
        "ListMemoryEvidenceResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/evidence",
        "createMemoryEvidence",
        "memory:write",
        true,
        true,
        Some("CreateMemoryEvidenceRequest"),
        "MemoryEvidenceResponse",
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
        "MemoryEvidenceResponse",
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
        "ListMemoryEntitiesResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/entities",
        "createMemoryEntity",
        "memory:write",
        true,
        true,
        Some("CreateMemoryEntityRequest"),
        "MemoryEntityResponse",
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
        "MemoryEntityResponse",
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
        "ListMemoryClaimsResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/claims",
        "createMemoryClaim",
        "memory:write",
        true,
        true,
        Some("CreateMemoryClaimRequest"),
        "MemoryClaimResponse",
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
        "MemoryClaimResponse",
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
        "ListMemoryEventsResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/events",
        "createMemoryEvent",
        "memory:write",
        true,
        true,
        Some("CreateMemoryEventRequest"),
        "MemoryEventResponse",
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
        "MemoryEventResponse",
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
        "ListMemoryRelationshipsResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/relationships",
        "createMemoryRelationship",
        "memory:write",
        true,
        true,
        Some("CreateMemoryRelationshipRequest"),
        "MemoryRelationshipResponse",
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
        "MemoryRelationshipResponse",
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
        "ListMemoryContractsResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/contracts",
        "createMemoryContract",
        "memory:write",
        true,
        true,
        Some("CreateMemoryContractRequest"),
        "MemoryContractResponse",
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
        "MemoryContractResponse",
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
        "ListMemorySubgraphsResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/subgraphs",
        "createMemorySubgraph",
        "memory:write",
        true,
        true,
        Some("CreateMemorySubgraphRequest"),
        "MemorySubgraphResponse",
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
        "MemorySubgraphResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/subgraphs/{id}/activate",
        "activateMemorySubgraph",
        "memory:write",
        true,
        true,
        Some("ActivateMemorySubgraphRequest"),
        "MemorySubgraphResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/subgraphs/{id}/members",
        "createMemorySubgraphMember",
        "memory:write",
        true,
        true,
        Some("CreateMemorySubgraphMemberRequest"),
        "MemorySubgraphMemberResponse",
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
        "ListCanonicalEntitiesResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/canonical-entities",
        "createCanonicalEntity",
        "memory:write",
        true,
        true,
        Some("CreateCanonicalEntityRequest"),
        "CanonicalEntityResponse",
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
        "ListEntityResolutionsResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/entity-resolutions",
        "createEntityResolution",
        "memory:write",
        true,
        true,
        Some("CreateEntityResolutionRequest"),
        "EntityResolutionResponse",
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
        "EntityResolutionResponse",
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
        "EntityResolutionResponse",
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
        "ListClaimConflictsResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/conflicts/{id}/resolve",
        "resolveClaimConflict",
        "memory:write",
        true,
        true,
        Some("ResolveClaimConflictRequest"),
        "ClaimConflictResponse",
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
        "ClaimConflictResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/memory/summary-traces",
        "createSummaryTrace",
        "memory:write",
        true,
        true,
        Some("CreateSummaryTraceRequest"),
        "SummaryTraceResponse",
        "201"
    ),
    endpoint!(
        "POST",
        "/v1/memory/entity-graph-attachments",
        "createEntityGraphAttachment",
        "memory:write",
        true,
        true,
        Some("CreateEntityGraphAttachmentRequest"),
        "EntityGraphAttachmentResponse",
        "201"
    ),
    endpoint!(
        "POST",
        "/v1/memory/subgraph-edges",
        "createSubgraphEdge",
        "memory:write",
        true,
        true,
        Some("CreateSubgraphEdgeRequest"),
        "SubgraphEdgeResponse",
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
        "ListIngestionConnectorsResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/ingestion/connectors",
        "createIngestionConnector",
        "connectors:write",
        true,
        true,
        Some("CreateIngestionConnectorRequest"),
        "IngestionConnectorResponse",
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
        "IngestionConnectorResponse",
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
        "IngestionRunWithOutputsResponse",
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
        "ListIngestionRunsResponse",
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
        "IngestionRunWithOutputsResponse",
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
        "ListIngestionReviewClaimsResponse",
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
        "ReviewClaimResponse",
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
        "ReviewClaimResponse",
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
        "ListAutomationsResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/automations",
        "createAutomation",
        "automations:write",
        true,
        true,
        Some("CreateAutomationRequest"),
        "AutomationResponse",
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
        "AutomationResponse",
        "200"
    ),
    endpoint!(
        "PUT",
        "/v1/automations/{id}",
        "updateAutomation",
        "automations:write",
        true,
        true,
        Some("CreateAutomationRequest"),
        "AutomationResponse",
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
        "EmptyResponse",
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
        "AutomationResponse",
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
        "AutomationResponse",
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
        "ListAutomationTriggersResponse",
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
        "WorkflowRunResponse",
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
        "ListTriggerPluginsResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/trigger-plugins",
        "createTriggerPlugin",
        "automations:write",
        true,
        true,
        Some("CreateTriggerPluginRequest"),
        "TriggerPluginResponse",
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
        "TriggerPluginResponse",
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
        "ListWorkflowRunsResponse",
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
        "EventStreamResponse",
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
        "WorkflowRunResponse",
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
        "WorkflowRunLogsResponse",
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
        "EventStreamResponse",
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
        "WorkflowRunResponse",
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
        "WorkflowRunResponse",
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
        "WorkflowRunResponse",
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
        "ListRunsResponse",
        "200"
    ),
    endpoint!(
        "POST",
        "/v1/jobs/runs",
        "createJobRun",
        "jobs:run",
        true,
        true,
        Some("CreateRunRequest"),
        "JobRunResponse",
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
        "JobRunResponse",
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
        "JobRunResponse",
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
        "JobRunLogsResponse",
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
        "EventStreamResponse",
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
        "ListArtifactsResponse",
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
        "BinaryResponse",
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
