//! Waiting, and what may end it.
//!
//! The refusals are the substance here. A wait that resumes on the wrong
//! signal produces a run whose remaining history nobody can explain, so every
//! near-miss below is a refusal with a reason rather than a resumption.

mod fixtures;

use std::collections::BTreeSet;

use capsulet_ir::correctness::evidence::RecordedTime;
use capsulet_runtime::wait::{Signal, SignalKind, WaitError, resume, suspend, timer_is_due};
use capsulet_runtime::{Decision, RunEvent, RunState, Wait, decide};

use fixtures::{admitted, at, event, id, pipeline_definition};

/// Authorities a person holds.
fn holding(authorities: &[&str]) -> BTreeSet<capsulet_ir::Identifier> {
    authorities.iter().map(|name| id(name)).collect()
}

#[test]
fn a_timer_that_is_not_due_resumes_nothing() {
    let wait = Wait::Timer {
        until: RecordedTime(5_000),
    };
    assert!(!timer_is_due(&wait, at(4_999)));
    assert!(timer_is_due(&wait, at(5_000)));

    let error = resume(Some(&wait), &Signal::timer(id("scheduler")), at(4_999))
        .expect_err("a scheduler that fires early does not make the timer due");
    assert!(matches!(error, WaitError::NotDue { .. }));

    let resumed = resume(Some(&wait), &Signal::timer(id("scheduler")), at(5_000))
        .expect("a due timer resumes");
    assert_eq!(
        resumed,
        RunEvent::Resumed {
            wait,
            by: id("scheduler")
        }
    );
}

#[test]
fn a_due_timer_resumes_exactly_once_because_the_fold_says_so() {
    let definition = pipeline_definition();
    let wait = Wait::Timer {
        until: RecordedTime(5_000),
    };

    let mut events = admitted();
    events.push(event(1, RunEvent::Started { by: id("worker-1") }));
    events.push(event(2, suspend(wait.clone())));

    let waiting = RunState::fold(&events).expect("folds");
    assert_eq!(
        decide(&definition, &waiting, at(5_000)),
        Decision::ResumeWait { wait: wait.clone() }
    );

    events.push(event(
        3,
        resume(
            waiting.waiting_on(),
            &Signal::timer(id("scheduler")),
            at(5_000),
        )
        .expect("resumes"),
    ));
    let resumed = RunState::fold(&events).expect("folds");

    // The wait is gone from the state, so a second delivery of the same timer
    // has nothing to resume.
    assert_eq!(resumed.waiting_on(), None);
    let error = resume(
        resumed.waiting_on(),
        &Signal::timer(id("scheduler")),
        at(5_000),
    )
    .expect_err("the run is running again");
    assert_eq!(error, WaitError::NotWaiting);
}

#[test]
fn an_external_event_resumes_only_the_wait_that_names_it() {
    let wait = Wait::Event {
        name: id("review-posted"),
    };

    let wrong = resume(
        Some(&wait),
        &Signal::event(id("build-finished"), id("webhook")),
        at(0),
    )
    .expect_err("a different event is not this event");
    assert!(matches!(wrong, WaitError::Mismatch { .. }));

    let right = resume(
        Some(&wait),
        &Signal::event(id("review-posted"), id("webhook")),
        at(0),
    )
    .expect("the named event resumes it");
    assert_eq!(
        right,
        RunEvent::Resumed {
            wait,
            by: id("webhook")
        }
    );
}

#[test]
fn a_timer_signal_does_not_open_an_event_wait() {
    // Different kinds entirely. Worth its own case, because the tempting
    // implementation is "anything that arrives resumes whatever is waiting".
    let wait = Wait::Event {
        name: id("review-posted"),
    };
    let error = resume(Some(&wait), &Signal::timer(id("scheduler")), at(i64::MAX))
        .expect_err("no amount of waiting turns an event into a timer");
    assert!(matches!(error, WaitError::Mismatch { .. }));
}

#[test]
fn a_human_gate_opens_only_for_somebody_who_holds_the_authority() {
    let wait = Wait::HumanGate {
        node: id("release"),
        obligation: id("release-manager"),
    };

    let unauthorised = resume(
        Some(&wait),
        &Signal::decision(
            id("release-manager"),
            id("keen-engineer"),
            holding(&["reviewer"]),
        ),
        at(0),
    )
    .expect_err("wanting to approve is not being allowed to");
    assert_eq!(
        unauthorised,
        WaitError::Unauthorised {
            obligation: id("release-manager"),
            by: id("keen-engineer"),
        },
        "a gate anyone could open is not a gate"
    );

    let other_obligation = resume(
        Some(&wait),
        &Signal::decision(
            id("security-review"),
            id("release-captain"),
            holding(&["release-manager", "security-review"]),
        ),
        at(0),
    )
    .expect_err("holding the authority is not the same as answering this gate");
    assert!(matches!(other_obligation, WaitError::Mismatch { .. }));

    let opened = resume(
        Some(&wait),
        &Signal::decision(
            id("release-manager"),
            id("release-captain"),
            holding(&["release-manager"]),
        ),
        at(0),
    )
    .expect("the authorised decision opens it");
    assert_eq!(
        opened,
        RunEvent::Resumed {
            wait,
            by: id("release-captain")
        }
    );
}

#[test]
fn a_gate_never_becomes_due_by_waiting() {
    let definition = pipeline_definition();
    let mut events = admitted();
    events.push(event(1, RunEvent::Started { by: id("worker-1") }));
    events.push(event(
        2,
        suspend(Wait::HumanGate {
            node: id("release"),
            obligation: id("release-manager"),
        }),
    ));

    let state = RunState::fold(&events).expect("folds");
    assert_eq!(
        decide(&definition, &state, at(i64::MAX)),
        Decision::Idle,
        "no amount of time turns a human decision into a due timer"
    );
}

#[test]
fn suspension_and_resumption_are_both_events_and_nothing_else() {
    // The point of the whole module: between these two events there is no
    // worker, no timer thread, and nothing in memory that a restart could lose.
    let wait = Wait::Event {
        name: id("review-posted"),
    };
    assert_eq!(
        suspend(wait.clone()),
        RunEvent::Suspended { wait: wait.clone() }
    );

    let mut events = admitted();
    events.push(event(1, RunEvent::Started { by: id("worker-1") }));
    events.push(event(2, suspend(wait.clone())));

    // A completely fresh process folds the same log and knows exactly what the
    // run is waiting for.
    let recovered = RunState::fold(&events).expect("folds");
    assert_eq!(recovered.waiting_on(), Some(&wait));
    assert_eq!(
        recovered.status(),
        capsulet_runtime::RunStatus::Waiting,
        "the status is a projection of the log, so recovery reads it the same way"
    );

    let signal = Signal::event(id("review-posted"), id("webhook"));
    assert_eq!(
        signal.kind,
        SignalKind::Event {
            name: id("review-posted")
        }
    );
    events.push(event(
        3,
        resume(recovered.waiting_on(), &signal, at(0)).expect("resumes"),
    ));
    assert_eq!(
        RunState::fold(&events).expect("folds").status(),
        capsulet_runtime::RunStatus::Running
    );
}
