use capsulet_application::AgentRunRecord;
use capsulet_core::{
    AgentBudget, AgentDefinition, AgentTerminationPolicy, Automation, AutomationTrigger,
    CustomTriggerPlugin, GraphDefinition, GraphHyperedge, GraphNode, GraphPort,
    GraphTransitionMode, GraphTransitionPolicy, HyperedgeEndpoint, HyperedgeId, JobArtifact,
    JobDefinition, JobRun, JobRunStatus, NodeId, NodeKind, PortDirection, PortId, PortValueType,
    TerminationCondition, WorkflowDefinition, WorkflowRun, WorkflowStep, WorkflowStepDependency,
    WorkflowStepRun,
};
use capsulet_postgres::{ProjectMembershipRecord, ProjectRecord, ServiceAccountRecord};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{error::ApiError, http::json_from_string};

#[derive(Debug, Serialize)]
pub(crate) struct HealthResponse {
    pub(crate) status: &'static str,
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Serialize)]
pub(crate) struct ServiceAccountResponse {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) tenant_id: String,
    pub(crate) project_id: String,
    pub(crate) role: String,
    pub(crate) scopes: Vec<String>,
    pub(crate) expires_at: Option<String>,
    pub(crate) revoked_at: Option<String>,
    pub(crate) last_used_at: Option<String>,
    pub(crate) created_at: String,
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

#[derive(Debug, Serialize)]
pub(crate) struct CreateServiceAccountResponse {
    #[serde(flatten)]
    pub(crate) account: ServiceAccountResponse,
    pub(crate) token: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListServiceAccountsResponse {
    pub(crate) service_accounts: Vec<ServiceAccountResponse>,
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
pub(crate) struct ListProjectsResponse {
    pub(crate) projects: Vec<ProjectResponse>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UpsertProjectMembershipRequest {
    pub(crate) principal_kind: String,
    pub(crate) principal_name: String,
    pub(crate) role: String,
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
pub(crate) struct ListProjectMembershipsResponse {
    pub(crate) memberships: Vec<ProjectMembershipResponse>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRunRequest {
    pub job_definition_id: String,
    #[serde(alias = "host_group")]
    pub execution_pool: String,
    pub run_id: Option<String>,
    pub python_script: Option<String>,
    pub input: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct CreateJobDefinitionRequest {
    pub id: Option<String>,
    pub name: String,
    pub runtime_image: Option<String>,
    pub python_script: String,
    #[serde(default)]
    pub python_dependencies: Vec<String>,
    #[serde(default)]
    pub input_schema: Option<Value>,
    pub retry_max_attempts: Option<u32>,
    pub retry_delay_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkflowRequest {
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub steps: Vec<CreateWorkflowStepRequest>,
    pub dependencies: Option<Vec<CreateWorkflowDependencyRequest>>,
    pub deadline_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateGraphRequest {
    pub id: Option<String>,
    pub name: String,
    pub nodes: Vec<GraphNodeRequest>,
    pub hyperedges: Vec<GraphHyperedgeRequest>,
    pub transition_policy: Option<GraphTransitionPolicyRequest>,
}

#[derive(Debug, Deserialize)]
pub struct GraphNodeRequest {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub ports: Vec<GraphPortRequest>,
}

#[derive(Debug, Deserialize)]
pub struct GraphPortRequest {
    pub id: String,
    pub direction: String,
    pub value_type: String,
}

#[derive(Debug, Deserialize)]
pub struct GraphHyperedgeRequest {
    pub id: String,
    pub sources: Vec<HyperedgeEndpointRequest>,
    pub targets: Vec<HyperedgeEndpointRequest>,
}

#[derive(Debug, Deserialize)]
pub struct HyperedgeEndpointRequest {
    pub kind: String,
    pub node_id: Option<String>,
    pub port_id: Option<String>,
    pub field: Option<String>,
    pub value_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GraphTransitionPolicyRequest {
    pub mode: String,
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(default)]
    pub cycles_allowed: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub id: Option<String>,
    pub name: String,
    pub graph_id: String,
    pub budget: AgentBudgetRequest,
    #[serde(default)]
    pub termination_conditions: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct AgentBudgetRequest {
    #[serde(rename = "max_steps")]
    pub steps: u32,
    #[serde(rename = "max_tokens")]
    pub tokens: u64,
    #[serde(rename = "max_seconds")]
    pub seconds: u64,
    #[serde(rename = "max_cost_micros")]
    pub cost_micros: u64,
}

#[derive(Debug, Deserialize)]
pub struct StartAgentRunRequest {
    pub id: Option<String>,
    #[serde(default)]
    pub initial_state: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct JobDefinitionSourceResponse {
    pub(crate) python_script: String,
    pub(crate) python_dependencies: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkflowEditabilityResponse {
    pub(crate) editable: bool,
    pub(crate) reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TopologyResponse {
    pub(crate) nodes: Vec<TopologyNodeResponse>,
    pub(crate) edges: Vec<TopologyEdgeResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TopologyNodeResponse {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) kind: String,
    pub(crate) status: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct TopologyEdgeResponse {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) label: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkflowStepRequest {
    pub id: Option<String>,
    pub name: String,
    pub job_definition_id: String,
    #[serde(alias = "host_group")]
    pub execution_pool: String,
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAutomationRequest {
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub workflow_id: String,
    pub status: Option<String>,
    pub job_input: Option<Value>,
    pub triggers: Option<Vec<CreateAutomationTriggerRequest>>,
    pub condition: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAutomationTriggerRequest {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub config: Value,
    pub plugin_id: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTriggerPluginRequest {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub runtime_image: String,
    pub command: Option<Vec<String>>,
    pub python_script: Option<String>,
    pub config_schema: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListRunsQuery {
    pub(crate) limit: Option<u16>,
    pub(crate) start_at: Option<String>,
    pub(crate) end_at: Option<String>,
    pub(crate) q: Option<String>,
    pub(crate) state: Option<String>,
    pub(crate) sort: Option<String>,
    pub(crate) direction: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkflowDependencyRequest {
    pub from_step_id: String,
    pub to_step_id: String,
    pub policy: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListWorkflowRunsQuery {
    pub(crate) limit: Option<u16>,
    pub(crate) start_at: Option<String>,
    pub(crate) end_at: Option<String>,
    pub(crate) q: Option<String>,
    pub(crate) state: Option<String>,
    pub(crate) sort: Option<String>,
    pub(crate) direction: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListJobDefinitionsQuery {
    pub(crate) limit: Option<u16>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListRunsResponse {
    pub(crate) runs: Vec<JobRunResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListJobDefinitionsResponse {
    pub(crate) job_definitions: Vec<JobDefinitionResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListExecutionPoolsResponse {
    pub(crate) execution_pools: Vec<ExecutionPoolResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListHostGroupsResponse {
    pub(crate) host_groups: Vec<HostGroupResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListWorkflowsResponse {
    pub(crate) workflows: Vec<WorkflowResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListGraphsResponse {
    pub(crate) graphs: Vec<GraphResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListAgentsResponse {
    pub(crate) agents: Vec<AgentResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListAgentRunsResponse {
    pub(crate) agent_runs: Vec<AgentRunResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListAutomationsResponse {
    pub(crate) automations: Vec<AutomationResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListAutomationTriggersResponse {
    pub(crate) triggers: Vec<TriggerResponse>,
    pub(crate) condition: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListTriggerPluginsResponse {
    pub(crate) trigger_plugins: Vec<TriggerPluginResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListWorkflowRunsResponse {
    pub(crate) workflow_runs: Vec<WorkflowRunResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkflowRunLogsResponse {
    pub(crate) workflow_run_id: String,
    pub(crate) workflow_id: String,
    pub(crate) status: String,
    pub(crate) entries: Vec<WorkflowRunLogEntryResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkflowRunLogEntryResponse {
    pub(crate) step_run_id: String,
    pub(crate) workflow_step_id: String,
    pub(crate) job_run_id: Option<String>,
    pub(crate) position: i32,
    pub(crate) status: String,
    pub(crate) logs: String,
    pub(crate) object_log_available: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct ExecutionPoolResponse {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) is_default: bool,
    pub(crate) host_group: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct HostGroupResponse {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) is_default: bool,
    pub(crate) execution_pool: String,
    pub(crate) host_count: Option<u32>,
}

#[derive(Debug, Serialize)]
pub(crate) struct JobDefinitionResponse {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) runtime_image: String,
    pub(crate) command: Vec<String>,
    pub(crate) python_dependencies: Vec<String>,
    pub(crate) bundle_object_key: String,
    pub(crate) input_schema: Value,
    pub(crate) retry_max_attempts: u32,
    pub(crate) retry_delay_seconds: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkflowResponse {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) status: String,
    pub(crate) steps: Vec<WorkflowStepResponse>,
    pub(crate) dependencies: Vec<WorkflowDependencyResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct GraphResponse {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) nodes: Vec<GraphNodeResponse>,
    pub(crate) hyperedges: Vec<GraphHyperedgeResponse>,
    pub(crate) transition_policy: GraphTransitionPolicyResponse,
    pub(crate) static_order: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct GraphNodeResponse {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) ports: Vec<GraphPortResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct GraphPortResponse {
    pub(crate) id: String,
    pub(crate) direction: String,
    pub(crate) value_type: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct GraphHyperedgeResponse {
    pub(crate) id: String,
    pub(crate) sources: Vec<HyperedgeEndpointResponse>,
    pub(crate) targets: Vec<HyperedgeEndpointResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct HyperedgeEndpointResponse {
    pub(crate) kind: String,
    pub(crate) node_id: Option<String>,
    pub(crate) port_id: Option<String>,
    pub(crate) field: Option<String>,
    pub(crate) value_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct GraphTransitionPolicyResponse {
    pub(crate) mode: String,
    pub(crate) actions: Vec<String>,
    pub(crate) cycles_allowed: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct AgentResponse {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) graph_id: String,
    pub(crate) budget: AgentBudgetResponse,
    pub(crate) termination_conditions: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AgentBudgetResponse {
    #[serde(rename = "max_steps")]
    pub(crate) steps: u32,
    #[serde(rename = "max_tokens")]
    pub(crate) tokens: u64,
    #[serde(rename = "max_seconds")]
    pub(crate) seconds: u64,
    #[serde(rename = "max_cost_micros")]
    pub(crate) cost_micros: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct AgentRunResponse {
    pub(crate) id: String,
    pub(crate) agent_id: String,
    pub(crate) status: String,
    pub(crate) state_version: u64,
    pub(crate) state: Value,
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
pub(crate) struct ListAuditEventsResponse {
    pub(crate) audit_events: Vec<AuditEventResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkflowDependencyResponse {
    pub(crate) from_step_id: String,
    pub(crate) to_step_id: String,
    pub(crate) policy: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkflowStepResponse {
    pub(crate) id: String,
    pub(crate) position: i32,
    pub(crate) name: String,
    pub(crate) job_definition_id: String,
    pub(crate) execution_pool: String,
    pub(crate) host_group: String,
    pub(crate) timeout_seconds: Option<u64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AutomationResponse {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) workflow_id: String,
    pub(crate) status: String,
    pub(crate) triggers: Vec<TriggerResponse>,
    pub(crate) condition: Value,
    pub(crate) job_input: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct TriggerResponse {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) config: Value,
    pub(crate) plugin_id: Option<String>,
    pub(crate) enabled: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct TriggerPluginResponse {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) runtime_image: String,
    pub(crate) command: Vec<String>,
    pub(crate) python_script: String,
    pub(crate) config_schema: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkflowRunResponse {
    pub(crate) id: String,
    pub(crate) workflow_id: String,
    pub(crate) automation_id: Option<String>,
    pub(crate) status: String,
    pub(crate) current_step_position: i32,
    pub(crate) created_at: String,
    pub(crate) step_runs: Vec<WorkflowStepRunResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkflowStepRunResponse {
    pub(crate) id: String,
    pub(crate) workflow_step_id: String,
    pub(crate) job_run_id: Option<String>,
    pub(crate) position: i32,
    pub(crate) status: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct JobRunResponse {
    pub(crate) id: String,
    pub(crate) job_definition_id: String,
    pub(crate) status: String,
    pub(crate) execution_pool: String,
    pub(crate) host_group: String,
    pub(crate) attempt_count: u32,
    pub(crate) created_at: String,
    pub(crate) input: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct JobRunLogsResponse {
    pub(crate) run_id: String,
    pub(crate) logs: String,
    pub(crate) object_log_available: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListArtifactsResponse {
    pub(crate) artifacts: Vec<ArtifactResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ArtifactResponse {
    pub(crate) id: String,
    pub(crate) run_id: String,
    pub(crate) name: String,
    pub(crate) content_type: String,
    pub(crate) size_bytes: u64,
    pub(crate) kind: String,
}

impl From<&JobArtifact> for ArtifactResponse {
    fn from(artifact: &JobArtifact) -> Self {
        Self {
            id: artifact.id().as_str().to_string(),
            run_id: artifact.run_id().as_str().to_string(),
            name: artifact.name().to_string(),
            content_type: artifact.content_type().to_string(),
            size_bytes: artifact.size_bytes(),
            kind: artifact.kind().as_str().to_string(),
        }
    }
}

impl From<&JobDefinition> for JobDefinitionResponse {
    fn from(definition: &JobDefinition) -> Self {
        Self {
            id: definition.id().as_str().to_string(),
            name: definition.name().to_string(),
            runtime_image: definition.runtime_image().to_string(),
            command: definition.command().to_vec(),
            python_dependencies: definition.python_dependencies().to_vec(),
            bundle_object_key: definition.bundle_object_key().to_string(),
            input_schema: json_from_string(definition.input_schema()).unwrap_or_else(|_| json!({})),
            retry_max_attempts: definition.retry_max_attempts(),
            retry_delay_seconds: definition.retry_delay_seconds(),
        }
    }
}

impl From<&WorkflowDefinition> for WorkflowResponse {
    fn from(workflow: &WorkflowDefinition) -> Self {
        Self {
            id: workflow.id().as_str().to_string(),
            name: workflow.name().to_string(),
            description: workflow.description().to_string(),
            status: workflow.status().to_string(),
            steps: workflow
                .steps()
                .iter()
                .map(WorkflowStepResponse::from)
                .collect(),
            dependencies: workflow
                .dependencies()
                .iter()
                .map(WorkflowDependencyResponse::from)
                .collect(),
        }
    }
}

impl From<&WorkflowStepDependency> for WorkflowDependencyResponse {
    fn from(dependency: &WorkflowStepDependency) -> Self {
        Self {
            from_step_id: dependency.from_step_id().as_str().to_string(),
            to_step_id: dependency.to_step_id().as_str().to_string(),
            policy: dependency.policy().to_string(),
        }
    }
}

impl From<&WorkflowStep> for WorkflowStepResponse {
    fn from(step: &WorkflowStep) -> Self {
        Self {
            id: step.id().as_str().to_string(),
            position: step.position(),
            name: step.name().to_string(),
            job_definition_id: step.job_definition_id().as_str().to_string(),
            execution_pool: step.execution_pool().as_str().to_string(),
            host_group: step.execution_pool().as_str().to_string(),
            timeout_seconds: step.timeout_seconds(),
        }
    }
}

impl From<&GraphDefinition> for GraphResponse {
    fn from(graph: &GraphDefinition) -> Self {
        Self {
            id: graph.id().as_str().to_string(),
            name: graph.name().to_string(),
            nodes: graph.nodes().iter().map(GraphNodeResponse::from).collect(),
            hyperedges: graph
                .hyperedges()
                .iter()
                .map(GraphHyperedgeResponse::from)
                .collect(),
            transition_policy: GraphTransitionPolicyResponse::from(graph.transition_policy()),
            static_order: graph
                .static_order()
                .iter()
                .map(|node_id| node_id.as_str().to_string())
                .collect(),
        }
    }
}

impl From<&GraphNode> for GraphNodeResponse {
    fn from(node: &GraphNode) -> Self {
        Self {
            id: node.id().as_str().to_string(),
            name: node.name().to_string(),
            kind: node.kind().to_string(),
            ports: node.ports().iter().map(GraphPortResponse::from).collect(),
        }
    }
}

impl From<&GraphPort> for GraphPortResponse {
    fn from(port: &GraphPort) -> Self {
        Self {
            id: port.id().as_str().to_string(),
            direction: port.direction().to_string(),
            value_type: port.value_type().to_string(),
        }
    }
}

impl From<&GraphHyperedge> for GraphHyperedgeResponse {
    fn from(hyperedge: &GraphHyperedge) -> Self {
        Self {
            id: hyperedge.id().as_str().to_string(),
            sources: hyperedge
                .sources()
                .iter()
                .map(HyperedgeEndpointResponse::from)
                .collect(),
            targets: hyperedge
                .targets()
                .iter()
                .map(HyperedgeEndpointResponse::from)
                .collect(),
        }
    }
}

impl From<&HyperedgeEndpoint> for HyperedgeEndpointResponse {
    fn from(endpoint: &HyperedgeEndpoint) -> Self {
        match endpoint {
            HyperedgeEndpoint::Port { node_id, port_id } => Self {
                kind: "port".to_string(),
                node_id: Some(node_id.as_str().to_string()),
                port_id: Some(port_id.as_str().to_string()),
                field: None,
                value_type: None,
            },
            HyperedgeEndpoint::StateField { field, value_type } => Self {
                kind: "state_field".to_string(),
                node_id: None,
                port_id: None,
                field: Some(field.clone()),
                value_type: Some(value_type.to_string()),
            },
        }
    }
}

impl From<&GraphTransitionPolicy> for GraphTransitionPolicyResponse {
    fn from(policy: &GraphTransitionPolicy) -> Self {
        let (mode, actions) = match policy.mode() {
            GraphTransitionMode::Static => ("static".to_string(), Vec::new()),
            GraphTransitionMode::Planner { actions } => (
                "planner".to_string(),
                actions
                    .iter()
                    .map(|action| action.as_str().to_string())
                    .collect(),
            ),
        };
        Self {
            mode,
            actions,
            cycles_allowed: policy.cycles_allowed(),
        }
    }
}

impl From<&AgentDefinition> for AgentResponse {
    fn from(agent: &AgentDefinition) -> Self {
        Self {
            id: agent.id().as_str().to_string(),
            name: agent.name().to_string(),
            graph_id: agent.graph().id().as_str().to_string(),
            budget: AgentBudgetResponse::from(agent.budget()),
            termination_conditions: agent
                .termination_policy()
                .conditions()
                .iter()
                .map(ToString::to_string)
                .collect(),
        }
    }
}

impl From<&AgentBudget> for AgentBudgetResponse {
    fn from(budget: &AgentBudget) -> Self {
        Self {
            steps: budget.max_steps(),
            tokens: budget.max_tokens(),
            seconds: budget.max_seconds(),
            cost_micros: budget.max_cost_micros(),
        }
    }
}

impl AgentRunResponse {
    pub(crate) fn new(run: &AgentRunRecord) -> Result<Self, ApiError> {
        Ok(Self {
            id: run.id.as_str().to_string(),
            agent_id: run.agent_id.as_str().to_string(),
            status: run.status.to_string(),
            state_version: run.state_version,
            state: json_from_string(&run.state_json)?,
        })
    }
}

impl CreateGraphRequest {
    pub(crate) fn into_graph(self, id: String) -> Result<GraphDefinition, ApiError> {
        GraphDefinition::new(
            capsulet_core::GraphId::new(id).map_err(ApiError::validation)?,
            self.name,
            self.nodes
                .into_iter()
                .map(GraphNodeRequest::into_node)
                .collect::<Result<Vec<_>, _>>()?,
            self.hyperedges
                .into_iter()
                .map(GraphHyperedgeRequest::into_hyperedge)
                .collect::<Result<Vec<_>, _>>()?,
            self.transition_policy
                .map(GraphTransitionPolicyRequest::into_policy)
                .transpose()?
                .unwrap_or_else(GraphTransitionPolicy::static_acyclic),
        )
        .map_err(|error| ApiError::Validation(error.to_string()))
    }
}

impl GraphNodeRequest {
    fn into_node(self) -> Result<GraphNode, ApiError> {
        Ok(GraphNode::new(
            NodeId::new(self.id).map_err(ApiError::validation)?,
            self.name,
            parse_node_kind(&self.kind)?,
            self.ports
                .into_iter()
                .map(GraphPortRequest::into_port)
                .collect::<Result<Vec<_>, _>>()?,
        ))
    }
}

impl GraphPortRequest {
    fn into_port(self) -> Result<GraphPort, ApiError> {
        Ok(GraphPort::new(
            PortId::new(self.id).map_err(ApiError::validation)?,
            parse_port_direction(&self.direction)?,
            parse_port_value_type(&self.value_type)?,
        ))
    }
}

impl GraphHyperedgeRequest {
    fn into_hyperedge(self) -> Result<GraphHyperedge, ApiError> {
        Ok(GraphHyperedge::new(
            HyperedgeId::new(self.id).map_err(ApiError::validation)?,
            self.sources
                .into_iter()
                .map(HyperedgeEndpointRequest::into_endpoint)
                .collect::<Result<Vec<_>, _>>()?,
            self.targets
                .into_iter()
                .map(HyperedgeEndpointRequest::into_endpoint)
                .collect::<Result<Vec<_>, _>>()?,
        ))
    }
}

impl HyperedgeEndpointRequest {
    fn into_endpoint(self) -> Result<HyperedgeEndpoint, ApiError> {
        match self.kind.as_str() {
            "port" => Ok(HyperedgeEndpoint::port(
                NodeId::new(required_field(self.node_id, "endpoint node_id")?)
                    .map_err(ApiError::validation)?,
                PortId::new(required_field(self.port_id, "endpoint port_id")?)
                    .map_err(ApiError::validation)?,
            )),
            "state_field" => Ok(HyperedgeEndpoint::state_field(
                required_field(self.field, "endpoint field")?,
                parse_port_value_type(&required_field(self.value_type, "endpoint value_type")?)?,
            )),
            value => Err(ApiError::Validation(format!(
                "unknown hyperedge endpoint kind {value}"
            ))),
        }
    }
}

impl GraphTransitionPolicyRequest {
    fn into_policy(self) -> Result<GraphTransitionPolicy, ApiError> {
        let policy = match self.mode.as_str() {
            "static" => GraphTransitionPolicy::static_acyclic(),
            "planner" => GraphTransitionPolicy::planner(
                self.actions
                    .into_iter()
                    .map(NodeId::new)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(ApiError::validation)?,
            ),
            value => {
                return Err(ApiError::Validation(format!(
                    "unknown graph transition mode {value}"
                )));
            }
        };
        Ok(policy.with_cycles_allowed(self.cycles_allowed))
    }
}

impl AgentBudgetRequest {
    pub(crate) fn into_budget(self) -> Result<AgentBudget, ApiError> {
        AgentBudget::new(self.steps, self.tokens, self.seconds, self.cost_micros)
            .map_err(|error| ApiError::Validation(error.to_string()))
    }
}

pub(crate) fn termination_policy_from_conditions(
    conditions: Vec<String>,
) -> Result<AgentTerminationPolicy, ApiError> {
    if conditions.is_empty() {
        return Ok(AgentTerminationPolicy::default_rag());
    }
    Ok(AgentTerminationPolicy::new(
        conditions
            .into_iter()
            .map(|condition| parse_termination_condition(&condition))
            .collect::<Result<Vec<_>, _>>()?,
    ))
}

fn required_field(value: Option<String>, label: &str) -> Result<String, ApiError> {
    value.ok_or_else(|| ApiError::Validation(format!("{label} is required")))
}

fn parse_node_kind(value: &str) -> Result<NodeKind, ApiError> {
    match value {
        "planner" => Ok(NodeKind::Planner),
        "query_normalizer" => Ok(NodeKind::QueryNormalizer),
        "embedding" => Ok(NodeKind::Embedding),
        "retriever" => Ok(NodeKind::Retriever),
        "reranker" => Ok(NodeKind::Reranker),
        "prompt_builder" => Ok(NodeKind::PromptBuilder),
        "llm" => Ok(NodeKind::Llm),
        "validator" => Ok(NodeKind::Validator),
        "memory_read" => Ok(NodeKind::MemoryRead),
        "memory_write" => Ok(NodeKind::MemoryWrite),
        "return" => Ok(NodeKind::Return),
        "job" => Ok(NodeKind::Job),
        value => Err(ApiError::Validation(format!("unknown node kind {value}"))),
    }
}

fn parse_port_direction(value: &str) -> Result<PortDirection, ApiError> {
    match value {
        "input" => Ok(PortDirection::Input),
        "output" => Ok(PortDirection::Output),
        value => Err(ApiError::Validation(format!(
            "unknown port direction {value}"
        ))),
    }
}

fn parse_port_value_type(value: &str) -> Result<PortValueType, ApiError> {
    match value {
        "user_query" => Ok(PortValueType::UserQuery),
        "conversation_context" => Ok(PortValueType::ConversationContext),
        "normalized_query" => Ok(PortValueType::NormalizedQuery),
        "embedding_vector" => Ok(PortValueType::EmbeddingVector),
        "retrieved_documents" => Ok(PortValueType::RetrievedDocuments),
        "ranked_documents" => Ok(PortValueType::RankedDocuments),
        "prompt" => Ok(PortValueType::Prompt),
        "model_response" => Ok(PortValueType::ModelResponse),
        "validation_result" => Ok(PortValueType::ValidationResult),
        "final_answer" => Ok(PortValueType::FinalAnswer),
        "json" => Ok(PortValueType::Json),
        value => Err(ApiError::Validation(format!(
            "unknown port value type {value}"
        ))),
    }
}

fn parse_termination_condition(value: &str) -> Result<TerminationCondition, ApiError> {
    match value {
        "validator_pass" => Ok(TerminationCondition::ValidatorPass),
        "safety_failure" => Ok(TerminationCondition::SafetyFailure),
        "no_progress" => Ok(TerminationCondition::NoProgress),
        "human_escalation" => Ok(TerminationCondition::HumanEscalation),
        value => Err(ApiError::Validation(format!(
            "unknown termination condition {value}"
        ))),
    }
}

impl AutomationResponse {
    pub(crate) fn new(
        automation: &Automation,
        triggers: &[AutomationTrigger],
        condition_json: &str,
    ) -> Result<Self, ApiError> {
        Ok(Self {
            id: automation.id().as_str().to_string(),
            name: automation.name().to_string(),
            description: automation.description().to_string(),
            workflow_id: automation.workflow_id().as_str().to_string(),
            status: automation.status().to_string(),
            triggers: triggers.iter().map(TriggerResponse::from).collect(),
            condition: json_from_string(condition_json)?,
            job_input: json_from_string(automation.job_input_json()).unwrap_or_else(|_| json!({})),
        })
    }
}

impl From<&AutomationTrigger> for TriggerResponse {
    fn from(trigger: &AutomationTrigger) -> Self {
        Self {
            name: trigger.name().as_str().to_string(),
            kind: trigger.kind().to_string(),
            config: json_from_string(trigger.config_json()).unwrap_or_else(|_| json!({})),
            plugin_id: trigger.plugin_id().map(str::to_string),
            enabled: trigger.enabled(),
        }
    }
}

impl From<&CustomTriggerPlugin> for TriggerPluginResponse {
    fn from(plugin: &CustomTriggerPlugin) -> Self {
        Self {
            id: plugin.id().to_string(),
            name: plugin.name().to_string(),
            description: plugin.description().to_string(),
            runtime_image: plugin.runtime_image().to_string(),
            command: plugin.command().to_vec(),
            python_script: plugin_python_script(plugin),
            config_schema: json_from_string(plugin.config_schema_json())
                .unwrap_or_else(|_| json!({})),
        }
    }
}

fn plugin_python_script(plugin: &CustomTriggerPlugin) -> String {
    let command = plugin.command();
    if command.len() == 3 && command[0] == "python" && command[1] == "-c" {
        command[2].clone()
    } else {
        String::new()
    }
}

impl WorkflowRunResponse {
    pub(crate) fn new(run: &WorkflowRun, step_runs: &[WorkflowStepRun]) -> Self {
        Self {
            id: run.id().as_str().to_string(),
            workflow_id: run.workflow_id().as_str().to_string(),
            automation_id: run.automation_id().map(|id| id.as_str().to_string()),
            status: run.status().to_string(),
            current_step_position: run.current_step_position(),
            created_at: run.created_at().to_string(),
            step_runs: step_runs
                .iter()
                .map(WorkflowStepRunResponse::from)
                .collect(),
        }
    }
}

impl From<&WorkflowStepRun> for WorkflowStepRunResponse {
    fn from(step_run: &WorkflowStepRun) -> Self {
        Self {
            id: step_run.id().as_str().to_string(),
            workflow_step_id: step_run.workflow_step_id().as_str().to_string(),
            job_run_id: step_run
                .maybe_job_run_id()
                .map(|job_run_id| job_run_id.as_str().to_string()),
            position: step_run.position(),
            status: step_run.status().to_string(),
        }
    }
}

impl From<&JobRun> for JobRunResponse {
    fn from(run: &JobRun) -> Self {
        Self {
            id: run.id().as_str().to_string(),
            job_definition_id: run.job_definition_id().as_str().to_string(),
            status: status_label(run.status()).to_string(),
            execution_pool: run.execution_pool().as_str().to_string(),
            host_group: run.execution_pool().as_str().to_string(),
            attempt_count: run.attempt_count(),
            created_at: run.created_at().to_string(),
            input: json_from_string(run.input_json()).unwrap_or_else(|_| json!({})),
        }
    }
}

const fn status_label(status: JobRunStatus) -> &'static str {
    match status {
        JobRunStatus::Queued => "queued",
        JobRunStatus::Leased => "leased",
        JobRunStatus::Running => "running",
        JobRunStatus::Succeeded => "succeeded",
        JobRunStatus::Failed => "failed",
        JobRunStatus::Cancelled => "cancelled",
        JobRunStatus::TimedOut => "timed_out",
        JobRunStatus::RetryScheduled => "retry_scheduled",
    }
}
