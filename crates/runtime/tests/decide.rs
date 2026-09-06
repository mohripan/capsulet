//! The properties recovery depends on, stated as tests.

mod fixtures;

use std::collections::BTreeMap;

use capsulet_ir::correctness::evidence::RecordedTime;
use capsulet_ir::loop_region::{BudgetKind, StopReason};
use capsulet_runtime::event::{ControlValue, RunFailure, Wait};
use capsulet_runtime::{Decision, Epoch, FoldError, RunEvent, RunState, RunStatus, decide};

use fixtures::{
    admitted, at, effect_definition, event, id, iteration, loop_definition, pipeline_definition,
};

#[test]
fn a_log_that_does_not_open_with_admission_is_refused() {
    assert_eq!(RunState::fold(&[]), Err(FoldError::NotAdmitted));

    let started = vec![event(0, RunEvent::Started { by: id("worker-1") })];
    assert_eq!(RunState::fold(&started), Err(FoldError::NotAdmitted));
}

#[test]
fn a_gap_in_the_log_is_refused_rather_than_skipped() {
    let mut events = admitted();
    events.push(event(5, RunEvent::Started { by: id("worker-1") }));

    assert_eq!(
        RunState::fold(&events),
        Err(FoldError::OutOfOrder {
            found: 5,
            expected_after: 0
        })
    );
}

#[test]
fn folding_the_same_log_twice_gives_the_same_state() {
    let mut events = admitted();
    events.push(event(1, RunEvent::Started { by: id("worker-1") }));
    events.push(event(
        2,
        RunEvent::NodeStarted {
            node: id("normalize"),
        },
    ));

    let first = RunState::fold(&events).expect("the log folds");
    let second = RunState::fold(&events).expect("the log folds");
    assert_eq!(first, second);
    assert_eq!(first.status(), RunStatus::Running);
    assert_eq!(first.next_position(), 3);
}

#[test]
fn an_event_after_a_terminal_one_is_refused() {
    let mut events = admitted();
    events.push(event(1, RunEvent::Started { by: id("worker-1") }));
    events.push(event(
        2,
        RunEvent::Completed {
            outputs: BTreeMap::new(),
        },
    ));
    events.push(event(
        3,
        RunEvent::NodeStarted {
            node: id("normalize"),
        },
    ));

    assert_eq!(
        RunState::fold(&events),
        Err(FoldError::AfterTerminal {
            event: "node_started"
        })
    );
}

#[test]
fn a_node_cannot_finish_without_starting_or_start_twice() {
    let mut finished = admitted();
    finished.push(event(
        1,
        RunEvent::NodeFinished {
            node: id("normalize"),
            outputs: BTreeMap::new(),
            control: std::collections::BTreeMap::new(),
        },
    ));
    assert_eq!(
        RunState::fold(&finished),
        Err(FoldError::FinishedWithoutStarting {
            node: id("normalize")
        })
    );

    let mut twice = admitted();
    for position in 1..=2 {
        twice.push(event(
            position,
            RunEvent::NodeStarted {
                node: id("normalize"),
            },
        ));
    }
    assert_eq!(
        RunState::fold(&twice),
        Err(FoldError::StartedTwice {
            node: id("normalize")
        })
    );
}

#[test]
fn a_run_advances_through_its_graph_in_dependency_order() {
    let definition = pipeline_definition();
    let mut events = admitted();

    // Queued: the only thing to do is start.
    let state = RunState::fold(&events).expect("folds");
    assert_eq!(decide(&definition, &state, at(0)), Decision::Start);

    events.push(event(1, RunEvent::Started { by: id("worker-1") }));
    let state = RunState::fold(&events).expect("folds");
    assert_eq!(
        decide(&definition, &state, at(0)),
        Decision::StartNode {
            node: id("normalize")
        },
        "the node with no predecessors goes first"
    );

    events.push(event(
        2,
        RunEvent::NodeStarted {
            node: id("normalize"),
        },
    ));
    events.push(event(
        3,
        RunEvent::NodeFinished {
            node: id("normalize"),
            outputs: BTreeMap::new(),
            control: std::collections::BTreeMap::new(),
        },
    ));
    let state = RunState::fold(&events).expect("folds");
    assert_eq!(
        decide(&definition, &state, at(0)),
        Decision::StartNode {
            node: id("summarize")
        },
        "its successor becomes ready only once it finished"
    );

    events.push(event(
        4,
        RunEvent::NodeStarted {
            node: id("summarize"),
        },
    ));
    events.push(event(
        5,
        RunEvent::NodeFinished {
            node: id("summarize"),
            outputs: BTreeMap::new(),
            control: std::collections::BTreeMap::new(),
        },
    ));
    let state = RunState::fold(&events).expect("folds");
    assert_eq!(decide(&definition, &state, at(0)), Decision::Complete);
}

#[test]
fn an_idempotent_effect_left_claimed_is_retried() {
    let definition = effect_definition(capsulet_ir::effect::Idempotency::Idempotent);
    let mut events = admitted();
    events.push(event(1, RunEvent::Started { by: id("worker-1") }));
    events.push(event(
        2,
        RunEvent::EffectClaimed {
            node: id("publish"),
            effect: id("open-pull-request"),
            attempt: 0,
            key: None,
        },
    ));
    // The worker died here.

    let state = RunState::fold(&events).expect("folds");
    assert_eq!(
        decide(&definition, &state, at(0)),
        Decision::RetryClaimedEffect {
            node: id("publish"),
            effect: id("open-pull-request"),
            attempt: 1,
        }
    );
}

#[test]
fn a_keyed_effect_is_retried_under_the_same_attempt_and_key() {
    let definition = effect_definition(capsulet_ir::effect::Idempotency::Keyed {
        key_source: "run_id".to_string(),
    });
    let mut events = admitted();
    events.push(event(1, RunEvent::Started { by: id("worker-1") }));
    events.push(event(
        2,
        RunEvent::EffectClaimed {
            node: id("publish"),
            effect: id("open-pull-request"),
            attempt: 3,
            key: Some("run-1".to_string()),
        },
    ));

    let state = RunState::fold(&events).expect("folds");
    assert_eq!(
        decide(&definition, &state, at(0)),
        Decision::RetryClaimedEffect {
            node: id("publish"),
            effect: id("open-pull-request"),
            attempt: 3,
        },
        "the far side deduplicates on the key already used, so the attempt must not change"
    );
}

#[test]
fn a_non_idempotent_effect_left_claimed_stops_the_run() {
    let definition = effect_definition(capsulet_ir::effect::Idempotency::NonIdempotent);
    let mut events = admitted();
    events.push(event(1, RunEvent::Started { by: id("worker-1") }));
    events.push(event(
        2,
        RunEvent::EffectClaimed {
            node: id("publish"),
            effect: id("open-pull-request"),
            attempt: 0,
            key: None,
        },
    ));

    let state = RunState::fold(&events).expect("folds");
    assert_eq!(
        decide(&definition, &state, at(0)),
        Decision::HaltUncertainEffect {
            node: id("publish"),
            effect: id("open-pull-request"),
        },
        "nobody can say whether it happened, and the definition forbids repeating it"
    );
}

#[test]
fn a_finalized_effect_is_never_performed_again() {
    let definition = effect_definition(capsulet_ir::effect::Idempotency::NonIdempotent);
    let mut events = admitted();
    events.push(event(1, RunEvent::Started { by: id("worker-1") }));
    events.push(event(
        2,
        RunEvent::EffectClaimed {
            node: id("publish"),
            effect: id("open-pull-request"),
            attempt: 0,
            key: None,
        },
    ));
    events.push(event(
        3,
        RunEvent::EffectFinalized {
            node: id("publish"),
            effect: id("open-pull-request"),
            attempt: 0,
            receipt: capsulet_ir::Digest::of(b"pull request 41"),
        },
    ));

    let state = RunState::fold(&events).expect("folds");
    assert!(state.outstanding_effects().is_empty());
    assert!(state.effect_completed(&id("publish"), &id("open-pull-request")));

    let decision = decide(&definition, &state, at(0));
    assert_ne!(
        decision,
        Decision::PerformEffect {
            node: id("publish"),
            effect: id("open-pull-request"),
            attempt: 0
        }
    );
}

#[test]
fn finalizing_an_effect_nobody_claimed_is_refused() {
    let mut events = admitted();
    events.push(event(
        1,
        RunEvent::EffectFinalized {
            node: id("publish"),
            effect: id("open-pull-request"),
            attempt: 0,
            receipt: capsulet_ir::Digest::of(b"receipt"),
        },
    ));

    assert_eq!(
        RunState::fold(&events),
        Err(FoldError::FinalizedWithoutClaim {
            node: id("publish"),
            effect: id("open-pull-request"),
        })
    );
}

#[test]
fn a_loop_budget_spent_before_a_crash_stays_spent() {
    let definition = loop_definition();
    let mut events = admitted();
    events.push(event(1, RunEvent::Started { by: id("worker-1") }));

    // Three iterations ran, which is the whole budget. Whether the worker that
    // ran them is the one asking now makes no difference: the count is in the
    // log.
    let mut position = 2;
    for index in 0..3 {
        events.push(event(
            position,
            RunEvent::IterationStarted {
                region: id("repair-loop"),
                index,
                members: std::collections::BTreeSet::new(),
            },
        ));
        events.push(event(
            position + 1,
            RunEvent::IterationFinished {
                region: id("repair-loop"),
                record: Box::new(iteration(index, None, None)),
            },
        ));
        position += 2;
    }

    // And the loop would go round again if it could: the continuation says so.
    // Without that the runtime could not attribute the stop to the budget,
    // because a loop whose condition had gone false did not exhaust anything.
    events.push(event(position, RunEvent::NodeStarted { node: id("check") }));
    events.push(event(
        position + 1,
        RunEvent::NodeFinished {
            node: id("check"),
            outputs: std::collections::BTreeMap::new(),
            control: std::collections::BTreeMap::from([(
                "keep-going".to_string(),
                ControlValue::Bool { value: true },
            )]),
        },
    ));

    let state = RunState::fold(&events).expect("folds");
    assert_eq!(state.loop_progress(&id("repair-loop")).started, 3);
    assert_eq!(
        decide(&definition, &state, at(0)),
        Decision::StopLoop {
            region: id("repair-loop"),
            reason: StopReason::BudgetExhausted {
                budget: BudgetKind::Iterations
            }
        },
        "a restart must not hand the loop its budget back"
    );
}

#[test]
fn two_iterations_of_one_loop_cannot_be_open_at_once() {
    let mut events = admitted();
    events.push(event(1, RunEvent::Started { by: id("worker-1") }));
    for (position, index) in [(2, 0), (3, 1)] {
        events.push(event(
            position,
            RunEvent::IterationStarted {
                region: id("repair-loop"),
                index,
                members: std::collections::BTreeSet::new(),
            },
        ));
    }

    assert_eq!(
        RunState::fold(&events),
        Err(FoldError::IterationAlreadyOpen {
            region: id("repair-loop"),
            index: 1
        }),
        "a fold that accepted this would be reconstructing a run that never happened"
    );
}

#[test]
fn a_due_timer_resumes_and_an_undue_one_does_not() {
    let definition = pipeline_definition();
    let mut events = admitted();
    events.push(event(1, RunEvent::Started { by: id("worker-1") }));
    events.push(event(
        2,
        RunEvent::Suspended {
            wait: Wait::Timer {
                until: RecordedTime(5_000),
            },
        },
    ));

    let state = RunState::fold(&events).expect("folds");
    assert_eq!(state.status(), RunStatus::Waiting);
    assert_eq!(decide(&definition, &state, at(4_999)), Decision::Idle);
    assert_eq!(
        decide(&definition, &state, at(5_000)),
        Decision::ResumeWait {
            wait: Wait::Timer {
                until: RecordedTime(5_000)
            }
        }
    );
}

#[test]
fn an_external_wait_needs_something_outside_the_run() {
    let definition = pipeline_definition();
    let mut events = admitted();
    events.push(event(1, RunEvent::Started { by: id("worker-1") }));
    events.push(event(
        2,
        RunEvent::Suspended {
            wait: Wait::HumanGate {
                node: id("review"),
                obligation: id("release-approval"),
            },
        },
    ));

    let state = RunState::fold(&events).expect("folds");
    assert_eq!(
        decide(&definition, &state, at(i64::MAX)),
        Decision::Idle,
        "no amount of waiting turns a human decision into a due timer"
    );

    events.push(event(
        3,
        RunEvent::Resumed {
            wait: Wait::HumanGate {
                node: id("review"),
                obligation: id("release-approval"),
            },
            by: id("release-manager"),
        },
    ));
    let state = RunState::fold(&events).expect("folds");
    assert_eq!(state.status(), RunStatus::Running);
}

#[test]
fn resuming_without_being_suspended_is_refused() {
    let mut events = admitted();
    events.push(event(
        1,
        RunEvent::Resumed {
            wait: Wait::Event { name: id("push") },
            by: id("webhook"),
        },
    ));

    assert_eq!(RunState::fold(&events), Err(FoldError::ResumedWithoutWait));
}

#[test]
fn a_terminal_run_decides_nothing() {
    let definition = pipeline_definition();
    for terminal in [
        RunEvent::Completed {
            outputs: BTreeMap::new(),
        },
        RunEvent::Cancelled { by: id("operator") },
        RunEvent::Failed {
            reason: RunFailure::BudgetExhausted {
                resource: "tokens".to_string(),
            },
        },
    ] {
        let mut events = admitted();
        events.push(event(1, terminal));
        let state = RunState::fold(&events).expect("folds");
        assert!(state.status().is_terminal());
        assert_eq!(decide(&definition, &state, at(0)), Decision::Idle);
    }
}

#[test]
fn the_epoch_of_the_log_is_the_highest_written() {
    let mut events = admitted();
    let mut started = event(1, RunEvent::Started { by: id("worker-2") });
    started.epoch = Epoch(7);
    events.push(started);

    let state = RunState::fold(&events).expect("folds");
    assert_eq!(state.epoch(), Epoch(7));
}
