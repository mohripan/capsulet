//! Typed execution graph primitives.
//!
//! This module models the graph that an agent runtime executes: action nodes,
//! typed ports, hyperedges, and transition policy. It is intentionally not the
//! long-term memory graph. The memory graph will model claims, entities,
//! events, evidence, permissions, trust, and time; execution graphs are how
//! agents and ingestion pipelines act on that memory.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Display};

use thiserror::Error;

use super::{GraphId, HyperedgeId, NodeId, PortId, WorkflowDefinition, WorkflowStepId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NodeKind {
    Planner,
    QueryNormalizer,
    Embedding,
    Retriever,
    Reranker,
    PromptBuilder,
    Llm,
    Validator,
    MemoryRead,
    MemoryWrite,
    Return,
    Job,
}

impl Display for NodeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Planner => "planner",
            Self::QueryNormalizer => "query_normalizer",
            Self::Embedding => "embedding",
            Self::Retriever => "retriever",
            Self::Reranker => "reranker",
            Self::PromptBuilder => "prompt_builder",
            Self::Llm => "llm",
            Self::Validator => "validator",
            Self::MemoryRead => "memory_read",
            Self::MemoryWrite => "memory_write",
            Self::Return => "return",
            Self::Job => "job",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PortDirection {
    Input,
    Output,
}

impl Display for PortDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Input => "input",
            Self::Output => "output",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PortValueType {
    UserQuery,
    ConversationContext,
    NormalizedQuery,
    EmbeddingVector,
    RetrievedDocuments,
    RankedDocuments,
    Prompt,
    ModelResponse,
    ValidationResult,
    FinalAnswer,
    Json,
}

impl Display for PortValueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UserQuery => "user_query",
            Self::ConversationContext => "conversation_context",
            Self::NormalizedQuery => "normalized_query",
            Self::EmbeddingVector => "embedding_vector",
            Self::RetrievedDocuments => "retrieved_documents",
            Self::RankedDocuments => "ranked_documents",
            Self::Prompt => "prompt",
            Self::ModelResponse => "model_response",
            Self::ValidationResult => "validation_result",
            Self::FinalAnswer => "final_answer",
            Self::Json => "json",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphPort {
    id: PortId,
    direction: PortDirection,
    value_type: PortValueType,
}

impl GraphPort {
    #[must_use]
    pub const fn new(id: PortId, direction: PortDirection, value_type: PortValueType) -> Self {
        Self {
            id,
            direction,
            value_type,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &PortId {
        &self.id
    }

    #[must_use]
    pub const fn direction(&self) -> PortDirection {
        self.direction
    }

    #[must_use]
    pub const fn value_type(&self) -> PortValueType {
        self.value_type
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNode {
    id: NodeId,
    name: String,
    kind: NodeKind,
    ports: Vec<GraphPort>,
}

impl GraphNode {
    #[must_use]
    pub fn new(id: NodeId, name: impl Into<String>, kind: NodeKind, ports: Vec<GraphPort>) -> Self {
        Self {
            id,
            name: name.into(),
            kind,
            ports,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &NodeId {
        &self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn kind(&self) -> NodeKind {
        self.kind
    }

    #[must_use]
    pub fn ports(&self) -> &[GraphPort] {
        &self.ports
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum HyperedgeEndpoint {
    Port {
        node_id: NodeId,
        port_id: PortId,
    },
    StateField {
        field: String,
        value_type: PortValueType,
    },
}

impl HyperedgeEndpoint {
    #[must_use]
    pub const fn port(node_id: NodeId, port_id: PortId) -> Self {
        Self::Port { node_id, port_id }
    }

    #[must_use]
    pub fn state_field(field: impl Into<String>, value_type: PortValueType) -> Self {
        Self::StateField {
            field: field.into(),
            value_type,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphHyperedge {
    id: HyperedgeId,
    sources: Vec<HyperedgeEndpoint>,
    targets: Vec<HyperedgeEndpoint>,
}

impl GraphHyperedge {
    #[must_use]
    pub const fn new(
        id: HyperedgeId,
        sources: Vec<HyperedgeEndpoint>,
        targets: Vec<HyperedgeEndpoint>,
    ) -> Self {
        Self {
            id,
            sources,
            targets,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &HyperedgeId {
        &self.id
    }

    #[must_use]
    pub fn sources(&self) -> &[HyperedgeEndpoint] {
        &self.sources
    }

    #[must_use]
    pub fn targets(&self) -> &[HyperedgeEndpoint] {
        &self.targets
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphTransitionMode {
    Static,
    Planner { actions: Vec<NodeId> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphTransitionPolicy {
    mode: GraphTransitionMode,
    cycles_allowed: bool,
}

impl GraphTransitionPolicy {
    #[must_use]
    pub const fn static_acyclic() -> Self {
        Self {
            mode: GraphTransitionMode::Static,
            cycles_allowed: false,
        }
    }

    #[must_use]
    pub const fn planner(actions: Vec<NodeId>) -> Self {
        Self {
            mode: GraphTransitionMode::Planner { actions },
            cycles_allowed: false,
        }
    }

    #[must_use]
    pub const fn with_cycles_allowed(mut self, cycles_allowed: bool) -> Self {
        self.cycles_allowed = cycles_allowed;
        self
    }

    #[must_use]
    pub const fn cycles_allowed(&self) -> bool {
        self.cycles_allowed
    }

    #[must_use]
    pub const fn mode(&self) -> &GraphTransitionMode {
        &self.mode
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GraphError {
    #[error("duplicate graph node id {node_id}")]
    DuplicateNodeId { node_id: NodeId },
    #[error("duplicate graph port id {port_id} on node {node_id}")]
    DuplicatePortId { node_id: NodeId, port_id: PortId },
    #[error("duplicate graph hyperedge id {hyperedge_id}")]
    DuplicateHyperedgeId { hyperedge_id: HyperedgeId },
    #[error("hyperedge {hyperedge_id} must have at least one source")]
    EmptyHyperedgeSources { hyperedge_id: HyperedgeId },
    #[error("hyperedge {hyperedge_id} must have at least one target")]
    EmptyHyperedgeTargets { hyperedge_id: HyperedgeId },
    #[error("hyperedge endpoint references unknown node {node_id}")]
    UnknownNode { node_id: NodeId },
    #[error("hyperedge endpoint references unknown port {port_id} on node {node_id}")]
    UnknownPort { node_id: NodeId, port_id: PortId },
    #[error("port {port_id} on node {node_id} has invalid direction for this endpoint")]
    InvalidPortDirection { node_id: NodeId, port_id: PortId },
    #[error("target type {target_type:?} is not provided by any hyperedge source")]
    PortTypeMismatch {
        source_types: Vec<PortValueType>,
        target_type: PortValueType,
    },
    #[error("transition action references unknown node {node_id}")]
    UnknownActionNode { node_id: NodeId },
    #[error("graph contains a cycle but transition policy forbids cycles")]
    CycleForbidden,
    #[error("agent definition must include a budget policy")]
    MissingBudgetPolicy,
    #[error("agent definition must include a termination policy")]
    MissingTerminationPolicy,
    #[error("agent budget values must be greater than zero")]
    InvalidBudget,
    #[error("workflow definition could not be converted into a typed graph: {reason}")]
    InvalidWorkflowGraph { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphDefinition {
    id: GraphId,
    name: String,
    nodes: Vec<GraphNode>,
    hyperedges: Vec<GraphHyperedge>,
    transition_policy: GraphTransitionPolicy,
    static_order: Vec<NodeId>,
}

impl GraphDefinition {
    /// Builds and validates a typed directed hypergraph definition.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError`] when nodes, ports, hyperedges, endpoint types,
    /// or transition policy references are invalid.
    pub fn new(
        id: GraphId,
        name: impl Into<String>,
        nodes: Vec<GraphNode>,
        hyperedges: Vec<GraphHyperedge>,
        transition_policy: GraphTransitionPolicy,
    ) -> Result<Self, GraphError> {
        let index = GraphIndex::new(&nodes)?;
        let node_edges =
            Self::validate_hyperedges(&index, &hyperedges, transition_policy.cycles_allowed())?;
        Self::validate_transition_policy(&index, &transition_policy)?;
        let static_order = static_order(index.node_ids(), &node_edges);
        Ok(Self {
            id,
            name: name.into(),
            nodes,
            hyperedges,
            transition_policy,
            static_order,
        })
    }

    /// Compiles a legacy workflow definition into a constrained typed graph.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError`] when workflow identifiers cannot be represented
    /// as graph identifiers or when the compiled dependency graph is invalid.
    pub fn from_workflow(workflow: &WorkflowDefinition) -> Result<Self, GraphError> {
        let nodes = workflow
            .steps()
            .iter()
            .map(|step| -> Result<GraphNode, GraphError> {
                let node_id = NodeId::new(step.id().as_str())
                    .map_err(|reason| GraphError::InvalidWorkflowGraph { reason })?;
                let input_id = PortId::new(format!("{}.input", step.id()))
                    .map_err(|reason| GraphError::InvalidWorkflowGraph { reason })?;
                let result_id = PortId::new(format!("{}.result", step.id()))
                    .map_err(|reason| GraphError::InvalidWorkflowGraph { reason })?;
                Ok(GraphNode::new(
                    node_id,
                    step.name(),
                    NodeKind::Job,
                    vec![
                        GraphPort::new(input_id, PortDirection::Input, PortValueType::Json),
                        GraphPort::new(result_id, PortDirection::Output, PortValueType::Json),
                    ],
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let hyperedges = workflow
            .dependencies()
            .iter()
            .map(|dependency| {
                workflow_dependency_to_hyperedge(dependency.from_step_id(), dependency.to_step_id())
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|reason| GraphError::InvalidWorkflowGraph { reason })?;
        Self::new(
            GraphId::new(workflow.id().as_str())
                .map_err(|reason| GraphError::InvalidWorkflowGraph { reason })?,
            workflow.name(),
            nodes,
            hyperedges,
            GraphTransitionPolicy::static_acyclic(),
        )
    }

    fn validate_hyperedges(
        index: &GraphIndex,
        hyperedges: &[GraphHyperedge],
        cycles_allowed: bool,
    ) -> Result<BTreeMap<NodeId, Vec<NodeId>>, GraphError> {
        let mut ids = BTreeSet::new();
        let mut node_edges = BTreeMap::<NodeId, Vec<NodeId>>::new();
        for hyperedge in hyperedges {
            if !ids.insert(hyperedge.id().clone()) {
                return Err(GraphError::DuplicateHyperedgeId {
                    hyperedge_id: hyperedge.id().clone(),
                });
            }
            if hyperedge.sources().is_empty() {
                return Err(GraphError::EmptyHyperedgeSources {
                    hyperedge_id: hyperedge.id().clone(),
                });
            }
            if hyperedge.targets().is_empty() {
                return Err(GraphError::EmptyHyperedgeTargets {
                    hyperedge_id: hyperedge.id().clone(),
                });
            }
            let source_types = hyperedge
                .sources()
                .iter()
                .map(|endpoint| index.endpoint_type(endpoint, PortDirection::Output))
                .collect::<Result<Vec<_>, _>>()?;
            for target in hyperedge.targets() {
                let target_type = index.endpoint_type(target, PortDirection::Input)?;
                if !source_types.contains(&target_type) {
                    return Err(GraphError::PortTypeMismatch {
                        source_types,
                        target_type,
                    });
                }
            }
            for source in hyperedge.sources() {
                let Some(source_node) = source.node_id() else {
                    continue;
                };
                for target in hyperedge.targets() {
                    let Some(target_node) = target.node_id() else {
                        continue;
                    };
                    if source_node != target_node {
                        node_edges
                            .entry(source_node.clone())
                            .or_default()
                            .push(target_node.clone());
                    }
                }
            }
        }
        if !cycles_allowed && contains_cycle(index.node_ids(), &node_edges) {
            return Err(GraphError::CycleForbidden);
        }
        Ok(node_edges)
    }

    fn validate_transition_policy(
        index: &GraphIndex,
        policy: &GraphTransitionPolicy,
    ) -> Result<(), GraphError> {
        if let GraphTransitionMode::Planner { actions } = policy.mode() {
            for action in actions {
                if !index.nodes.contains_key(action) {
                    return Err(GraphError::UnknownActionNode {
                        node_id: action.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    #[must_use]
    pub const fn id(&self) -> &GraphId {
        &self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn nodes(&self) -> &[GraphNode] {
        &self.nodes
    }

    #[must_use]
    pub fn hyperedges(&self) -> &[GraphHyperedge] {
        &self.hyperedges
    }

    #[must_use]
    pub const fn transition_policy(&self) -> &GraphTransitionPolicy {
        &self.transition_policy
    }

    #[must_use]
    pub fn static_order(&self) -> &[NodeId] {
        &self.static_order
    }
}

struct GraphIndex {
    nodes: BTreeMap<NodeId, BTreeMap<PortId, GraphPort>>,
}

impl GraphIndex {
    fn new(nodes: &[GraphNode]) -> Result<Self, GraphError> {
        let mut indexed_nodes = BTreeMap::new();
        for node in nodes {
            let mut ports = BTreeMap::new();
            for port in node.ports() {
                if ports.insert(port.id().clone(), port.clone()).is_some() {
                    return Err(GraphError::DuplicatePortId {
                        node_id: node.id().clone(),
                        port_id: port.id().clone(),
                    });
                }
            }
            if indexed_nodes.insert(node.id().clone(), ports).is_some() {
                return Err(GraphError::DuplicateNodeId {
                    node_id: node.id().clone(),
                });
            }
        }
        Ok(Self {
            nodes: indexed_nodes,
        })
    }

    fn node_ids(&self) -> impl Iterator<Item = &NodeId> {
        self.nodes.keys()
    }

    fn endpoint_type(
        &self,
        endpoint: &HyperedgeEndpoint,
        expected_direction: PortDirection,
    ) -> Result<PortValueType, GraphError> {
        match endpoint {
            HyperedgeEndpoint::Port { node_id, port_id } => {
                let ports = self
                    .nodes
                    .get(node_id)
                    .ok_or_else(|| GraphError::UnknownNode {
                        node_id: node_id.clone(),
                    })?;
                let port = ports.get(port_id).ok_or_else(|| GraphError::UnknownPort {
                    node_id: node_id.clone(),
                    port_id: port_id.clone(),
                })?;
                if port.direction() != expected_direction {
                    return Err(GraphError::InvalidPortDirection {
                        node_id: node_id.clone(),
                        port_id: port_id.clone(),
                    });
                }
                Ok(port.value_type())
            }
            HyperedgeEndpoint::StateField { value_type, .. } => Ok(*value_type),
        }
    }
}

impl HyperedgeEndpoint {
    fn node_id(&self) -> Option<&NodeId> {
        match self {
            Self::Port { node_id, .. } => Some(node_id),
            Self::StateField { .. } => None,
        }
    }
}

fn contains_cycle<'a>(
    node_ids: impl Iterator<Item = &'a NodeId>,
    edges: &BTreeMap<NodeId, Vec<NodeId>>,
) -> bool {
    let node_ids = node_ids.collect::<Vec<_>>();
    static_order(node_ids.iter().copied(), edges).len() != node_ids.len()
}

fn static_order<'a>(
    node_ids: impl Iterator<Item = &'a NodeId>,
    edges: &BTreeMap<NodeId, Vec<NodeId>>,
) -> Vec<NodeId> {
    let mut incoming = node_ids
        .map(|id| (id.clone(), 0usize))
        .collect::<BTreeMap<_, _>>();
    for targets in edges.values() {
        for target in targets {
            if let Some(count) = incoming.get_mut(target) {
                *count += 1;
            }
        }
    }
    let mut ready = incoming
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(id.clone()))
        .collect::<BTreeSet<_>>();
    let mut order = Vec::with_capacity(incoming.len());
    while let Some(id) = ready.pop_first() {
        order.push(id.clone());
        if let Some(targets) = edges.get(&id) {
            for target in targets {
                let Some(count) = incoming.get_mut(target) else {
                    continue;
                };
                *count -= 1;
                if *count == 0 {
                    ready.insert(target.clone());
                }
            }
        }
    }
    order
}

fn workflow_dependency_to_hyperedge(
    from: &WorkflowStepId,
    to: &WorkflowStepId,
) -> Result<GraphHyperedge, String> {
    Ok(GraphHyperedge::new(
        HyperedgeId::new(format!("{from}-to-{to}"))?,
        vec![HyperedgeEndpoint::port(
            NodeId::new(from.as_str())?,
            PortId::new(format!("{from}.result"))?,
        )],
        vec![HyperedgeEndpoint::port(
            NodeId::new(to.as_str())?,
            PortId::new(format!("{to}.input"))?,
        )],
    ))
}
