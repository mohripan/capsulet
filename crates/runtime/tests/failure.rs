//! Stopping, and the ways of doing it badly.
//!
//! Each test here is a moment where the convenient answer is wrong: stop now,
//! time it out now, end the run now. What makes them worth writing down is that
//! the convenient answer is also the one a reasonable person implements first.

mod fixtures;

use capsulet_ir::Digest;
use capsulet_ir::correctness::evidence::RecordedTime;
use capsulet_ir::effect::{Idempotency, Reversibility};
use capsulet_ir::loop_region::{FailureKind, RepairRoute, Route};
use capsulet_runtime::failure::{
    self, Compensation, request_cancellation, run_deadline_passed, safe_to_stop, timed_out_node,
};
use capsulet_runtime::{Decision, RunEvent, RunFailure, RunState, decide};

use fixtures::{
    LoopShape, admitted, at, effect_definition, event, id, loop_definition_with,
    pipeline_definition, reversible_effect_definition,
};

/// Admitted and started at the fixed moment the fixtures use.
fn started() -> Vec<capsulet_runtime::RecordedEvent> {
    let mut events = admitted();
    events.push(event(1, RunEvent::Started { by: id("worker-1") }));
    events
}

#[test]
fn cancellation_stops_at_the_next_safe_point_and_not_inside_an_effect() {
    let definition = effect_definition(Idempotency::Idempotent);
    let mut events = started();
    events.push(event(
        2,
        RunEvent::CancellationRequested { by: id("operator") },
    ));

    // Nothing in flight: stopping now is safe, and the run stops.
    let quiet = RunState::fold(&events).expect("folds");
    assert!(safe_to_stop(&quiet));
    assert_eq!(
        decide(&definition, &quiet, at(0)),
        Decision::Cancel { by: id("operator") }
    );

    // Now with an effect claimed and unresolved. Ending here would leave
    // something nobody can ever account for.
    let mut mid_effect = started();
    mid_effect.push(event(
        2,
        RunEvent::NodeStarted {
            node: id("publish"),
        },
    ));
    mid_effect.push(event(
        3,
        RunEvent::EffectClaimed {
            node: id("publish"),
            effect: id("open-pull-request"),
            attempt: 0,
            key: None,
        },
    ));
    mid_effect.push(event(
        4,
        RunEvent::CancellationRequested { by: id("operator") },
    ));

    let state = RunState::fold(&mid_effect).expect("folds");
    assert!(!safe_to_stop(&state));
    assert_eq!(
        decide(&definition, &state, at(0)),
        Decision::RetryClaimedEffect {
            node: id("publish"),
            effect: id("open-pull-request"),
            attempt: 1,
        },
        "the outstanding claim is resolved first; cancelling on top of it would strand the effect"
    );
}

#[test]
fn cancellation_stops_before_an_effect_that_has_not_started() {
    let definition = effect_definition(Idempotency::NonIdempotent);
    let mut events = started();
    events.push(event(
        2,
        RunEvent::CancellationRequested { by: id("operator") },
    ));

    let state = RunState::fold(&events).expect("folds");
    assert_ne!(
        decide(&definition, &state, at(0)),
        Decision::PerformEffect {
            node: id("publish"),
            effect: id("open-pull-request"),
            attempt: 0,
        },
        "an effect nobody has started yet must not happen after somebody asked to stop"
    );
}

#[test]
fn a_retry_route_is_honoured_up_to_the_attempts_it_declared_and_no_further() {
    let definition = loop_definition_with(LoopShape {
        max_iterations: 8,
        repairs: vec![RepairRoute {
            failure: FailureKind::BudgetExhaustion,
            route: Route::Retry {
                node: id("check"),
                attempts: 1,
            },
        }],
        ..LoopShape::plain()
    });

    let mut events = started();
    events.push(event(2, RunEvent::NodeStarted { node: id("check") }));
    events.push(event(
        3,
        RunEvent::NodeFailed {
            node: id("check"),
            failure: FailureKind::BudgetExhaustion,
            detail: "took too long".to_string(),
        },
    ));

    let once = RunState::fold(&events).expect("folds");
    assert_eq!(
        decide(&definition, &once, at(0)),
        Decision::StartNode { node: id("check") },
        "one retry is what was declared"
    );

    events.push(event(4, RunEvent::NodeStarted { node: id("check") }));
    events.push(event(
        5,
        RunEvent::NodeFailed {
            node: id("check"),
            failure: FailureKind::BudgetExhaustion,
            detail: "took too long again".to_string(),
        },
    ));
    let twice = RunState::fold(&events).expect("folds");
    assert!(matches!(
        decide(&definition, &twice, at(0)),
        Decision::StopLoop { .. }
    ));
}

#[test]
fn a_node_that_outran_its_budget_becomes_a_typed_failure() {
    let definition = pipeline_definition();
    let mut events = started();
    events.push(event(
        2,
        RunEvent::NodeStarted {
            node: id("normalize"),
        },
    ));

    let state = RunState::fold(&events).expect("folds");
    let started_at = events[2].at.epoch_millis();

    // The fixture node declares a 1000ms budget.
    assert!(timed_out_node(&definition, &state, RecordedTime(started_at + 999)).is_none());
    let timed_out = timed_out_node(&definition, &state, RecordedTime(started_at + 1_000))
        .expect("the deadline passed");
    assert_eq!(timed_out.node, id("normalize"));
    assert_eq!(timed_out.deadline_ms, 1_000);

    assert_eq!(
        decide(&definition, &state, RecordedTime(started_at + 5_000)),
        Decision::TimeOutNode {
            node: id("normalize")
        }
    );

    // A timeout is a typed failure rather than a bare stop, so a loop that
    // declared a route for it can answer it like any other failure.
    let RunEvent::NodeFailed { failure, .. } = failure::timed_out(&timed_out) else {
        panic!("a timeout is recorded as a node failure");
    };
    assert_eq!(failure, FailureKind::BudgetExhaustion);
}

#[test]
fn a_node_deadline_is_measured_from_the_log_not_from_when_a_worker_took_over() {
    // The reattach case: a worker inherits a node another worker started long
    // ago. Restarting the clock would let a stuck node run forever, one
    // handover at a time.
    let definition = pipeline_definition();
    let mut events = started();
    events.push(event(
        2,
        RunEvent::NodeStarted {
            node: id("normalize"),
        },
    ));

    let state = RunState::fold(&events).expect("folds");
    let started_at = state
        .running()
        .get(&id("normalize"))
        .copied()
        .expect("the node is running");
    assert_eq!(started_at, events[2].at);
    assert!(
        timed_out_node(
            &definition,
            &state,
            RecordedTime(started_at.epoch_millis() + 1)
        )
        .is_none()
    );
    assert!(
        timed_out_node(
            &definition,
            &state,
            RecordedTime(started_at.epoch_millis() + 60_000)
        )
        .is_some()
    );
}

#[test]
fn a_run_that_outran_its_wall_time_stops() {
    let definition = pipeline_definition();
    let events = started();
    let state = RunState::fold(&events).expect("folds");
    let began = state.started_at().expect("the run started").epoch_millis();

    assert!(!run_deadline_passed(
        &definition,
        &state,
        RecordedTime(began + 1)
    ));
    assert!(run_deadline_passed(
        &definition,
        &state,
        RecordedTime(began + 600_000)
    ));
    assert_eq!(
        decide(&definition, &state, RecordedTime(began + 600_000)),
        Decision::Fail {
            reason: RunFailure::BudgetExhausted {
                resource: "wall time".to_string()
            }
        }
    );
}

#[test]
fn a_reversible_effect_that_happened_is_compensated_before_the_run_ends() {
    let definition = reversible_effect_definition();
    let mut events = started();
    events.push(event(
        2,
        RunEvent::NodeStarted {
            node: id("publish"),
        },
    ));
    events.push(event(
        3,
        RunEvent::EffectClaimed {
            node: id("publish"),
            effect: id("open-pull-request"),
            attempt: 0,
            key: None,
        },
    ));
    events.push(event(
        4,
        RunEvent::EffectFinalized {
            node: id("publish"),
            effect: id("open-pull-request"),
            attempt: 0,
            receipt: Digest::of(b"pull request 41"),
        },
    ));
    events.push(event(
        5,
        RunEvent::NodeFinished {
            node: id("publish"),
            outputs: std::collections::BTreeMap::new(),
            control: std::collections::BTreeMap::new(),
        },
    ));
    events.push(event(
        6,
        RunEvent::CancellationRequested { by: id("operator") },
    ));

    let state = RunState::fold(&events).expect("folds");
    let owed = Compensation {
        node: id("publish"),
        effect: id("open-pull-request"),
        route: id("close-pull-request"),
    };
    assert_eq!(
        failure::pending_compensations(&definition, &state),
        vec![owed.clone()]
    );
    assert_eq!(
        decide(&definition, &state, at(0)),
        Decision::Compensate {
            compensation: owed.clone()
        },
        "the run still owes what it published; ending first would leave it published"
    );

    events.push(event(
        7,
        RunEvent::Compensated {
            node: id("publish"),
            effect: id("open-pull-request"),
            compensation: id("close-pull-request"),
            receipt: Digest::of(b"closed"),
        },
    ));
    let settled = RunState::fold(&events).expect("folds");
    assert!(failure::pending_compensations(&definition, &settled).is_empty());
    assert_eq!(
        decide(&definition, &settled, at(0)),
        Decision::Cancel { by: id("operator") }
    );
}

#[test]
fn an_irreversible_effect_is_never_pretended_to_be_undone() {
    let definition = effect_definition(Idempotency::Idempotent);
    assert!(matches!(
        definition
            .graph
            .node(&id("publish"))
            .expect("the node exists")
            .effects[0]
            .reversibility,
        Reversibility::Irreversible
    ));

    let mut events = started();
    events.push(event(
        2,
        RunEvent::NodeStarted {
            node: id("publish"),
        },
    ));
    events.push(event(
        3,
        RunEvent::EffectClaimed {
            node: id("publish"),
            effect: id("open-pull-request"),
            attempt: 0,
            key: None,
        },
    ));
    events.push(event(
        4,
        RunEvent::EffectFinalized {
            node: id("publish"),
            effect: id("open-pull-request"),
            attempt: 0,
            receipt: Digest::of(b"pull request 41"),
        },
    ));
    events.push(event(
        5,
        RunEvent::CancellationRequested { by: id("operator") },
    ));

    let state = RunState::fold(&events).expect("folds");
    assert!(
        failure::pending_compensations(&definition, &state).is_empty(),
        "there is no route to undo it, and inventing one would be a lie about what happened"
    );
    assert_eq!(
        decide(&definition, &state, at(0)),
        Decision::Cancel { by: id("operator") }
    );
}

#[test]
fn an_effect_that_never_happened_is_not_compensated() {
    let definition = reversible_effect_definition();
    let mut events = started();
    events.push(event(
        2,
        RunEvent::NodeStarted {
            node: id("publish"),
        },
    ));
    events.push(event(
        3,
        RunEvent::EffectClaimed {
            node: id("publish"),
            effect: id("open-pull-request"),
            attempt: 0,
            key: None,
        },
    ));
    events.push(event(
        4,
        RunEvent::EffectUncertain {
            node: id("publish"),
            effect: id("open-pull-request"),
            attempt: 0,
        },
    ));

    let state = RunState::fold(&events).expect("folds");
    assert!(
        failure::pending_compensations(&definition, &state).is_empty(),
        "undoing something that may never have happened is its own way of causing damage"
    );
}

#[test]
fn escalation_suspends_rather_than_failing() {
    let definition = loop_definition_with(LoopShape {
        max_iterations: 8,
        repairs: vec![RepairRoute {
            failure: FailureKind::InterpretationResidual,
            route: Route::Escalate {
                to: id("domain-owner"),
            },
        }],
        ..LoopShape::plain()
    });

    let mut events = started();
    events.push(event(2, RunEvent::NodeStarted { node: id("check") }));
    events.push(event(
        3,
        RunEvent::NodeFailed {
            node: id("check"),
            failure: FailureKind::InterpretationResidual,
            detail: "the wording is ambiguous and no rule settles it".to_string(),
        },
    ));

    let state = RunState::fold(&events).expect("folds");
    assert!(
        matches!(decide(&definition, &state, at(0)), Decision::Suspend { .. }),
        "an unresolved question is a reason to ask a person, not a reason to fail the run"
    );
}

#[test]
fn a_cancellation_request_is_recorded_rather_than_acted_on_by_whoever_asked() {
    let asked = request_cancellation(id("operator"));
    assert_eq!(
        asked,
        RunEvent::CancellationRequested { by: id("operator") }
    );
    assert!(
        !asked.is_terminal(),
        "asking is not stopping; the run decides where it is safe to stop"
    );
    assert!(failure::cancelled(id("operator")).is_terminal());
}
