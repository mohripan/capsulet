//! Loops have to declare their bounds, their invariants, and why they stopped.

use std::collections::{BTreeMap, BTreeSet};

use capsulet_ir::graph::{Combine, TrustDerivation};
use capsulet_ir::loop_region::{
    BudgetKind, Continuation, FailureKind, Invariant, InvariantOutcome, InvariantTiming,
    IterationRecord, LoopBudget, LoopError, LoopOutcome, LoopSpec, ProgressDirection,
    ProgressMeasure, RepairRoute, Route, StopReason,
};
use capsulet_ir::value::{IntegerRange, LengthBounds};
use capsulet_ir::{
    CapabilitySet, Digest, Endpoint, Graph, GraphBuilder, Hyperedge, Identifier, InputPort, Node,
    NodeKind, OutputPort, Region, RegionKind, ResourceBudget, ValueSchema,
};

fn id(value: &str) -> Identifier {
    Identifier::parse(value).expect("test identifiers are well formed")
}

fn text() -> ValueSchema {
    ValueSchema::Text {
        length: LengthBounds::new(0, 1_024),
    }
}

fn counter() -> ValueSchema {
    ValueSchema::Integer {
        range: IntegerRange::new(0, 1_000),
    }
}

fn node(name: &str, kind: NodeKind, inputs: Vec<InputPort>, outputs: Vec<OutputPort>) -> Node {
    Node {
        id: id(name),
        name: name.to_string(),
        kind,
        inputs,
        outputs,
        capabilities: vec![],
        effects: vec![],
        budget: ResourceBudget::deterministic(1_000),
        provider: None,
        sub_workflow: None,
    }
}

fn budget() -> LoopBudget {
    LoopBudget {
        max_iterations: 5,
        wall_ms: 60_000,
        tokens: 20_000,
        cost_micro_units: 5_000,
        effect_count: 0,
    }
}

fn spec() -> LoopSpec {
    let mut state = BTreeMap::new();
    state.insert("draft".to_string(), text());
    let mut exit = BTreeMap::new();
    exit.insert("accepted".to_string(), text());

    LoopSpec {
        state,
        exit,
        continuation: Continuation {
            evaluated_by: id("check"),
            port: id("keep-going"),
        },
        budget: budget(),
        invariants: vec![Invariant {
            id: id("draft-is-bounded"),
            description: "The draft never exceeds the size the reviewer accepts".to_string(),
            evaluator: id("check"),
            port: id("within-bounds"),
            timing: InvariantTiming::AfterIteration,
        }],
        progress: Some(ProgressMeasure {
            id: id("open-findings"),
            measured_by: id("check"),
            port: id("remaining"),
            direction: ProgressDirection::StrictlyDecreasing,
        }),
        repairs: vec![
            RepairRoute {
                failure: FailureKind::SchemaMismatch,
                route: Route::Retry {
                    node: id("revise"),
                    attempts: 2,
                },
            },
            RepairRoute {
                failure: FailureKind::InterpretationResidual,
                route: Route::Escalate { to: id("reviewer") },
            },
        ],
    }
}

/// A loop region: `enter -> revise -> check -> leave`, with `check` feeding
/// `revise` again, which is the cycle the loop declaration authorises.
fn loop_graph(spec: LoopSpec) -> GraphBuilder {
    let mut parts = GraphBuilder {
        nodes: vec![
            node(
                "enter",
                NodeKind::RegionEntry,
                vec![InputPort::new(id("in"), text())],
                vec![OutputPort::new(id("out"), text())],
            ),
            node(
                "revise",
                NodeKind::PureComputation,
                vec![InputPort::new(id("draft"), text())],
                vec![OutputPort::new(id("revised"), text())],
            ),
            node(
                "check",
                NodeKind::Verifier,
                vec![InputPort::new(id("candidate"), text())],
                vec![
                    OutputPort::new(id("keep-going"), ValueSchema::Bool),
                    OutputPort::new(id("within-bounds"), ValueSchema::Bool),
                    OutputPort::new(id("remaining"), counter()),
                    OutputPort::new(id("accepted"), text()),
                ],
            ),
            node(
                "leave",
                NodeKind::RegionExit,
                vec![InputPort::new(id("in"), text())],
                vec![OutputPort::new(id("out"), text())],
            ),
        ],
        ..GraphBuilder::default()
    };

    let forward = |name: &str, from: (&str, &str), to: (&str, &str)| Hyperedge {
        id: id(name),
        sources: vec![Endpoint::Port {
            node: id(from.0),
            port: id(from.1),
        }],
        targets: vec![Endpoint::Port {
            node: id(to.0),
            port: id(to.1),
        }],
        combine: Combine::Forward,
        trust: TrustDerivation::Weakest,
    };

    parts.edges = vec![
        forward("enter-to-revise", ("enter", "out"), ("revise", "draft")),
        forward(
            "revise-to-check",
            ("revise", "revised"),
            ("check", "candidate"),
        ),
        forward(
            "check-to-revise",
            ("check", "accepted"),
            ("revise", "draft"),
        ),
        forward("check-to-leave", ("check", "accepted"), ("leave", "in")),
    ];

    let mut nodes = BTreeSet::new();
    for member in ["enter", "revise", "check", "leave"] {
        nodes.insert(id(member));
    }
    parts.regions = vec![Region {
        id: id("repair-loop"),
        kind: RegionKind::Loop {
            spec: Box::new(spec),
        },
        parent: None,
        entry: id("enter"),
        exit: id("leave"),
        nodes,
        capabilities: CapabilitySet::empty(),
        budget: ResourceBudget::deterministic(60_000),
    }];
    parts
}

fn check(parts: GraphBuilder) -> Result<(), capsulet_ir::GraphError> {
    let graph = Graph::new(parts).expect("identifiers are distinct");
    graph.check(
        &CapabilitySet::empty(),
        &ResourceBudget::deterministic(120_000),
    )
}

#[test]
fn a_declared_loop_may_contain_the_cycle_it_declares() {
    assert_eq!(check(loop_graph(spec())), Ok(()));
}

#[test]
fn a_loop_without_an_iteration_bound_is_refused() {
    let mut declared = spec();
    declared.budget.max_iterations = 0;

    let error = check(loop_graph(declared)).expect_err("an unbounded loop is refused");
    assert!(error.to_string().contains("iterations"), "found {error}");
}

#[test]
fn a_loop_without_a_time_bound_is_refused() {
    let mut declared = spec();
    declared.budget.wall_ms = 0;

    let error = check(loop_graph(declared)).expect_err("an untimed loop is refused");
    assert!(error.to_string().contains("wall time"), "found {error}");
}

#[test]
fn a_continuation_condition_must_actually_be_a_condition() {
    let mut declared = spec();
    declared.continuation.port = id("accepted");

    let error = check(loop_graph(declared)).expect_err("text is not a condition");
    assert!(error.to_string().contains("not a boolean"), "found {error}");
}

#[test]
fn an_invariant_must_be_evaluated_inside_the_loop() {
    let mut declared = spec();
    declared.invariants[0].evaluator = id("outsider");

    let mut parts = loop_graph(declared);
    parts.nodes.push(node(
        "outsider",
        NodeKind::Verifier,
        vec![],
        vec![OutputPort::new(id("within-bounds"), ValueSchema::Bool)],
    ));

    let error =
        check(parts).expect_err("an invariant checked outside the loop is not an invariant");
    assert!(
        error.to_string().contains("not inside the loop"),
        "found {error}"
    );
}

#[test]
fn a_progress_measure_must_be_ordered() {
    let mut declared = spec();
    declared.progress = Some(ProgressMeasure {
        id: id("open-findings"),
        measured_by: id("check"),
        port: id("accepted"),
        direction: ProgressDirection::StrictlyDecreasing,
    });

    let error = check(loop_graph(declared)).expect_err("text cannot measure progress");
    assert!(
        error.to_string().contains("not an integer"),
        "found {error}"
    );
}

#[test]
fn a_retry_route_must_have_attempts() {
    let mut declared = spec();
    declared.repairs[0].route = Route::Retry {
        node: id("revise"),
        attempts: 0,
    };

    assert_eq!(
        declared.check(&id("repair-loop")),
        Err(LoopError::RetryWithoutAttempts {
            region: id("repair-loop"),
            failure: "schema_mismatch",
        })
    );
}

#[test]
fn a_failure_kind_may_have_only_one_route() {
    let mut declared = spec();
    declared.repairs.push(RepairRoute {
        failure: FailureKind::SchemaMismatch,
        route: Route::Reject,
    });

    assert_eq!(
        declared.check(&id("repair-loop")),
        Err(LoopError::DuplicateRoute {
            region: id("repair-loop"),
            failure: "schema_mismatch",
        })
    );
}

#[test]
fn repair_routes_are_looked_up_by_failure_kind() {
    let declared = spec();

    assert!(matches!(
        declared.route_for(FailureKind::SchemaMismatch),
        Some(Route::Retry { attempts: 2, .. })
    ));
    assert!(matches!(
        declared.route_for(FailureKind::InterpretationResidual),
        Some(Route::Escalate { .. })
    ));
    // Nothing routes a policy denial back to the model, and nothing pretends
    // there is a default.
    assert_eq!(declared.route_for(FailureKind::PolicyDenial), None);
}

#[test]
fn exhausting_a_budget_is_never_completion() {
    let exhausted = LoopOutcome {
        region: id("repair-loop"),
        iterations: vec![IterationRecord {
            index: 0,
            state_in: Digest::of(b"before"),
            state_out: Digest::of(b"after"),
            invariants: vec![InvariantOutcome {
                invariant: id("draft-is-bounded"),
                held: true,
                timing: InvariantTiming::AfterIteration,
            }],
            progress: Some(3),
            spent: budget(),
        }],
        stopped: StopReason::BudgetExhausted {
            budget: BudgetKind::Iterations,
        },
    };
    assert!(!exhausted.completed());
    assert_eq!(exhausted.stopped.as_str(), "budget_exhausted");

    let finished = LoopOutcome {
        stopped: StopReason::ConditionFalse,
        ..exhausted
    };
    assert!(finished.completed());
}

#[test]
fn non_progress_and_invariant_failure_are_distinct_stops() {
    let reasons = [
        StopReason::NonProgress {
            measure: id("open-findings"),
        },
        StopReason::InvariantFailed {
            invariant: id("draft-is-bounded"),
        },
        StopReason::RepairExhausted {
            failure: FailureKind::SchemaMismatch,
        },
        StopReason::EscalationRequired { to: id("reviewer") },
        StopReason::Cancelled { by: id("operator") },
    ];

    for reason in &reasons {
        assert!(
            !reason.is_completion(),
            "{} is not completion",
            reason.as_str()
        );
    }

    let names: BTreeSet<&str> = reasons.iter().map(StopReason::as_str).collect();
    assert_eq!(
        names.len(),
        reasons.len(),
        "each stop reason is distinguishable"
    );
}

#[test]
fn an_iteration_history_records_the_state_on_both_sides() {
    let record = IterationRecord {
        index: 2,
        state_in: Digest::of(b"iteration two, before"),
        state_out: Digest::of(b"iteration two, after"),
        invariants: vec![],
        progress: Some(1),
        spent: budget(),
    };

    assert_ne!(record.state_in, record.state_out);
    assert_eq!(record.index, 2);
}
