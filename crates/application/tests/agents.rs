use std::sync::Mutex;

use async_trait::async_trait;
use capsulet_application::{
    AgentRepository, AgentRunRecord, AgentService, GraphRepository, GraphService,
    StartAgentRunCommand,
};
use capsulet_core::{
    AgentBudget, AgentDefinition, AgentId, AgentRunId, AgentRunStatus, AgentTerminationPolicy,
    GraphDefinition, GraphHyperedge, GraphId, GraphNode, GraphPort, GraphTransitionPolicy,
    HyperedgeEndpoint, HyperedgeId, NodeId, NodeKind, PortDirection, PortId, PortValueType,
};

fn graph_id(value: &str) -> GraphId {
    GraphId::new(value).expect("valid graph id")
}

fn node_id(value: &str) -> NodeId {
    NodeId::new(value).expect("valid node id")
}

fn port_id(value: &str) -> PortId {
    PortId::new(value).expect("valid port id")
}

fn edge_id(value: &str) -> HyperedgeId {
    HyperedgeId::new(value).expect("valid edge id")
}

fn sample_graph() -> GraphDefinition {
    GraphDefinition::new(
        graph_id("rag"),
        "RAG",
        vec![
            GraphNode::new(
                node_id("prompt"),
                "Prompt",
                NodeKind::PromptBuilder,
                vec![GraphPort::new(
                    port_id("prompt.out"),
                    PortDirection::Output,
                    PortValueType::Prompt,
                )],
            ),
            GraphNode::new(
                node_id("llm"),
                "LLM",
                NodeKind::Llm,
                vec![
                    GraphPort::new(
                        port_id("llm.prompt"),
                        PortDirection::Input,
                        PortValueType::Prompt,
                    ),
                    GraphPort::new(
                        port_id("llm.answer"),
                        PortDirection::Output,
                        PortValueType::FinalAnswer,
                    ),
                ],
            ),
        ],
        vec![GraphHyperedge::new(
            edge_id("prompt-llm"),
            vec![HyperedgeEndpoint::port(
                node_id("prompt"),
                port_id("prompt.out"),
            )],
            vec![HyperedgeEndpoint::port(
                node_id("llm"),
                port_id("llm.prompt"),
            )],
        )],
        GraphTransitionPolicy::planner(vec![node_id("prompt"), node_id("llm")])
            .with_cycles_allowed(true),
    )
    .expect("valid graph")
}

fn sample_agent() -> AgentDefinition {
    AgentDefinition::new(
        AgentId::new("support").expect("valid agent id"),
        "Support",
        sample_graph(),
        Some(AgentBudget::new(4, 4000, 30, 1000).expect("valid budget")),
        Some(AgentTerminationPolicy::default_rag()),
    )
    .expect("valid agent")
}

#[derive(Default)]
struct FakeGraphRepository {
    saved: Mutex<Vec<GraphDefinition>>,
}

#[async_trait]
impl GraphRepository for FakeGraphRepository {
    type Error = String;

    async fn save_graph(&self, graph: &GraphDefinition) -> Result<(), Self::Error> {
        self.saved.lock().expect("lock").push(graph.clone());
        Ok(())
    }
}

#[derive(Default)]
struct FakeAgentRepository {
    saved_agents: Mutex<Vec<AgentDefinition>>,
    saved_runs: Mutex<Vec<AgentRunRecord>>,
}

#[async_trait]
impl AgentRepository for FakeAgentRepository {
    type Error = String;

    async fn save_agent(&self, agent: &AgentDefinition) -> Result<(), Self::Error> {
        self.saved_agents.lock().expect("lock").push(agent.clone());
        Ok(())
    }

    async fn save_run(&self, run: &AgentRunRecord) -> Result<(), Self::Error> {
        self.saved_runs.lock().expect("lock").push(run.clone());
        Ok(())
    }
}

#[tokio::test]
async fn graph_service_persists_graph_definition() {
    let repository = FakeGraphRepository::default();
    let service = GraphService::new(&repository);
    let graph = sample_graph();

    service.create_graph(&graph).await.expect("created graph");

    assert_eq!(repository.saved.lock().expect("lock").len(), 1);
}

#[tokio::test]
async fn agent_service_persists_agent_definition() {
    let repository = FakeAgentRepository::default();
    let service = AgentService::new(&repository);
    let agent = sample_agent();

    service.create_agent(&agent).await.expect("created agent");

    assert_eq!(repository.saved_agents.lock().expect("lock").len(), 1);
}

#[tokio::test]
async fn agent_service_starts_queued_run_with_initial_state() {
    let repository = FakeAgentRepository::default();
    let service = AgentService::new(&repository);
    let command = StartAgentRunCommand {
        run_id: AgentRunId::new("run-1").expect("valid run id"),
        agent_id: AgentId::new("support").expect("valid agent id"),
        initial_state_json: r#"{"query":"How do I reset MFA?"}"#.to_string(),
    };

    service.start_run(command).await.expect("started run");

    let runs = repository.saved_runs.lock().expect("lock");
    assert_eq!(runs[0].status, AgentRunStatus::Queued);
}
