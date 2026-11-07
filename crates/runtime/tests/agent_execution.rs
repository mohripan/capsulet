//! What running real work needs from the log: inputs a run was given, what its
//! nodes spent, and a person's answer at a gate.

mod fixtures;

use std::collections::{BTreeMap, BTreeSet};

use capsulet_ir::digest::Digest;
use capsulet_ir::loop_region::{FailureKind, IterationRecord, LoopBudget, RepairRoute, Route};
use capsulet_runtime::event::{NodeUsage, RunEvent, RunFailure, Wait};
use capsulet_runtime::state::RunState;
use capsulet_runtime::wait::{Signal, SignalKind, WaitError, resume};
use capsulet_runtime::{Decision, decide};
use fixtures::{LoopShape, admitted, at, event, id, loop_definition_with};

fn started() -> Vec<capsulet_runtime::RecordedEvent> {
    let mut events = admitted();
    events.push(event(1, RunEvent::Started { by: id("worker") }));
    events
}

fn usage(input: Option<u64>, output: Option<u64>, cost: Option<u64>) -> NodeUsage {
    NodeUsage {
        provider: Some("ollama".to_string()),
        model: Some("small-model".to_string()),
        input_tokens: input,
        output_tokens: output,
        cost_micro_units: cost,
        latency_ms: Some(12),
    }
}

#[test]
fn a_run_reads_back_the_inputs_it_was_bound_to() {
    let query = Digest::of(b"what is capsulet?");
    let mut events = started();
    let position = u64::try_from(events.len()).expect("a short log");
    events.push(event(
        position,
        RunEvent::InputsBound {
            inputs: BTreeMap::from([("query".to_string(), query)]),
        },
    ));

    let state = RunState::fold(&events).expect("folds");
    assert_eq!(state.input("query"), Some(query));
    assert_eq!(state.input("anything-else"), None);
}

#[test]
fn inputs_are_bound_once() {
    let inputs = BTreeMap::from([("query".to_string(), Digest::of(b"one"))]);
    let mut events = started();
    let position = u64::try_from(events.len()).expect("a short log");
    events.push(event(
        position,
        RunEvent::InputsBound {
            inputs: inputs.clone(),
        },
    ));
    events.push(event(position + 1, RunEvent::InputsBound { inputs }));

    assert!(
        RunState::fold(&events).is_err(),
        "a run has one set of inputs, or nobody can say what it ran on"
    );
}

#[test]
fn what_a_node_consumed_counts_as_soon_as_it_is_recorded() {
    let mut events = started();
    let position = u64::try_from(events.len()).expect("a short log");
    events.push(event(
        position,
        RunEvent::ResourcesConsumed {
            node: id("check"),
            usage: usage(Some(30), Some(12), Some(500)),
        },
    ));

    let spent = RunState::fold(&events).expect("folds").spent();
    assert_eq!(spent.tokens, 42);
    assert_eq!(spent.cost_micro_units, 500);
    assert_eq!(spent.unmeasured, 0);
}

#[test]
fn usage_nobody_measured_is_counted_as_unknown_rather_than_as_zero() {
    let mut events = started();
    let position = u64::try_from(events.len()).expect("a short log");
    events.push(event(
        position,
        RunEvent::ResourcesConsumed {
            node: id("check"),
            usage: usage(None, None, None),
        },
    ));

    let spent = RunState::fold(&events).expect("folds").spent();
    assert_eq!(spent.tokens, 0);
    assert_eq!(
        spent.unmeasured, 1,
        "zero tokens and nobody-measured-it are different statements"
    );
}

#[test]
fn an_iteration_record_does_not_count_what_its_nodes_already_reported() {
    let mut events = started();
    let mut position = u64::try_from(events.len()).expect("a short log");
    let mut push = |events: &mut Vec<_>, body| {
        events.push(event(position, body));
        position += 1;
    };
    push(
        &mut events,
        RunEvent::IterationStarted {
            region: id("attempts"),
            index: 0,
            members: BTreeSet::from([id("check")]),
        },
    );
    push(
        &mut events,
        RunEvent::ResourcesConsumed {
            node: id("check"),
            usage: usage(Some(40), Some(2), Some(9)),
        },
    );

    let open = RunState::fold(&events).expect("folds");
    assert_eq!(open.iteration_usage(&id("attempts")), (42, 9));

    push(
        &mut events,
        RunEvent::IterationFinished {
            region: id("attempts"),
            record: Box::new(IterationRecord {
                index: 0,
                state_in: Digest::of(b"in"),
                state_out: Digest::of(b"out"),
                invariants: vec![],
                progress: None,
                spent: LoopBudget {
                    max_iterations: 1,
                    wall_ms: 5,
                    tokens: 42,
                    cost_micro_units: 9,
                    effect_count: 0,
                },
            }),
        },
    );

    let spent = RunState::fold(&events).expect("folds").spent();
    assert_eq!(spent.tokens, 42, "the same tokens must not count twice");
    assert_eq!(spent.cost_micro_units, 9);
    assert_eq!(spent.wall_ms, 5);
}

#[test]
fn a_cancellation_signal_is_not_a_resumption() {
    let signal = Signal::cancellation(id("operator"));
    assert_eq!(signal.kind, SignalKind::Cancel);

    let waiting = Wait::Event {
        name: id("webhook"),
    };
    assert!(matches!(
        resume(Some(&waiting), &signal, at(0)),
        Err(WaitError::Mismatch { .. })
    ));
}

fn gate_definition() -> capsulet_ir::Definition {
    loop_definition_with(LoopShape {
        max_iterations: 8,
        repairs: vec![RepairRoute {
            failure: FailureKind::InterpretationResidual,
            route: Route::Escalate {
                to: id("domain-owner"),
            },
        }],
        ..LoopShape::plain()
    })
}

/// A run suspended at a gate because `check` could not settle something.
fn suspended_at_gate() -> Vec<capsulet_runtime::RecordedEvent> {
    let mut events = started();
    let position = u64::try_from(events.len()).expect("a short log");
    let wait = Wait::HumanGate {
        node: id("check"),
        obligation: id("domain-owner"),
    };
    events.push(event(position, RunEvent::NodeStarted { node: id("check") }));
    events.push(event(
        position + 1,
        RunEvent::NodeFailed {
            node: id("check"),
            failure: FailureKind::InterpretationResidual,
            detail: "the wording is ambiguous".to_string(),
        },
    ));
    events.push(event(position + 2, RunEvent::Suspended { wait }));
    events
}

fn authorised() -> BTreeSet<capsulet_ir::Identifier> {
    BTreeSet::from([id("domain-owner")])
}

#[test]
fn approval_answers_the_failure_instead_of_asking_again() {
    let definition = gate_definition();
    let mut events = suspended_at_gate();
    let waiting_state = RunState::fold(&events).expect("folds");
    let decided = resume(
        waiting_state.waiting_on(),
        &Signal::decision(id("domain-owner"), id("reviewer"), authorised()),
        at(0),
    )
    .expect("the reviewer holds the authority");
    assert!(matches!(decided, RunEvent::Resumed { .. }));

    let position = u64::try_from(events.len()).expect("a short log");
    events.push(event(position, decided));
    let state = RunState::fold(&events).expect("folds");

    assert_eq!(state.approved_by(&id("check")), Some(&id("reviewer")));
    assert!(
        !matches!(decide(&definition, &state, at(0)), Decision::Suspend { .. }),
        "asking the same person the same question forever is not a decision"
    );
}

#[test]
fn rejection_ends_the_run_through_the_decision_core() {
    let definition = gate_definition();
    let mut events = suspended_at_gate();
    let waiting_state = RunState::fold(&events).expect("folds");
    let decided = resume(
        waiting_state.waiting_on(),
        &Signal::rejection(id("domain-owner"), id("reviewer"), authorised()),
        at(0),
    )
    .expect("the reviewer holds the authority");
    assert!(matches!(decided, RunEvent::GateRejected { .. }));

    let position = u64::try_from(events.len()).expect("a short log");
    events.push(event(position, decided));
    let state = RunState::fold(&events).expect("folds");

    assert_eq!(
        decide(&definition, &state, at(0)),
        Decision::Fail {
            reason: RunFailure::GateRejected {
                node: id("check"),
                obligation: id("domain-owner"),
                by: id("reviewer"),
            },
        },
        "going through the decision core is what compensates anything the run still owes"
    );
}

#[test]
fn nobody_without_the_authority_can_reject_either() {
    let events = suspended_at_gate();
    let state = RunState::fold(&events).expect("folds");
    assert!(matches!(
        resume(
            state.waiting_on(),
            &Signal::rejection(id("domain-owner"), id("bystander"), BTreeSet::new()),
            at(0),
        ),
        Err(WaitError::Unauthorised { .. })
    ));
}

#[test]
fn a_new_iteration_clears_an_approval_for_the_nodes_it_resets() {
    let mut events = suspended_at_gate();
    let state = RunState::fold(&events).expect("folds");
    let decided = resume(
        state.waiting_on(),
        &Signal::decision(id("domain-owner"), id("reviewer"), authorised()),
        at(0),
    )
    .expect("authorised");
    let position = u64::try_from(events.len()).expect("a short log");
    events.push(event(position, decided));
    events.push(event(
        position + 1,
        RunEvent::IterationStarted {
            region: id("attempts"),
            index: 0,
            members: BTreeSet::from([id("check")]),
        },
    ));

    let state = RunState::fold(&events).expect("folds");
    assert_eq!(
        state.approved_by(&id("check")),
        None,
        "approving one iteration's answer says nothing about the next one's"
    );
}

#[test]
fn the_outputs_of_the_last_iteration_survive_the_reset() {
    let output = Digest::of(b"carried");
    let mut events = started();
    let position = u64::try_from(events.len()).expect("a short log");
    events.push(event(position, RunEvent::NodeStarted { node: id("check") }));
    events.push(event(
        position + 1,
        RunEvent::NodeFinished {
            node: id("check"),
            outputs: BTreeMap::from([("answer".to_string(), output)]),
            control: BTreeMap::new(),
        },
    ));
    events.push(event(
        position + 2,
        RunEvent::IterationStarted {
            region: id("attempts"),
            index: 1,
            members: BTreeSet::from([id("check")]),
        },
    ));

    let state = RunState::fold(&events).expect("folds");
    assert!(
        state.outputs_of(&id("check")).is_empty(),
        "the node has not run in this iteration"
    );
    assert_eq!(
        state.previous_outputs_of(&id("check")).get("answer"),
        Some(&output),
        "state a loop carries forward has to come from somewhere"
    );
}

#[test]
fn an_integer_reading_survives_being_written_and_read_back() {
    let event = RunEvent::NodeFinished {
        node: id("check"),
        outputs: BTreeMap::new(),
        control: BTreeMap::from([(
            "distinct-results".to_string(),
            capsulet_runtime::event::ControlValue::Integer { value: 3 },
        )]),
    };
    let written = serde_json::to_string(&event).expect("writes");
    let read: RunEvent = serde_json::from_str(&written)
        .expect("a reading a loop stops on has to come back after a restart");
    assert_eq!(read, event);
}
