//! The kernel decides every derivation it is handed, including hostile ones.
//!
//! `check` walks nested rules, and the proposer chooses how deep to nest them.
//! Unbounded, that walk — and the encoding of the same value for the replay
//! digest — ended the process rather than returning a verdict: a stack
//! overflow, not a panic a caller could catch.
//!
//! These tests pin three things: where the bound sits, that it is the kernel's
//! own rather than the transport's, and that exceeding it is an ordinary
//! rejection.

use capsulet_kernel::ir::{Proposal, Proposition, Rule};
use capsulet_kernel::snapshot::Snapshot;
use capsulet_kernel::{MAX_DERIVATION_DEPTH, Verdict, check};

/// A chain of `depth` rules, innermost first.
fn nested(depth: u32) -> Rule {
    let mut rule = Rule::Attest {
        claim_id: "nonexistent".to_string(),
    };
    // The chain already has one rule in it, so add the rest.
    for _ in 1..depth {
        rule = Rule::Trust {
            premise: Box::new(rule),
            min_authority: "primary".to_string(),
        };
    }
    rule
}

fn decide(derivation: Rule) -> capsulet_kernel::Certificate {
    check(
        &Proposal {
            goal: Proposition::new("s", "p", "o"),
            derivation,
        },
        &Snapshot::new(),
    )
}

fn codes(certificate: &capsulet_kernel::Certificate) -> Vec<&str> {
    certificate
        .errors
        .iter()
        .map(|error| error.code.as_str())
        .collect()
}

#[test]
fn a_derivation_past_the_bound_is_rejected_rather_than_ending_the_process() {
    let certificate = decide(nested(MAX_DERIVATION_DEPTH + 1));

    assert_eq!(certificate.verdict, Verdict::Rejected);
    assert!(
        codes(&certificate).contains(&"derivation_too_deep"),
        "expected the depth bound to be the reason, got {:?}",
        codes(&certificate)
    );
}

#[test]
fn a_derivation_at_the_bound_still_decides() {
    let certificate = decide(nested(MAX_DERIVATION_DEPTH));

    // It is rejected — the innermost claim is not in the snapshot — but the
    // bound is not what rejected it. A bound that fires one rule early is a
    // different bound from the one documented.
    assert_eq!(certificate.verdict, Verdict::Rejected);
    assert!(
        !codes(&certificate).contains(&"derivation_too_deep"),
        "the bound fired at the documented depth, so it is off by one: {:?}",
        codes(&certificate)
    );
    assert!(codes(&certificate).contains(&"dangling_claim"));
}

#[test]
fn a_derivation_far_past_the_bound_is_still_only_a_rejection() {
    // Ten thousand rules is well past the depth at which walking the chain
    // *and* encoding it for the replay digest each used to end the process.
    // Nothing about the answer changes: it is a rejection, like any other.
    let certificate = decide(nested(10_000));

    assert_eq!(certificate.verdict, Verdict::Rejected);
    assert_eq!(codes(&certificate), vec!["derivation_too_deep"]);
}

#[test]
fn a_refused_derivation_is_not_digested_as_if_it_had_been_read() {
    // Two proposals with the same goal and different over-deep derivations get
    // the same digest, because neither derivation was encoded. That is the
    // honest outcome: the digest says which goal was refused, and the error
    // says the derivation was never read.
    let goal = Proposition::new("s", "p", "o");
    let one = check(
        &Proposal {
            goal: goal.clone(),
            derivation: nested(MAX_DERIVATION_DEPTH + 1),
        },
        &Snapshot::new(),
    );
    let other = check(
        &Proposal {
            goal,
            derivation: nested(MAX_DERIVATION_DEPTH + 9),
        },
        &Snapshot::new(),
    );

    assert_eq!(one.replay_digest, other.replay_digest);
}

#[test]
fn the_certificate_records_the_bound_it_was_decided_under() {
    // A later build with a different bound may reach a different verdict on the
    // same proposal. A reader should not have to know which kernel version had
    // which constant to see that.
    let certificate = decide(nested(2));

    assert_eq!(
        certificate.derivation_depth_limit,
        Some(MAX_DERIVATION_DEPTH)
    );
}

#[test]
fn the_kernel_does_not_rely_on_the_transport_to_bound_depth() {
    // serde_json refuses deep nesting on its own — its recursion limit is 128,
    // and each `Trust` costs two levels of JSON, so it gives up well before the
    // stack does. That is a property of the format the caller happened to
    // choose, not a decision this kernel made: a format without such a limit,
    // or a caller building the value in process, reaches `check` directly.
    let mut document = r#"{"Attest":{"claim_id":"leaf"}}"#.to_string();
    for _ in 0..MAX_DERIVATION_DEPTH {
        document = format!(r#"{{"Trust":{{"premise":{document},"min_authority":"primary"}}}}"#);
    }
    assert!(
        serde_json::from_str::<Rule>(&document).is_err(),
        "serde_json is expected to refuse this; if it stops doing so, the kernel's own bound is \
         the only thing left and this test should still pass"
    );

    // The kernel reaches the same answer without the parser's help.
    let certificate = decide(nested(MAX_DERIVATION_DEPTH + 1));
    assert!(codes(&certificate).contains(&"derivation_too_deep"));
}
