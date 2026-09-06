//! A certificate that says what the log says, and nothing else.
//!
//! The property that matters: a run killed and resumed and a run that went
//! straight through produce the same certificate inputs. If they did not, a
//! certificate would be a record of how lucky the infrastructure was rather
//! than of what the run did.

mod fixtures;

use std::collections::BTreeMap;

use capsulet_ir::Digest;
use capsulet_ir::correctness::obligation::{DischargeState, ObligationStatement, RepairOwner};
use capsulet_ir::loop_region::{BudgetKind, StopReason};
use capsulet_ir::{Obligation, admit};
use capsulet_runtime::certify::{
    CertifyError, check_obligations, evidence_of, loop_outcomes, stop_reasons,
};
use capsulet_runtime::{RunEvent, RunState};

use fixtures::{
    LoopShape, admitted, event, id, iteration, loop_definition_with, pipeline_definition,
};

/// The log of a pipeline run that went straight through.
fn uninterrupted() -> Vec<capsulet_runtime::RecordedEvent> {
    let mut events = admitted();
    events.push(event(1, RunEvent::Started { by: id("worker-1") }));
    for (position, node) in [(2, "normalize"), (4, "summarize")] {
        events.push(event(position, RunEvent::NodeStarted { node: id(node) }));
        events.push(event(
            position + 1,
            RunEvent::NodeFinished {
                node: id(node),
                outputs: BTreeMap::from([("out".to_string(), Digest::of(node.as_bytes()))]),
                control: std::collections::BTreeMap::new(),
            },
        ));
    }
    events.push(event(
        6,
        RunEvent::Completed {
            outputs: BTreeMap::new(),
        },
    ));
    events
}

/// The same run, with a worker dying between the two nodes and another taking
/// over under a new epoch.
fn interrupted() -> Vec<capsulet_runtime::RecordedEvent> {
    let mut events = uninterrupted();
    for recorded in events.iter_mut().skip(4) {
        recorded.epoch = capsulet_runtime::Epoch(2);
    }
    events
}

#[test]
fn a_resumed_run_and_an_uninterrupted_one_certify_the_same() {
    let definition = pipeline_definition();
    let admission = admit(&definition).expect("the fixture is admitted");

    let straight = uninterrupted();
    let recovered = interrupted();

    let first = evidence_of(
        &definition,
        admission.clone(),
        &id("run-7"),
        &RunState::fold(&straight).expect("folds"),
        &straight,
        vec![],
    );
    let second = evidence_of(
        &definition,
        admission,
        &id("run-7"),
        &RunState::fold(&recovered).expect("folds"),
        &recovered,
        vec![],
    );

    assert_eq!(
        first, second,
        "which worker wrote which event is not a fact about what the run did"
    );
    assert_eq!(first.subject.run, Some(id("run-7")));
    assert_eq!(
        first.subject.outputs.len(),
        2,
        "the outputs come from the fold, not from what a worker remembers"
    );
}

#[test]
fn the_certificate_names_every_loop_stop_reason_the_log_recorded() {
    let definition = loop_definition_with(LoopShape {
        max_iterations: 2,
        ..LoopShape::plain()
    });

    let mut events = admitted();
    events.push(event(1, RunEvent::Started { by: id("worker-1") }));
    events.push(event(
        2,
        RunEvent::IterationStarted {
            region: id("repair-loop"),
            index: 0,
            members: std::collections::BTreeSet::new(),
        },
    ));
    events.push(event(
        3,
        RunEvent::IterationFinished {
            region: id("repair-loop"),
            record: Box::new(iteration(0, Some(4), Some(true))),
        },
    ));
    events.push(event(
        4,
        RunEvent::IterationStarted {
            region: id("repair-loop"),
            index: 1,
            members: std::collections::BTreeSet::new(),
        },
    ));
    events.push(event(
        5,
        RunEvent::IterationFinished {
            region: id("repair-loop"),
            record: Box::new(iteration(1, Some(2), Some(true))),
        },
    ));
    let stopped = StopReason::BudgetExhausted {
        budget: BudgetKind::Iterations,
    };
    events.push(event(
        6,
        RunEvent::LoopStopped {
            region: id("repair-loop"),
            reason: stopped.clone(),
        },
    ));

    let state = RunState::fold(&events).expect("folds");
    let outcomes = loop_outcomes(&events, &state);
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].region, id("repair-loop"));
    assert_eq!(outcomes[0].iterations.len(), 2);
    assert_eq!(outcomes[0].stopped, stopped);
    assert!(
        !outcomes[0].completed(),
        "exhausting a budget is a stop, and rendering it as a completion is the mistake this \
         whole type exists to prevent"
    );

    assert_eq!(
        stop_reasons(&events),
        vec![(id("repair-loop"), stopped)],
        "the reasons come out of the log in the order the run recorded them"
    );

    let evidence = evidence_of(
        &definition,
        admit(&definition).expect("admitted"),
        &id("run-7"),
        &state,
        &events,
        vec![],
    );
    assert_eq!(evidence.loops, outcomes);
}

#[test]
fn a_loop_that_is_still_running_is_not_reported_as_having_stopped() {
    let definition = loop_definition_with(LoopShape::plain());
    let mut events = admitted();
    events.push(event(1, RunEvent::Started { by: id("worker-1") }));
    events.push(event(
        2,
        RunEvent::IterationStarted {
            region: id("repair-loop"),
            index: 0,
            members: std::collections::BTreeSet::new(),
        },
    ));

    let state = RunState::fold(&events).expect("folds");
    assert!(
        loop_outcomes(&events, &state).is_empty(),
        "every stop reason available would be a false statement about a loop that has not stopped"
    );

    let evidence = evidence_of(
        &definition,
        admit(&definition).expect("admitted"),
        &id("run-7"),
        &state,
        &events,
        vec![],
    );
    assert!(evidence.loops.is_empty());
}

#[test]
fn an_obligation_resting_on_evidence_the_run_never_carried_is_refused() {
    let missing = Digest::of(b"a log nobody kept");
    let obligation = Obligation {
        statement: ObligationStatement {
            id: id("patch-compiles"),
            statement: "the patch compiles".to_string(),
            owner: RepairOwner::Verifier,
        },
        contract: id("patch-compiles"),
        state: DischargeState::Discharged {
            by: id("cargo-test"),
            evidence: vec![missing],
        },
    };

    let error = check_obligations(std::slice::from_ref(&obligation), &[], &[])
        .expect_err("the evidence it rests on is not there");
    assert_eq!(
        error,
        CertifyError::UnknownEvidence {
            obligation: id("patch-compiles"),
            digest: missing.to_string(),
        },
        "a discharge that points at nothing is worse than no discharge at all"
    );

    // An obligation that rests on nothing is fine; an assumption is honest
    // about being an assumption.
    let assumed = Obligation {
        state: DischargeState::Assumed {
            rationale: "the reviewer accepted it".to_string(),
        },
        ..obligation
    };
    assert!(check_obligations(&[assumed], &[], &[]).is_ok());
}
