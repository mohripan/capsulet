use std::sync::Mutex;

use async_trait::async_trait;
use capsulet_application::{
    AgentNodeExecution, AgentNodeExecutor, AgentNodeOutcome, AgentRunRecord, AgentRuntime,
    AgentRuntimeRepository, AgentStopReason, AgentTraceRecord,
};
use capsulet_core::{
    AgentBudget, AgentDefinition, AgentId, AgentRunId, AgentRunStatus, AgentTerminationPolicy,
    GraphDefinition, GraphHyperedge, GraphId, GraphNode, GraphPort, GraphTransitionPolicy,
    HyperedgeEndpoint, HyperedgeId, NodeId, NodeKind, PortDirection, PortId, PortValueType,
    TerminationCondition,
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

fn runtime_graph() -> GraphDefinition {
    GraphDefinition::new(
        graph_id("runtime_graph"),
        "Runtime Graph",
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
            edge_id("prompt_to_llm"),
            vec![HyperedgeEndpoint::port(
                node_id("prompt"),
                port_id("prompt.out"),
            )],
            vec![HyperedgeEndpoint::port(
                node_id("llm"),
                port_id("llm.prompt"),
            )],
        )],
        GraphTransitionPolicy::static_acyclic(),
    )
    .expect("valid graph")
}

fn agent_with_budget(max_steps: u32) -> AgentDefinition {
    AgentDefinition::new(
        AgentId::new("agent").expect("valid agent id"),
        "Agent",
        runtime_graph(),
        Some(AgentBudget::new(max_steps, 10_000, 30, 1_000_000).expect("valid budget")),
        Some(AgentTerminationPolicy::new(vec![
            TerminationCondition::ValidatorPass,
        ])),
    )
    .expect("valid agent")
}

fn queued_run() -> AgentRunRecord {
    AgentRunRecord {
        id: AgentRunId::new("agent_run").expect("valid run id"),
        agent_id: AgentId::new("agent").expect("valid agent id"),
        status: AgentRunStatus::Queued,
        state_version: 0,
        state_json: r#"{"query":"hello"}"#.to_string(),
    }
}

#[derive(Default)]
struct FakeRuntimeRepository {
    runs: Mutex<Vec<AgentRunRecord>>,
    traces: Mutex<Vec<AgentTraceRecord>>,
}

#[async_trait]
impl AgentRuntimeRepository for FakeRuntimeRepository {
    type Error = String;

    async fn save_agent_run(&self, run: &AgentRunRecord) -> Result<(), Self::Error> {
        self.runs.lock().expect("runs lock").push(run.clone());
        Ok(())
    }

    async fn append_trace_event(&self, event: &AgentTraceRecord) -> Result<(), Self::Error> {
        self.traces.lock().expect("traces lock").push(event.clone());
        Ok(())
    }
}

struct RecordingExecutor;

#[async_trait]
impl AgentNodeExecutor for RecordingExecutor {
    type Error = String;

    async fn execute(
        &self,
        execution: AgentNodeExecution,
    ) -> Result<AgentNodeOutcome, Self::Error> {
        Ok(AgentNodeOutcome::continue_with_state(
            format!(
                r#"{{"last_node":"{}","version":{}}}"#,
                execution.node_id.as_str(),
                execution.state_version + 1
            ),
            100,
            10,
        ))
    }
}

struct PassingValidatorExecutor;

#[async_trait]
impl AgentNodeExecutor for PassingValidatorExecutor {
    type Error = String;

    async fn execute(
        &self,
        execution: AgentNodeExecution,
    ) -> Result<AgentNodeOutcome, Self::Error> {
        if execution.node_id.as_str() == "llm" {
            Ok(AgentNodeOutcome::stop_with_state(
                r#"{"answer":"done"}"#,
                AgentStopReason::ValidatorPass,
                100,
                10,
            ))
        } else {
            Ok(AgentNodeOutcome::continue_with_state(
                r#"{"prompt":"built"}"#,
                100,
                10,
            ))
        }
    }
}

struct FailingExecutor;

#[async_trait]
impl AgentNodeExecutor for FailingExecutor {
    type Error = String;

    async fn execute(
        &self,
        _execution: AgentNodeExecution,
    ) -> Result<AgentNodeOutcome, Self::Error> {
        Err("model provider unavailable".to_string())
    }
}

#[tokio::test]
async fn runtime_executes_static_graph_and_persists_state_versions() {
    let repository = FakeRuntimeRepository::default();
    let runtime = AgentRuntime::new(&repository, &RecordingExecutor);

    let result = runtime
        .execute_run(&agent_with_budget(4), queued_run())
        .await
        .expect("runtime executes");

    assert_eq!(result.status, AgentRunStatus::Succeeded);
    assert_eq!(result.state_version, 2);
}

#[tokio::test]
async fn runtime_appends_trace_events_for_node_execution() {
    let repository = FakeRuntimeRepository::default();
    let runtime = AgentRuntime::new(&repository, &RecordingExecutor);

    runtime
        .execute_run(&agent_with_budget(4), queued_run())
        .await
        .expect("runtime executes");

    let traces = repository.traces.lock().expect("traces lock");
    assert_eq!(traces[0].event_type, "node_started");
    assert_eq!(traces[1].event_type, "node_completed");
}

#[tokio::test]
async fn runtime_stops_when_step_budget_is_exhausted() {
    let repository = FakeRuntimeRepository::default();
    let runtime = AgentRuntime::new(&repository, &RecordingExecutor);

    let result = runtime
        .execute_run(&agent_with_budget(1), queued_run())
        .await
        .expect("runtime executes");

    assert_eq!(result.status, AgentRunStatus::Stopped);
}

#[tokio::test]
async fn runtime_succeeds_when_executor_returns_validator_pass() {
    let repository = FakeRuntimeRepository::default();
    let runtime = AgentRuntime::new(&repository, &PassingValidatorExecutor);

    let result = runtime
        .execute_run(&agent_with_budget(4), queued_run())
        .await
        .expect("runtime executes");

    assert_eq!(result.state_json, r#"{"answer":"done"}"#);
}

#[tokio::test]
async fn runtime_marks_run_failed_when_node_executor_fails() {
    let repository = FakeRuntimeRepository::default();
    let runtime = AgentRuntime::new(&repository, &FailingExecutor);

    let result = runtime
        .execute_run(&agent_with_budget(4), queued_run())
        .await
        .expect("runtime returns failed run");

    assert_eq!(result.status, AgentRunStatus::Failed);
}
