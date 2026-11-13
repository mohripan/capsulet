//! Typed graphs: their nodes, ports, hyperedges and transition policies.

use std::collections::BTreeMap;

use capsulet_core::{
    GraphDefinition, GraphHyperedge, GraphNode, GraphPort, GraphTransitionMode,
    GraphTransitionPolicy, HyperedgeEndpoint, HyperedgeId, NodeId, NodeKind, PortDirection, PortId,
    PortValueType,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::ApiError;

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateGraphRequest {
    pub id: Option<String>,
    pub name: String,
    pub nodes: Vec<GraphNodeRequest>,
    pub hyperedges: Vec<GraphHyperedgeRequest>,
    pub transition_policy: Option<GraphTransitionPolicyRequest>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GraphNodeRequest {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub ports: Vec<GraphPortRequest>,
    /// What this node does: a prompt template, a model, a validator's rule, the
    /// job it runs. Strings only, because it becomes part of a definition digest.
    #[serde(default)]
    pub config: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GraphPortRequest {
    pub id: String,
    pub direction: String,
    pub value_type: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GraphHyperedgeRequest {
    pub id: String,
    pub sources: Vec<HyperedgeEndpointRequest>,
    pub targets: Vec<HyperedgeEndpointRequest>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct HyperedgeEndpointRequest {
    pub kind: String,
    pub node_id: Option<String>,
    pub port_id: Option<String>,
    pub field: Option<String>,
    pub value_type: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GraphTransitionPolicyRequest {
    pub mode: String,
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(default)]
    pub cycles_allowed: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ListGraphsResponse {
    pub(crate) graphs: Vec<GraphResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct GraphResponse {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) nodes: Vec<GraphNodeResponse>,
    pub(crate) hyperedges: Vec<GraphHyperedgeResponse>,
    pub(crate) transition_policy: GraphTransitionPolicyResponse,
    pub(crate) static_order: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct GraphNodeResponse {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) ports: Vec<GraphPortResponse>,
    pub(crate) config: BTreeMap<String, String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct GraphPortResponse {
    pub(crate) id: String,
    pub(crate) direction: String,
    pub(crate) value_type: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct GraphHyperedgeResponse {
    pub(crate) id: String,
    pub(crate) sources: Vec<HyperedgeEndpointResponse>,
    pub(crate) targets: Vec<HyperedgeEndpointResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct HyperedgeEndpointResponse {
    pub(crate) kind: String,
    pub(crate) node_id: Option<String>,
    pub(crate) port_id: Option<String>,
    pub(crate) field: Option<String>,
    pub(crate) value_type: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct GraphTransitionPolicyResponse {
    pub(crate) mode: String,
    pub(crate) actions: Vec<String>,
    pub(crate) cycles_allowed: bool,
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
            config: node.config().clone(),
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
        )
        .with_config(self.config))
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

pub(super) fn required_field(value: Option<String>, label: &str) -> Result<String, ApiError> {
    value.ok_or_else(|| ApiError::Validation(format!("{label} is required")))
}

pub(super) fn parse_node_kind(value: &str) -> Result<NodeKind, ApiError> {
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

pub(super) fn parse_port_direction(value: &str) -> Result<PortDirection, ApiError> {
    match value {
        "input" => Ok(PortDirection::Input),
        "output" => Ok(PortDirection::Output),
        value => Err(ApiError::Validation(format!(
            "unknown port direction {value}"
        ))),
    }
}

pub(super) fn parse_port_value_type(value: &str) -> Result<PortValueType, ApiError> {
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
