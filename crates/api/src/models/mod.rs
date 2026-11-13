//! The request and response bodies of the control-plane routes, by area.
//!
//! Each type derives its schema, so the `OpenAPI` document is generated from these
//! and never written by hand. What several areas share, such as the list limit, is
//! here.

mod agents;
mod automations;
mod graphs;
mod identity;
mod jobs;
mod workflows;

pub(crate) use self::agents::{
    AgentResponse, AgentRunBundleResponse, AgentRunErasureResponse, AgentRunExplanationResponse,
    AgentRunResponse, AgentRunUsageResponse, CheckRuleResponse, DecisionReplayResponse,
    DivergenceResponse, EffectExplanationResponse, EvidenceDigestResponse, ExplainedEventResponse,
    ListAgentRunsResponse, ListAgentsResponse, NodeExplanationResponse,
    termination_policy_from_conditions,
};
pub use self::agents::{CreateAgentRequest, DecideAgentRunRequest, StartAgentRunRequest};
pub(crate) use self::automations::{
    AutomationResponse, ListAutomationTriggersResponse, ListAutomationsResponse,
    ListTriggerPluginsResponse, TriggerPluginResponse, TriggerResponse,
};
pub use self::automations::{
    CreateAutomationRequest, CreateAutomationTriggerRequest, CreateTriggerPluginRequest,
};
pub use self::graphs::CreateGraphRequest;
pub(crate) use self::graphs::{GraphResponse, ListGraphsResponse};
pub(crate) use self::identity::{
    AuditEventResponse, CreateServiceAccountRequest, HealthResponse, ListAuditEventsResponse,
    ListProjectMembershipsResponse, ListProjectsResponse, ListServiceAccountsResponse,
    ProjectMembershipResponse, ProjectResponse, UpsertProjectMembershipRequest,
    UpsertProjectRequest,
};
pub use self::identity::{CreateServiceAccountResponse, PrincipalResponse, ServiceAccountResponse};
pub(crate) use self::jobs::{
    ArtifactResponse, ExecutionPoolResponse, HostGroupResponse, JobDefinitionResponse,
    JobDefinitionSourceResponse, JobRunLogsResponse, JobRunResponse, ListArtifactsResponse,
    ListExecutionPoolsResponse, ListHostGroupsResponse, ListJobDefinitionsQuery,
    ListJobDefinitionsResponse, ListRunsQuery, ListRunsResponse,
};
pub use self::jobs::{CreateJobDefinitionRequest, CreateRunRequest};
pub use self::workflows::{
    CreateWorkflowDependencyRequest, CreateWorkflowRequest, CreateWorkflowStepRequest,
};
pub(crate) use self::workflows::{
    ListWorkflowRunsQuery, ListWorkflowRunsResponse, ListWorkflowsResponse, TopologyEdgeResponse,
    TopologyNodeResponse, TopologyResponse, WorkflowEditabilityResponse, WorkflowResponse,
    WorkflowRunLogEntryResponse, WorkflowRunLogsResponse, WorkflowRunResponse,
};

/// The most any listing returns; a larger `limit` is clamped to it.
pub(crate) const LIST_LIMIT_MAX: u16 = 500;
