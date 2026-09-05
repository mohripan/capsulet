//! Loops that a restart cannot reset, stated as tests.
//!
//! Each of these is the same shape: spend something, crash, fold the log, and
//! check that the loop is where it was rather than where it started. A budget
//! a restart hands back is not a budget, and the only way to be sure a restart
//! does not hand it back is to test the restart.

mod fixtures;

use capsulet_ir::loop_region::{
    BudgetKind, FailureKind, InvariantOutcome, InvariantTiming, ProgressDirection, ProgressMeasure,
    RepairRoute, Route, StopReason,
};
use capsulet_runtime::event::RunFailure;
use capsulet_runtime::{Decision, RunEvent, RunState, Wait, decide, loops};

use fixtures::{
    LoopShape, admitted, at, event, fixture_invariant, fixture_progress, id, iteration,
    loop_definition_with,
};

/// Admitted, started, and inside the loop.
fn running() -> Vec<capsulet_runtime::RecordedEvent> {
    let mut events = admitted();
    events.push(event(1, RunEvent::Started { by: id("worker-1") }));
    events
}

/// Appends a started/finished iteration pair at the next positions.
fn ran_iteration(
    events: &mut Vec<capsulet_runtime::RecordedEvent>,
    index: u32,
    progress: Option<i128>,
    invariant_held: Option<bool>,
) {
    let next = u64::from(index) * 2 + 2;
    events.push(event(
        next,
        RunEvent::IterationStarted {
            region: id("repair-loop"),
            index,
        },
    ));
    events.push(event(
        next + 1,
        RunEvent::IterationFinished {
            region: id("repair-loop"),
            record: Box::new(iteration(index, progress, invariant_held)),
        },
    ));
}

#[test]
fn iteration_counts_and_spending_survive_the_fold() {
    let mut events = running();
    ran_iteration(&mut events, 0, None, None);
    ran_iteration(&mut events, 1, None, None);

    let state = RunState::fold(&events).expect("folds");
    let progress = state.loop_progress(&id("repair-loop"));
    assert_eq!(progress.started, 2);
    assert_eq!(progress.finished, 2);
    assert_eq!(
        state.spent().wall_ms,
        20,
        "what each iteration spent is summed from the log, not held by a worker"
    );

    // Folding is the recovery path, so folding again must not double anything.
    let again = RunState::fold(&events).expect("folds");
    assert_eq!(again.spent(), state.spent());
    assert_eq!(again.loop_progress(&id("repair-loop")), progress);
}

#[test]
fn an_iteration_that_started_and_crashed_still_spent_an_iteration() {
    let definition = loop_definition_with(LoopShape {
        max_iterations: 2,
        ..LoopShape::plain()
    });
    let mut events = running();
    ran_iteration(&mut events, 0, None, None);
    // The second iteration began, and the worker died inside it.
    events.push(event(
        4,
        RunEvent::IterationStarted {
            region: id("repair-loop"),
            index: 1,
        },
    ));

    let state = RunState::fold(&events).expect("folds");
    assert_eq!(state.loop_progress(&id("repair-loop")).started, 2);
    assert_eq!(state.loop_progress(&id("repair-loop")).finished, 1);
    assert_eq!(
        decide(&definition, &state, at(0)),
        Decision::StopLoop {
            region: id("repair-loop"),
            reason: StopReason::BudgetExhausted {
                budget: BudgetKind::Iterations
            }
        },
        "counting only finished iterations would let a crash loop spend the budget forever"
    );
}

#[test]
fn an_invariant_that_did_not_hold_stops_the_loop_across_a_restart() {
    let definition = loop_definition_with(LoopShape {
        invariants: vec![fixture_invariant()],
        ..LoopShape::plain()
    });

    let mut events = running();
    ran_iteration(&mut events, 0, None, Some(true));
    let holding = RunState::fold(&events).expect("folds");
    assert_eq!(
        holding.loop_progress(&id("repair-loop")).failed_invariant,
        None
    );

    ran_iteration(&mut events, 1, None, Some(false));
    let state = RunState::fold(&events).expect("folds");
    assert_eq!(
        decide(&definition, &state, at(0)),
        Decision::StopLoop {
            region: id("repair-loop"),
            reason: StopReason::InvariantFailed {
                invariant: id("state-is-consistent")
            }
        }
    );
}

#[test]
fn a_loop_that_repaired_itself_is_not_stopped_for_the_failure_it_fixed() {
    let definition = loop_definition_with(LoopShape {
        invariants: vec![fixture_invariant()],
        ..LoopShape::plain()
    });

    let mut events = running();
    ran_iteration(&mut events, 0, None, Some(false));
    ran_iteration(&mut events, 1, None, Some(true));

    let state = RunState::fold(&events).expect("folds");
    assert_eq!(
        state.loop_progress(&id("repair-loop")).failed_invariant,
        None,
        "the invariant holds now, and stopping for a fixed problem would be wrong"
    );
    assert!(!matches!(
        decide(&definition, &state, at(0)),
        Decision::StopLoop {
            reason: StopReason::InvariantFailed { .. },
            ..
        }
    ));
}

#[test]
fn a_measure_that_has_not_moved_twice_running_is_non_progress() {
    let definition = loop_definition_with(LoopShape {
        max_iterations: 8,
        progress: Some(fixture_progress()),
        ..LoopShape::plain()
    });

    let mut events = running();
    ran_iteration(&mut events, 0, Some(10), None);
    ran_iteration(&mut events, 1, Some(7), None);
    let moving = RunState::fold(&events).expect("folds");
    assert!(!matches!(
        decide(&definition, &moving, at(0)),
        Decision::StopLoop { .. }
    ));

    ran_iteration(&mut events, 2, Some(7), None);
    ran_iteration(&mut events, 3, Some(7), None);
    let state = RunState::fold(&events).expect("folds");
    assert_eq!(state.loop_progress(&id("repair-loop")).stalled, 2);
    assert_eq!(
        decide(&definition, &state, at(0)),
        Decision::StopLoop {
            region: id("repair-loop"),
            reason: StopReason::NonProgress {
                measure: id("findings-remaining")
            }
        },
        "a measure that has stopped moving is going nowhere, restart or not"
    );
}

#[test]
fn a_measure_that_moved_the_wrong_way_is_non_progress_at_once() {
    let definition = loop_definition_with(LoopShape {
        max_iterations: 8,
        progress: Some(fixture_progress()),
        ..LoopShape::plain()
    });

    let mut events = running();
    ran_iteration(&mut events, 0, Some(5), None);
    ran_iteration(&mut events, 1, Some(9), None);

    let state = RunState::fold(&events).expect("folds");
    assert_eq!(
        state.loop_progress(&id("repair-loop")).stalled,
        0,
        "it moved, so it is not stalled — which is exactly why the direction has to be checked too"
    );
    assert_eq!(
        decide(&definition, &state, at(0)),
        Decision::StopLoop {
            region: id("repair-loop"),
            reason: StopReason::NonProgress {
                measure: id("findings-remaining")
            }
        },
        "a measure declared strictly decreasing that went up has contradicted its declaration"
    );
}

#[test]
fn direction_is_read_from_the_declaration_rather_than_assumed() {
    let increasing = ProgressMeasure {
        direction: ProgressDirection::StrictlyIncreasing,
        ..fixture_progress()
    };
    let definition = loop_definition_with(LoopShape {
        max_iterations: 8,
        progress: Some(increasing),
        ..LoopShape::plain()
    });

    let mut events = running();
    ran_iteration(&mut events, 0, Some(5), None);
    ran_iteration(&mut events, 1, Some(9), None);

    let state = RunState::fold(&events).expect("folds");
    assert!(
        !matches!(
            decide(&definition, &state, at(0)),
            Decision::StopLoop { .. }
        ),
        "the same readings are progress under the opposite declaration"
    );
}

#[test]
fn a_failure_takes_the_route_the_loop_declared_for_it() {
    let definition = loop_definition_with(LoopShape {
        max_iterations: 8,
        repairs: vec![RepairRoute {
            failure: FailureKind::SchemaMismatch,
            route: Route::Retry {
                node: id("check"),
                attempts: 2,
            },
        }],
        ..LoopShape::plain()
    });

    let mut events = running();
    events.push(event(2, RunEvent::NodeStarted { node: id("check") }));
    events.push(event(
        3,
        RunEvent::NodeFailed {
            node: id("check"),
            failure: FailureKind::SchemaMismatch,
            detail: "the model returned prose".to_string(),
        },
    ));

    let state = RunState::fold(&events).expect("folds");
    assert_eq!(
        decide(&definition, &state, at(0)),
        Decision::StartNode { node: id("check") },
        "the declared route says retry, so retrying is what may happen next"
    );
}

#[test]
fn a_retry_budget_spent_before_the_crash_stays_spent() {
    let definition = loop_definition_with(LoopShape {
        max_iterations: 8,
        repairs: vec![RepairRoute {
            failure: FailureKind::SchemaMismatch,
            route: Route::Retry {
                node: id("check"),
                attempts: 2,
            },
        }],
        ..LoopShape::plain()
    });

    let mut events = running();
    for attempt in 0..3_u64 {
        events.push(event(
            attempt * 2 + 2,
            RunEvent::NodeStarted { node: id("check") },
        ));
        events.push(event(
            attempt * 2 + 3,
            RunEvent::NodeFailed {
                node: id("check"),
                failure: FailureKind::SchemaMismatch,
                detail: "the model returned prose".to_string(),
            },
        ));
    }

    let state = RunState::fold(&events).expect("folds");
    assert_eq!(
        state.failure_count(&id("check"), FailureKind::SchemaMismatch),
        3,
        "the count comes from the log, so a restart cannot hand the route its attempts back"
    );
    assert_eq!(
        decide(&definition, &state, at(0)),
        Decision::StopLoop {
            region: id("repair-loop"),
            reason: StopReason::RepairExhausted {
                failure: FailureKind::SchemaMismatch
            }
        }
    );
}

#[test]
fn an_escalation_route_suspends_rather_than_spinning() {
    let definition = loop_definition_with(LoopShape {
        max_iterations: 8,
        repairs: vec![RepairRoute {
            failure: FailureKind::PolicyDenial,
            route: Route::Escalate {
                to: id("release-manager"),
            },
        }],
        ..LoopShape::plain()
    });

    let mut events = running();
    events.push(event(2, RunEvent::NodeStarted { node: id("check") }));
    events.push(event(
        3,
        RunEvent::NodeFailed {
            node: id("check"),
            failure: FailureKind::PolicyDenial,
            detail: "the change needs an authorised approver".to_string(),
        },
    ));

    let state = RunState::fold(&events).expect("folds");
    assert_eq!(
        decide(&definition, &state, at(0)),
        Decision::Suspend {
            wait: Wait::HumanGate {
                node: id("check"),
                obligation: id("release-manager"),
            }
        }
    );
}

#[test]
fn a_failure_the_loop_says_nothing_about_stops_the_run() {
    let definition = loop_definition_with(LoopShape {
        max_iterations: 8,
        repairs: vec![RepairRoute {
            failure: FailureKind::SchemaMismatch,
            route: Route::Retry {
                node: id("check"),
                attempts: 2,
            },
        }],
        ..LoopShape::plain()
    });

    let mut events = running();
    events.push(event(2, RunEvent::NodeStarted { node: id("check") }));
    events.push(event(
        3,
        RunEvent::NodeFailed {
            node: id("check"),
            failure: FailureKind::UnsafeEffect,
            detail: "the effect left the declared capability".to_string(),
        },
    ));

    let state = RunState::fold(&events).expect("folds");
    assert_eq!(
        decide(&definition, &state, at(0)),
        Decision::Fail {
            reason: RunFailure::Node {
                node: id("check"),
                failure: FailureKind::UnsafeEffect,
                detail: "the effect left the declared capability".to_string(),
            }
        },
        "silence in a declaration is not consent to retry"
    );
}

#[test]
fn a_failure_answered_by_starting_the_node_stops_being_unresolved() {
    let definition = loop_definition_with(LoopShape {
        max_iterations: 8,
        repairs: vec![RepairRoute {
            failure: FailureKind::SchemaMismatch,
            route: Route::Retry {
                node: id("check"),
                attempts: 2,
            },
        }],
        ..LoopShape::plain()
    });

    let mut events = running();
    events.push(event(2, RunEvent::NodeStarted { node: id("check") }));
    events.push(event(
        3,
        RunEvent::NodeFailed {
            node: id("check"),
            failure: FailureKind::SchemaMismatch,
            detail: "the model returned prose".to_string(),
        },
    ));
    events.push(event(4, RunEvent::NodeStarted { node: id("check") }));

    let state = RunState::fold(&events).expect("folds");
    assert!(
        state.unresolved_failures().is_empty(),
        "a node that is running again is not waiting for a decision"
    );
    assert_ne!(
        decide(&definition, &state, at(0)),
        Decision::StartNode { node: id("check") },
        "the retry is already in flight; deciding it again would start it twice"
    );
}

#[test]
fn the_recording_helpers_produce_the_events_the_fold_expects() {
    let started = loops::iteration_started(&id("repair-loop"), 0);
    let finished = loops::iteration_finished(&id("repair-loop"), iteration(0, Some(3), Some(true)));
    let stopped = loops::loop_stopped(&id("repair-loop"), StopReason::ConditionFalse);

    let mut events = running();
    for (offset, produced) in [started, finished, stopped].into_iter().enumerate() {
        events.push(event(2 + offset as u64, produced));
    }

    let state = RunState::fold(&events).expect("the helpers produce a foldable history");
    let progress = state.loop_progress(&id("repair-loop"));
    assert_eq!(progress.started, 1);
    assert_eq!(progress.finished, 1);
    assert_eq!(progress.last_progress, Some(3));
    assert_eq!(
        progress.stopped,
        Some(StopReason::ConditionFalse),
        "only this reason means the loop finished the work it set out to do"
    );
    assert!(progress.stopped.as_ref().expect("stopped").is_completion());
}

#[test]
fn an_invariant_outcome_that_held_is_not_a_failure() {
    // Guards the shape of the fold's reading of the record, which is easy to
    // invert and impossible to notice once inverted.
    let outcome = InvariantOutcome {
        invariant: id("state-is-consistent"),
        held: true,
        timing: InvariantTiming::AfterIteration,
    };
    assert!(outcome.held);

    let mut events = running();
    ran_iteration(&mut events, 0, None, Some(true));
    let state = RunState::fold(&events).expect("folds");
    assert_eq!(
        state.loop_progress(&id("repair-loop")).failed_invariant,
        None
    );
}
