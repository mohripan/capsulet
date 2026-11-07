//! What the adapters carry, what they lose, and what they refuse.
//!
//! The differential tests matter more than the happy path: a translation is
//! only useful if it accepts what today's models accept and refuses what they
//! refuse. A translator that quietly admits something the source rejects is
//! worse than no translator, because it launders the rejection.

use capsulet_core::{
    AgentBudget, AgentDefinition, AgentId, AgentTerminationPolicy, ExecutionPoolName,
    GraphDefinition, GraphHyperedge, GraphId, GraphNode, GraphPort, GraphTransitionPolicy,
    HyperedgeEndpoint, HyperedgeId, JobDefinitionId, NodeId, NodeKind as GraphNodeKind,
    PortDirection, PortId, PortValueType, TerminationCondition, WorkflowDefinition,
    WorkflowDependencyPolicy, WorkflowGraph, WorkflowId, WorkflowStatus, WorkflowStep,
    WorkflowStepDependency, WorkflowStepId,
};
use capsulet_ir::admission::AdmissionCode;
use capsulet_ir::definition::AssuranceMode;
use capsulet_ir::effect::Idempotency;
use capsulet_ir::node::NodeKind;
use capsulet_ir::region::RegionKind;
use capsulet_ir::{admit, digest_of};
use capsulet_ir_adapters::{CoverageLevel, coverage, from_agent, from_graph, from_workflow};
use std::collections::BTreeMap;

fn workflow_id() -> WorkflowId {
    WorkflowId::new("wf_release").expect("the fixture id is valid")
}

fn step(name: &str, position: i32, timeout: Option<u64>) -> WorkflowStep {
    WorkflowStep::new(
        WorkflowStepId::new(name).expect("the fixture id is valid"),
        workflow_id(),
        position,
        name,
        JobDefinitionId::new("job_build").expect("the fixture id is valid"),
        ExecutionPoolName::new("default").expect("the fixture pool is valid"),
    )
    .with_timeout_seconds(timeout)
}

fn workflow() -> WorkflowDefinition {
    WorkflowDefinition::new(
        workflow_id(),
        "Release",
        "Build, test, and publish",
        WorkflowStatus::Enabled,
        vec![
            step("build", 0, Some(600)),
            step("test", 1, Some(1_800)),
            step("publish", 2, None),
        ],
    )
}

fn agent_graph() -> GraphDefinition {
    GraphDefinition::from_workflow(&workflow()).expect("the fixture graph builds")
}

#[test]
fn a_workflow_translates_and_is_admitted() {
    let adapted = from_workflow(&workflow()).expect("the workflow translates");

    assert_eq!(adapted.definition.assurance, AssuranceMode::Observe);
    assert_eq!(adapted.definition.graph.nodes().count(), 3);
    assert!(
        admit(&adapted.definition).is_ok(),
        "a translated workflow must pass structural admission"
    );
}

#[test]
fn a_workflow_step_is_an_effect_with_an_opaque_result() {
    let adapted = from_workflow(&workflow()).expect("the workflow translates");
    let build = adapted
        .definition
        .graph
        .nodes()
        .find(|node| node.id.as_str() == "build")
        .expect("the build step is translated");

    assert_eq!(build.kind, NodeKind::Effect);
    assert_eq!(build.effects.len(), 1);
    assert_eq!(build.budget.wall_ms, 600_000);
    assert!(
        build.outputs[0].schema.carries_opacity(),
        "a job's output is not modelled, and the translation says so"
    );
    assert!(
        adapted
            .notes
            .iter()
            .any(|note| note.construct == "workflow step"),
        "the loss must be recorded: {:?}",
        adapted.notes
    );
}

#[test]
fn a_soft_dependency_is_carried_as_ordering_with_the_difference_recorded() {
    let workflow = WorkflowDefinition::with_dependencies(
        workflow_id(),
        "Release",
        "Build, test, and publish",
        WorkflowStatus::Enabled,
        vec![step("build", 0, None), step("publish", 1, None)],
        vec![WorkflowStepDependency::with_policy(
            WorkflowStepId::new("build").expect("valid"),
            WorkflowStepId::new("publish").expect("valid"),
            WorkflowDependencyPolicy::Soft,
        )],
    );

    let adapted = from_workflow(&workflow).expect("the workflow translates");
    assert!(
        adapted.notes.iter().any(|note| {
            note.construct == "workflow step dependency" && note.detail.contains("soft")
        }),
        "the soft dependency must be recorded as loss: {:?}",
        adapted.notes
    );
}

#[test]
fn translation_is_deterministic() {
    let first = from_workflow(&workflow()).expect("the workflow translates");
    let second = from_workflow(&workflow()).expect("the workflow translates");

    assert_eq!(
        digest_of(&first.definition).expect("it digests"),
        digest_of(&second.definition).expect("it digests")
    );
    assert_eq!(first.notes, second.notes);
}

#[test]
fn a_dependency_cycle_the_current_model_rejects_is_also_refused_by_admission() {
    let cyclic = WorkflowDefinition::with_dependencies(
        workflow_id(),
        "Release",
        "A workflow that waits for itself",
        WorkflowStatus::Enabled,
        vec![step("build", 0, None), step("publish", 1, None)],
        vec![
            WorkflowStepDependency::new(
                WorkflowStepId::new("build").expect("valid"),
                WorkflowStepId::new("publish").expect("valid"),
            ),
            WorkflowStepDependency::new(
                WorkflowStepId::new("publish").expect("valid"),
                WorkflowStepId::new("build").expect("valid"),
            ),
        ],
    );

    // Today's model refuses it.
    assert!(
        WorkflowGraph::new(cyclic.id(), cyclic.steps(), cyclic.dependencies()).is_err(),
        "the fixture must be one the current model rejects"
    );

    // And so does admission, for a reason the author can act on.
    let adapted = from_workflow(&cyclic).expect("the translation itself succeeds");
    let refusal = admit(&adapted.definition).expect_err("a cycle outside a loop is refused");
    assert_eq!(refusal.code, AdmissionCode::RepetitionUnbounded);
}

#[test]
fn a_graph_translates_with_its_node_kinds_becoming_rules() {
    let adapted = from_graph(&agent_graph()).expect("the graph translates");

    if let Err(refusal) = admit(&adapted.definition) {
        panic!("a translated graph must pass structural admission: {refusal}");
    }
    for node in adapted.definition.graph.nodes() {
        // Job nodes carry no declared effect after translation, so they must
        // not claim a kind that requires one.
        assert_ne!(
            (node.kind, node.effects.is_empty()),
            (NodeKind::Effect, true),
            "an effect node with no effect would not be admissible"
        );
    }
}

fn port(id: &str, direction: PortDirection, value_type: PortValueType) -> GraphPort {
    GraphPort::new(PortId::new(id).expect("valid"), direction, value_type)
}

fn config(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect()
}

fn state(field: &str, value_type: PortValueType) -> HyperedgeEndpoint {
    HyperedgeEndpoint::state_field(field, value_type)
}

fn at(node: &str, port_id: &str) -> HyperedgeEndpoint {
    HyperedgeEndpoint::port(
        NodeId::new(node).expect("valid"),
        PortId::new(port_id).expect("valid"),
    )
}

fn edge(id: &str, source: HyperedgeEndpoint, target: HyperedgeEndpoint) -> GraphHyperedge {
    GraphHyperedge::new(
        HyperedgeId::new(id).expect("valid"),
        vec![source],
        vec![target],
    )
}

/// `query -> prompt -> model -> check`, with the answer written back to state.
fn rag_nodes() -> Vec<GraphNode> {
    vec![
        GraphNode::new(
            NodeId::new("prompt").expect("valid"),
            "Prompt",
            GraphNodeKind::PromptBuilder,
            vec![
                port("query", PortDirection::Input, PortValueType::UserQuery),
                port("text", PortDirection::Output, PortValueType::Prompt),
            ],
        )
        .with_config(config(&[("template", "Answer this: {{query}}")])),
        GraphNode::new(
            NodeId::new("model").expect("valid"),
            "Model",
            GraphNodeKind::Llm,
            vec![
                port("prompt", PortDirection::Input, PortValueType::Prompt),
                port(
                    "answer",
                    PortDirection::Output,
                    PortValueType::ModelResponse,
                ),
            ],
        )
        .with_config(config(&[("model", "small-model")])),
        GraphNode::new(
            NodeId::new("check").expect("valid"),
            "Check",
            GraphNodeKind::Validator,
            vec![
                port("answer", PortDirection::Input, PortValueType::ModelResponse),
                port(
                    "result",
                    PortDirection::Output,
                    PortValueType::ValidationResult,
                ),
            ],
        )
        .with_config(config(&[("rule.non_empty", "true")])),
    ]
}

fn rag_edges() -> Vec<GraphHyperedge> {
    vec![
        edge(
            "query-in",
            state("query", PortValueType::UserQuery),
            at("prompt", "query"),
        ),
        edge("prompt-model", at("prompt", "text"), at("model", "prompt")),
        edge("model-check", at("model", "answer"), at("check", "answer")),
        edge(
            "answer-out",
            at("model", "answer"),
            state("answer", PortValueType::ModelResponse),
        ),
    ]
}

fn rag_graph() -> GraphDefinition {
    GraphDefinition::new(
        GraphId::new("graph_rag").expect("valid"),
        "RAG",
        rag_nodes(),
        rag_edges(),
        GraphTransitionPolicy::static_acyclic(),
    )
    .expect("the fixture graph builds")
}

fn agent_of(graph: GraphDefinition, conditions: Vec<TerminationCondition>) -> AgentDefinition {
    AgentDefinition::new(
        AgentId::new("agent_rag").expect("valid"),
        "RAG agent",
        graph,
        Some(AgentBudget::new(8, 40_000, 300, 250_000).expect("the budget is valid")),
        Some(AgentTerminationPolicy::new(conditions)),
    )
    .expect("the agent is valid")
}

fn rag_agent() -> AgentDefinition {
    agent_of(
        rag_graph(),
        vec![
            TerminationCondition::ValidatorPass,
            TerminationCondition::NoProgress,
            TerminationCondition::HumanEscalation,
        ],
    )
}

fn refusal_of(graph: GraphDefinition) -> String {
    match from_agent(&agent_of(graph, vec![TerminationCondition::ValidatorPass])) {
        Err(capsulet_ir_adapters::AdapterError::Unsupported { construct }) => construct,
        Err(other) => panic!("expected a refusal naming the construct, got {other}"),
        Ok(_) => panic!("this graph must be refused"),
    }
}

#[test]
fn an_agent_becomes_a_bounded_loop_that_admits_and_records_its_budget() {
    let adapted = from_agent(&rag_agent()).expect("the agent translates");
    if let Err(refusal) = admit(&adapted.definition) {
        panic!("a translated agent must pass structural admission: {refusal}");
    }

    let region = adapted
        .definition
        .graph
        .regions()
        .next()
        .expect("the agent becomes a loop region");
    let RegionKind::Loop { spec } = &region.kind else {
        panic!("an agent with a budget is a loop");
    };
    assert_eq!(spec.budget.max_iterations, 8);
    assert_eq!(spec.budget.tokens, 40_000);
    assert_eq!(spec.budget.wall_ms, 300_000);
    assert_eq!(spec.budget.cost_micro_units, 250_000);
    assert_eq!(adapted.definition.assurance, AssuranceMode::Observe);
    assert!(
        spec.progress.is_some(),
        "stopping on no progress needs a measure to read"
    );
    assert_eq!(
        spec.continuation.evaluated_by.as_str(),
        capsulet_ir::agent::AGENT_CONTINUE
    );
}

#[test]
fn a_loops_exit_cannot_run_before_its_body() {
    let adapted = from_agent(&rag_agent()).expect("the agent translates");
    let graph = &adapted.definition.graph;
    let leave = capsulet_ir::Identifier::parse(capsulet_ir::agent::AGENT_LEAVE).expect("valid");
    let decide = capsulet_ir::Identifier::parse(capsulet_ir::agent::AGENT_CONTINUE).expect("valid");

    assert!(graph.predecessors_of(&leave).contains(&decide));
    for body in ["prompt", "model", "check"] {
        let body = capsulet_ir::Identifier::parse(body).expect("valid");
        assert!(
            graph.predecessors_of(&decide).contains(&body),
            "`{body}` must finish before the loop decides whether to go round again"
        );
    }
}

#[test]
fn a_state_field_crosses_the_loop_through_its_boundary() {
    let adapted = from_agent(&rag_agent()).expect("the agent translates");
    let enter = adapted
        .definition
        .graph
        .nodes()
        .find(|node| node.id.as_str() == capsulet_ir::agent::AGENT_ENTER)
        .expect("the entry node");
    let leave = adapted
        .definition
        .graph
        .nodes()
        .find(|node| node.id.as_str() == capsulet_ir::agent::AGENT_LEAVE)
        .expect("the exit node");

    assert!(enter.outputs.iter().any(|port| port.id.as_str() == "query"));
    assert!(
        leave
            .outputs
            .iter()
            .any(|port| port.id.as_str() == "answer")
    );
}

#[test]
fn a_field_both_read_and_written_is_carried_to_the_next_iteration() {
    let mut edges = rag_edges();
    edges.pop();
    edges.push(edge(
        "query-out",
        at("model", "answer"),
        state("query", PortValueType::ModelResponse),
    ));
    let graph = GraphDefinition::new(
        GraphId::new("graph_carry").expect("valid"),
        "Carry",
        rag_nodes(),
        edges,
        GraphTransitionPolicy::static_acyclic(),
    )
    .expect("the graph builds");
    let adapted = from_agent(&agent_of(graph, vec![TerminationCondition::ValidatorPass]))
        .expect("the agent translates");

    let region = adapted.definition.graph.regions().next().expect("a region");
    let RegionKind::Loop { spec } = &region.kind else {
        panic!("a loop");
    };
    assert!(spec.state.contains_key("query"));
}

#[test]
fn what_a_node_does_is_part_of_the_digest() {
    let digest_of_template = |template: &str| {
        let mut nodes = rag_nodes();
        nodes[0] = nodes[0]
            .clone()
            .with_config(config(&[("template", template)]));
        let graph = GraphDefinition::new(
            GraphId::new("graph_rag").expect("valid"),
            "RAG",
            nodes,
            rag_edges(),
            GraphTransitionPolicy::static_acyclic(),
        )
        .expect("the graph builds");
        let adapted = from_agent(&agent_of(graph, vec![TerminationCondition::ValidatorPass]))
            .expect("the agent translates");
        capsulet_ir::digest_of(&adapted.definition).expect("a digest")
    };
    assert_ne!(
        digest_of_template("Answer: {{query}}"),
        digest_of_template("Summarise: {{query}}"),
        "changing a prompt must make a new version rather than change what one means"
    );
}

#[test]
fn a_model_node_names_the_exact_model_it_is_granted() {
    let adapted = from_agent(&rag_agent()).expect("the agent translates");
    let model = adapted
        .definition
        .graph
        .nodes()
        .find(|node| node.id.as_str() == "model")
        .expect("the model node");
    let provider = model.provider.as_ref().expect("a binding");
    assert_eq!(provider.selection, "small-model");
    assert_eq!(
        model.parameters.get(capsulet_ir::agent::KIND_PARAMETER),
        Some(&"llm".to_string())
    );
}

#[test]
fn a_planner_graph_is_refused_rather_than_run_in_static_order() {
    let graph = GraphDefinition::new(
        GraphId::new("graph_planner").expect("valid"),
        "Planner",
        rag_nodes(),
        rag_edges(),
        GraphTransitionPolicy::planner(vec![NodeId::new("model").expect("valid")]),
    )
    .expect("the graph builds");
    assert!(refusal_of(graph).contains("planner"));
}

#[test]
fn a_typed_input_nothing_feeds_is_refused_by_name() {
    let graph = GraphDefinition::new(
        GraphId::new("graph_unfed").expect("valid"),
        "Unfed",
        rag_nodes(),
        rag_edges()[1..].to_vec(),
        GraphTransitionPolicy::static_acyclic(),
    )
    .expect("the graph builds");
    assert!(refusal_of(graph).contains("`query`"));
}

#[test]
fn a_port_fed_twice_is_refused() {
    let mut edges = rag_edges();
    edges.push(edge(
        "query-again",
        state("other", PortValueType::UserQuery),
        at("prompt", "query"),
    ));
    let graph = GraphDefinition::new(
        GraphId::new("graph_twice").expect("valid"),
        "Twice",
        rag_nodes(),
        edges,
        GraphTransitionPolicy::static_acyclic(),
    )
    .expect("the graph builds");
    assert!(refusal_of(graph).contains("more than one edge"));
}

#[test]
fn a_model_node_without_a_model_is_refused() {
    let mut nodes = rag_nodes();
    nodes[1] = nodes[1].clone().with_config(BTreeMap::new());
    let graph = GraphDefinition::new(
        GraphId::new("graph_nameless").expect("valid"),
        "Nameless",
        nodes,
        rag_edges(),
        GraphTransitionPolicy::static_acyclic(),
    )
    .expect("the graph builds");
    assert!(refusal_of(graph).contains("`model`"));
}

#[test]
fn a_validator_with_no_rule_is_refused() {
    let mut nodes = rag_nodes();
    nodes[2] = nodes[2].clone().with_config(BTreeMap::new());
    let graph = GraphDefinition::new(
        GraphId::new("graph_ruleless").expect("valid"),
        "Ruleless",
        nodes,
        rag_edges(),
        GraphTransitionPolicy::static_acyclic(),
    )
    .expect("the graph builds");
    assert!(refusal_of(graph).contains("check nothing"));
}

#[test]
fn job_and_memory_write_nodes_admit_with_keyed_effects() {
    let job = GraphNode::new(
        NodeId::new("job").expect("valid"),
        "Job",
        GraphNodeKind::Job,
        vec![port("result", PortDirection::Output, PortValueType::Json)],
    )
    .with_config(config(&[("job_definition_id", "job_build")]));
    let write = GraphNode::new(
        NodeId::new("write").expect("valid"),
        "Write",
        GraphNodeKind::MemoryWrite,
        vec![port("claim", PortDirection::Input, PortValueType::Json)],
    );
    let graph = GraphDefinition::new(
        GraphId::new("graph_effects").expect("valid"),
        "Effects",
        vec![job, write],
        vec![edge("job-write", at("job", "result"), at("write", "claim"))],
        GraphTransitionPolicy::static_acyclic(),
    )
    .expect("the graph builds");
    let adapted = from_agent(&agent_of(graph, vec![TerminationCondition::ValidatorPass]))
        .expect("the agent translates");
    if let Err(refusal) = admit(&adapted.definition) {
        panic!("effect nodes must admit: {refusal}");
    }
    for node in adapted.definition.graph.nodes() {
        for effect in &node.effects {
            assert!(
                matches!(effect.idempotency, Idempotency::Keyed { .. }),
                "`{}` must say how a retry avoids a duplicate",
                effect.id
            );
        }
    }
}

#[test]
fn the_published_coverage_report_matches_the_adapters() {
    use std::fmt::Write as _;

    let mut expected = String::new();
    for row in coverage() {
        let _ = writeln!(
            expected,
            "| {} | {} | {} |",
            row.construct,
            row.level.as_str(),
            row.note
        );
    }

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/contracts/ir-adapter-coverage.md");

    if std::env::var_os("CAPSULET_UPDATE_GOLDEN").is_some() {
        let document = std::fs::read_to_string(&path).unwrap_or_default();
        let head = document
            .split_once("<!-- generated: adapter coverage -->")
            .map(|(head, _)| head.to_string())
            .unwrap_or_default();
        std::fs::write(
            &path,
            format!(
                "{head}<!-- generated: adapter coverage -->\n\n| Construct | Carries | Notes |\n| --- | --- | --- |\n{expected}"
            ),
        )
        .expect("the coverage report is writable");
    }

    let published = std::fs::read_to_string(&path).expect("the coverage report is readable");
    assert!(
        published.contains(&expected),
        "the published coverage report has drifted from what the adapters actually do"
    );
}

#[test]
fn every_unsupported_construct_is_declared_rather_than_approximated() {
    let unsupported: Vec<&str> = coverage()
        .iter()
        .filter(|row| row.level == CoverageLevel::Unsupported)
        .map(|row| row.construct)
        .collect();

    // The two things M2 deliberately does not translate. If either becomes
    // supported, this test is where that decision gets recorded.
    assert!(unsupported.contains(&"workflow run and step run state"));
    assert!(unsupported.contains(&"automation trigger"));
}

#[test]
fn nothing_that_executes_depends_on_the_adapters() {
    // The adapters describe; they do not run anything. The API is the one
    // deliberate exception: starting an agent run compiles the agent, and the graph
    // worker runs the result. It runs definitions, not adapters, so it stays off
    // this list's other side too.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for crate_name in [
        "application",
        "graph-worker",
        "evaluator",
        "runner",
        "scheduler",
        "worker",
    ] {
        let manifest = root.join("crates").join(crate_name).join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest)
            .unwrap_or_else(|_| panic!("{} is readable", manifest.display()));
        assert!(
            !text.contains("capsulet-ir-adapters"),
            "`{crate_name}` depends on the adapters; only the API compiles agents"
        );
    }
}
