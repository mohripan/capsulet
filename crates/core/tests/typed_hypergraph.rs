use capsulet_core::{
    AgentBudget, AgentDefinition, AgentId, AgentTerminationPolicy, GraphDefinition, GraphError,
    GraphHyperedge, GraphId, GraphNode, GraphPort, GraphTransitionPolicy, HyperedgeEndpoint,
    HyperedgeId, NodeId, NodeKind, PortDirection, PortId, PortValueType,
};

fn id(value: &str) -> GraphId {
    GraphId::new(value).expect("valid graph id")
}

fn node_id(value: &str) -> NodeId {
    NodeId::new(value).expect("valid node id")
}

fn port_id(value: &str) -> PortId {
    PortId::new(value).expect("valid port id")
}

fn edge_id(value: &str) -> HyperedgeId {
    HyperedgeId::new(value).expect("valid hyperedge id")
}

fn input(id: &str, value_type: PortValueType) -> GraphPort {
    GraphPort::new(port_id(id), PortDirection::Input, value_type)
}

fn output(id: &str, value_type: PortValueType) -> GraphPort {
    GraphPort::new(port_id(id), PortDirection::Output, value_type)
}

fn rag_graph() -> GraphDefinition {
    GraphDefinition::new(
        id("support-rag"),
        "Support RAG",
        vec![
            GraphNode::new(
                node_id("query"),
                "Query",
                NodeKind::QueryNormalizer,
                vec![
                    input("query.in", PortValueType::UserQuery),
                    output("query.out", PortValueType::NormalizedQuery),
                ],
            ),
            GraphNode::new(
                node_id("retrieve"),
                "Retrieve",
                NodeKind::Retriever,
                vec![
                    input("retrieve.query", PortValueType::NormalizedQuery),
                    input("retrieve.context", PortValueType::ConversationContext),
                    output("retrieve.docs", PortValueType::RetrievedDocuments),
                ],
            ),
            GraphNode::new(
                node_id("prompt"),
                "Prompt",
                NodeKind::PromptBuilder,
                vec![
                    input("prompt.query", PortValueType::NormalizedQuery),
                    input("prompt.docs", PortValueType::RetrievedDocuments),
                    output("prompt.out", PortValueType::Prompt),
                ],
            ),
        ],
        vec![
            GraphHyperedge::new(
                edge_id("query-to-retrieve"),
                vec![HyperedgeEndpoint::port(
                    node_id("query"),
                    port_id("query.out"),
                )],
                vec![HyperedgeEndpoint::port(
                    node_id("retrieve"),
                    port_id("retrieve.query"),
                )],
            ),
            GraphHyperedge::new(
                edge_id("query-and-docs-to-prompt"),
                vec![
                    HyperedgeEndpoint::port(node_id("query"), port_id("query.out")),
                    HyperedgeEndpoint::port(node_id("retrieve"), port_id("retrieve.docs")),
                ],
                vec![
                    HyperedgeEndpoint::port(node_id("prompt"), port_id("prompt.query")),
                    HyperedgeEndpoint::port(node_id("prompt"), port_id("prompt.docs")),
                ],
            ),
        ],
        GraphTransitionPolicy::planner(vec![
            node_id("query"),
            node_id("retrieve"),
            node_id("prompt"),
        ])
        .with_cycles_allowed(true),
    )
    .expect("valid graph")
}

#[test]
fn graph_definition_accepts_multi_source_and_multi_target_hyperedges() {
    let graph = rag_graph();

    assert_eq!(graph.hyperedges().len(), 2);
}

#[test]
fn graph_definition_rejects_type_mismatch_between_ports() {
    let result = GraphDefinition::new(
        id("bad"),
        "Bad",
        vec![
            GraphNode::new(
                node_id("a"),
                "A",
                NodeKind::Embedding,
                vec![output("a.out", PortValueType::EmbeddingVector)],
            ),
            GraphNode::new(
                node_id("b"),
                "B",
                NodeKind::PromptBuilder,
                vec![input("b.in", PortValueType::Prompt)],
            ),
        ],
        vec![GraphHyperedge::new(
            edge_id("bad-edge"),
            vec![HyperedgeEndpoint::port(node_id("a"), port_id("a.out"))],
            vec![HyperedgeEndpoint::port(node_id("b"), port_id("b.in"))],
        )],
        GraphTransitionPolicy::static_acyclic(),
    );

    assert!(matches!(result, Err(GraphError::PortTypeMismatch { .. })));
}

#[test]
fn graph_definition_rejects_cycle_when_transition_policy_forbids_cycles() {
    let result = GraphDefinition::new(
        id("cycle"),
        "Cycle",
        vec![
            GraphNode::new(
                node_id("a"),
                "A",
                NodeKind::MemoryRead,
                vec![
                    input("a.in", PortValueType::ConversationContext),
                    output("a.out", PortValueType::ConversationContext),
                ],
            ),
            GraphNode::new(
                node_id("b"),
                "B",
                NodeKind::MemoryWrite,
                vec![
                    input("b.in", PortValueType::ConversationContext),
                    output("b.out", PortValueType::ConversationContext),
                ],
            ),
        ],
        vec![
            GraphHyperedge::new(
                edge_id("a-b"),
                vec![HyperedgeEndpoint::port(node_id("a"), port_id("a.out"))],
                vec![HyperedgeEndpoint::port(node_id("b"), port_id("b.in"))],
            ),
            GraphHyperedge::new(
                edge_id("b-a"),
                vec![HyperedgeEndpoint::port(node_id("b"), port_id("b.out"))],
                vec![HyperedgeEndpoint::port(node_id("a"), port_id("a.in"))],
            ),
        ],
        GraphTransitionPolicy::static_acyclic(),
    );

    assert!(matches!(result, Err(GraphError::CycleForbidden)));
}

#[test]
fn graph_definition_returns_deterministic_static_order() {
    let graph = GraphDefinition::new(
        id("static"),
        "Static",
        vec![
            GraphNode::new(
                node_id("return"),
                "Return",
                NodeKind::Return,
                vec![input("return.in", PortValueType::FinalAnswer)],
            ),
            GraphNode::new(
                node_id("llm"),
                "LLM",
                NodeKind::Llm,
                vec![
                    input("llm.prompt", PortValueType::Prompt),
                    output("llm.answer", PortValueType::FinalAnswer),
                ],
            ),
            GraphNode::new(
                node_id("prompt"),
                "Prompt",
                NodeKind::PromptBuilder,
                vec![output("prompt.out", PortValueType::Prompt)],
            ),
        ],
        vec![
            GraphHyperedge::new(
                edge_id("prompt-llm"),
                vec![HyperedgeEndpoint::port(
                    node_id("prompt"),
                    port_id("prompt.out"),
                )],
                vec![HyperedgeEndpoint::port(
                    node_id("llm"),
                    port_id("llm.prompt"),
                )],
            ),
            GraphHyperedge::new(
                edge_id("llm-return"),
                vec![HyperedgeEndpoint::port(
                    node_id("llm"),
                    port_id("llm.answer"),
                )],
                vec![HyperedgeEndpoint::port(
                    node_id("return"),
                    port_id("return.in"),
                )],
            ),
        ],
        GraphTransitionPolicy::static_acyclic(),
    )
    .expect("valid static graph");

    let ordered = graph
        .static_order()
        .iter()
        .map(NodeId::as_str)
        .collect::<Vec<_>>();

    assert_eq!(ordered, ["prompt", "llm", "return"]);
}

#[test]
fn agent_definition_requires_budget_and_termination_policy() {
    let result = AgentDefinition::new(
        AgentId::new("support-agent").expect("valid agent id"),
        "Support Agent",
        rag_graph(),
        None,
        Some(AgentTerminationPolicy::default_rag()),
    );

    assert!(matches!(result, Err(GraphError::MissingBudgetPolicy)));
}

#[test]
fn agent_definition_accepts_bounded_rag_agent() {
    let agent = AgentDefinition::new(
        AgentId::new("support-agent").expect("valid agent id"),
        "Support Agent",
        rag_graph(),
        Some(AgentBudget::new(12, 12_000, 90, 2500).expect("valid budget")),
        Some(AgentTerminationPolicy::default_rag()),
    )
    .expect("valid agent");

    assert_eq!(agent.name(), "Support Agent");
}
