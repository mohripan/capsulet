//! What the adapters carry, what they lose, and what they refuse.
//!
//! The differential tests matter more than the happy path: a translation is
//! only useful if it accepts what today's models accept and refuses what they
//! refuse. A translator that quietly admits something the source rejects is
//! worse than no translator, because it launders the rejection.

use capsulet_core::{
    AgentBudget, AgentDefinition, AgentId, AgentTerminationPolicy, ExecutionPoolName,
    GraphDefinition, JobDefinitionId, TerminationCondition, WorkflowDefinition,
    WorkflowDependencyPolicy, WorkflowGraph, WorkflowId, WorkflowStatus, WorkflowStep,
    WorkflowStepDependency, WorkflowStepId,
};
use capsulet_ir::admission::AdmissionCode;
use capsulet_ir::definition::AssuranceMode;
use capsulet_ir::node::NodeKind;
use capsulet_ir::region::RegionKind;
use capsulet_ir::{admit, digest_of};
use capsulet_ir_adapters::{CoverageLevel, coverage, from_agent, from_graph, from_workflow};

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

#[test]
fn an_agent_becomes_a_bounded_loop_with_its_budget() {
    let agent = AgentDefinition::new(
        AgentId::new("agent_rag").expect("valid"),
        "RAG agent",
        agent_graph(),
        Some(AgentBudget::new(8, 40_000, 300, 250_000).expect("the budget is valid")),
        Some(AgentTerminationPolicy::new(vec![
            TerminationCondition::ValidatorPass,
            TerminationCondition::NoProgress,
            TerminationCondition::HumanEscalation,
        ])),
    )
    .expect("the agent is valid");

    let adapted = from_agent(&agent).expect("the agent translates");
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

    // The placeholder condition is recorded rather than presented as fidelity.
    assert!(
        adapted
            .notes
            .iter()
            .any(|note| note.detail.contains("placeholder condition node")),
        "{:?}",
        adapted.notes
    );
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
    // The adapters describe; they do not run anything. If an execution crate
    // starts depending on them, that is a decision worth making deliberately.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for crate_name in [
        "api",
        "application",
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
            "`{crate_name}` depends on the adapters; M2 wires nothing into execution"
        );
    }
}
