//! Once-only semantics, stated as tests.
//!
//! The properties here are the ones that decide whether a crash costs a
//! duplicate pull request. They are all pure, so every crash point can be
//! exercised without a database and without waiting.

mod fixtures;

use capsulet_ir::Digest;
use capsulet_ir::effect::Idempotency;
use capsulet_runtime::effect::{EffectAttempt, EffectContext, EffectError, KeySource};
use capsulet_runtime::{Decision, RunEvent, RunState, check_effect_keys, decide};

use fixtures::{admitted, at, effect_definition, event, id};

/// The single effect the fixture definition declares.
fn declared(definition: &capsulet_ir::Definition) -> &capsulet_ir::effect::Effect {
    definition
        .graph
        .node(&id("publish"))
        .expect("the fixture has the node")
        .effects
        .first()
        .expect("the fixture node declares an effect")
}

#[test]
fn a_keyed_effect_gets_the_key_its_source_asked_for() {
    let definition = effect_definition(Idempotency::Keyed {
        key_source: "run_id".to_string(),
    });
    let attempt = EffectAttempt::claim(
        declared(&definition),
        &EffectContext {
            run: "run-7",
            node: &id("publish"),
            attempt: 0,
        },
    )
    .expect("the key source is one this runtime supplies");

    assert_eq!(attempt.key(), Some("capsulet:run-7:open-pull-request"));

    // The point of keying on the run: attempt two presents the same key, so the
    // far side collapses it into the first.
    let retried = EffectAttempt::claim(
        declared(&definition),
        &EffectContext {
            run: "run-7",
            node: &id("publish"),
            attempt: 5,
        },
    )
    .expect("claim");
    assert_eq!(retried.key(), attempt.key());
}

#[test]
fn an_unkeyed_effect_is_given_no_key_to_pretend_with() {
    for idempotency in [Idempotency::Idempotent, Idempotency::NonIdempotent] {
        let definition = effect_definition(idempotency);
        let attempt = EffectAttempt::claim(
            declared(&definition),
            &EffectContext {
                run: "run-7",
                node: &id("publish"),
                attempt: 0,
            },
        )
        .expect("claim");
        assert_eq!(attempt.key(), None);
    }
}

#[test]
fn a_key_source_this_runtime_cannot_supply_is_refused_before_the_run_starts() {
    let definition = effect_definition(Idempotency::Keyed {
        key_source: "whatever_the_vendor_calls_it".to_string(),
    });

    // Refused at the effect...
    let error = EffectAttempt::claim(
        declared(&definition),
        &EffectContext {
            run: "run-7",
            node: &id("publish"),
            attempt: 0,
        },
    )
    .expect_err("an unknown key source is not something to improvise around");
    assert!(matches!(error, EffectError::UnknownKeySource { .. }));

    // ...and at the definition, which is where it is worth finding out: before
    // any of the graph has run.
    assert!(matches!(
        check_effect_keys(&definition),
        Err(EffectError::UnknownKeySource { .. })
    ));

    let supported = effect_definition(Idempotency::Keyed {
        key_source: "run_id+node_id".to_string(),
    });
    assert!(check_effect_keys(&supported).is_ok());
}

#[test]
fn every_key_source_this_runtime_names_can_actually_be_parsed() {
    // The set is closed, so the names in the documentation and the names the
    // parser accepts are the same set or this fails.
    for (declared_source, expected) in [
        ("run_id", KeySource::Run),
        ("run_id+node_id", KeySource::RunAndNode),
        ("run_id+node_id+attempt", KeySource::RunNodeAndAttempt),
    ] {
        assert_eq!(
            KeySource::parse(&id("open-pull-request"), declared_source).expect("parses"),
            expected
        );
    }
    assert!(KeySource::parse(&id("open-pull-request"), "run").is_err());
}

#[test]
fn a_recovered_attempt_presents_the_key_the_far_side_already_saw() {
    let definition = effect_definition(Idempotency::Keyed {
        key_source: "run_id".to_string(),
    });
    let original = EffectAttempt::claim(
        declared(&definition),
        &EffectContext {
            run: "run-7",
            node: &id("publish"),
            attempt: 3,
        },
    )
    .expect("claim");

    let mut events = admitted();
    events.push(event(1, RunEvent::Started { by: id("worker-1") }));
    events.push(event(2, original.claimed()));
    // The worker died here.

    let state = RunState::fold(&events).expect("folds");
    let outstanding = state
        .outstanding_effects()
        .first()
        .expect("the claim is outstanding");
    let recovered = EffectAttempt::recovered(outstanding, outstanding.attempt);

    assert_eq!(
        recovered, original,
        "a retry that derived a fresh key would be a second effect wearing the first one's name"
    );
    assert_eq!(
        decide(&definition, &state, at(0)),
        Decision::RetryClaimedEffect {
            node: id("publish"),
            effect: id("open-pull-request"),
            attempt: 3,
        }
    );
}

#[test]
fn the_protocol_records_the_claim_before_the_outcome() {
    let definition = effect_definition(Idempotency::Idempotent);
    let attempt = EffectAttempt::claim(
        declared(&definition),
        &EffectContext {
            run: "run-7",
            node: &id("publish"),
            attempt: 0,
        },
    )
    .expect("claim");

    // A log with the finalization and no claim is not a history this runtime
    // can produce, and the fold says so rather than accepting it.
    let mut without_claim = admitted();
    without_claim.push(event(1, attempt.finalized(Digest::of(b"pull request 41"))));
    assert!(RunState::fold(&without_claim).is_err());

    let mut ordered = admitted();
    ordered.push(event(1, attempt.claimed()));
    ordered.push(event(2, attempt.finalized(Digest::of(b"pull request 41"))));
    let state = RunState::fold(&ordered).expect("folds");
    assert!(state.outstanding_effects().is_empty());
    assert!(state.effect_completed(&id("publish"), &id("open-pull-request")));
}

#[test]
fn an_effect_nobody_could_resolve_stops_being_outstanding_without_becoming_done() {
    let definition = effect_definition(Idempotency::NonIdempotent);
    let attempt = EffectAttempt::claim(
        declared(&definition),
        &EffectContext {
            run: "run-7",
            node: &id("publish"),
            attempt: 0,
        },
    )
    .expect("claim");

    let mut events = admitted();
    events.push(event(1, RunEvent::Started { by: id("worker-1") }));
    events.push(event(2, attempt.claimed()));
    let stalled = RunState::fold(&events).expect("folds");
    assert_eq!(
        decide(&definition, &stalled, at(0)),
        Decision::HaltUncertainEffect {
            node: id("publish"),
            effect: id("open-pull-request"),
        }
    );

    events.push(event(3, attempt.uncertain()));
    let state = RunState::fold(&events).expect("folds");
    assert!(
        state.outstanding_effects().is_empty(),
        "the doubt is recorded, so recovery does not keep re-deciding it"
    );
    assert!(
        !state.effect_completed(&id("publish"), &id("open-pull-request")),
        "recording the doubt must not be mistaken for recording success"
    );
    assert_eq!(
        state.uncertain_effects(),
        [(id("publish"), id("open-pull-request"))],
        "the run remembers which effect it could not account for"
    );
    assert_eq!(
        decide(&definition, &state, at(0)),
        Decision::Fail {
            reason: capsulet_runtime::RunFailure::EffectUncertain {
                node: id("publish"),
                effect: id("open-pull-request"),
            }
        },
        "a run cannot finish on top of a step whose outcome nobody knows"
    );
}

#[test]
fn an_effect_the_far_side_refused_outright_is_abandoned_rather_than_doubted() {
    let definition = effect_definition(Idempotency::NonIdempotent);
    let attempt = EffectAttempt::claim(
        declared(&definition),
        &EffectContext {
            run: "run-7",
            node: &id("publish"),
            attempt: 0,
        },
    )
    .expect("claim");

    let mut events = admitted();
    events.push(event(1, RunEvent::Started { by: id("worker-1") }));
    events.push(event(2, attempt.claimed()));
    events.push(event(
        3,
        attempt.abandoned("the repository is archived".to_string()),
    ));

    let state = RunState::fold(&events).expect("folds");
    assert!(state.outstanding_effects().is_empty());
    assert!(!state.effect_completed(&id("publish"), &id("open-pull-request")));
    assert!(
        state.uncertain_effects().is_empty(),
        "knowing it did not happen is not the same as not knowing, and only the second stops a run"
    );
    assert!(!matches!(
        decide(&definition, &state, at(0)),
        Decision::Fail {
            reason: capsulet_runtime::RunFailure::EffectUncertain { .. }
        }
    ));
}
