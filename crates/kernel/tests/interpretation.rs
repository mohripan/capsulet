//! The step no kernel can take, and what it has to leave behind.
//!
//! `Interpret` concludes whatever it is asked to. That is deliberate: whether a
//! passage *means* what a proposition says is not decidable over natural
//! language, and a kernel that pretended otherwise would be the failure this
//! design exists to avoid. What makes it safe to have at all is that it
//! discharges nothing and records a residual, so the verdict is `conditional`
//! and a person is told exactly what they are being asked to accept.
//!
//! These tests pin the "exactly what" part. A residual nobody can act on is not
//! a smaller problem than an unsound rule — it is the same problem, filed.

use capsulet_core::{
    Authority, Evidence, EvidenceId, EvidenceSpan, MemoryScope, Source, SourceContent, SourceId,
};
use capsulet_kernel::{
    Certificate, MAX_DERIVATION_DEPTH, Proposal, Proposition, Rule, Snapshot, Verdict, check,
};

const DOC: &str = "Acme renewed the Contoso contract on 2026-03-01.";

fn source_id() -> SourceId {
    SourceId::new("src_1").expect("source id")
}

fn snapshot(authority: Authority) -> Snapshot {
    let scope = MemoryScope::new("acme", "prod").expect("scope");
    let content = SourceContent::new(source_id(), DOC).expect("content");
    let evidence = Evidence::new(
        EvidenceId::new("ev_1").expect("evidence id"),
        scope.clone(),
        source_id(),
        "para-1",
        DOC,
        "2026-03-02T00:00:00Z",
    )
    .expect("evidence")
    .with_span(EvidenceSpan::new(0, DOC.len(), content.content_hash()).expect("span"));

    Snapshot::new()
        .with_source(
            Source::new(
                source_id(),
                scope,
                "document",
                None,
                "Renewal memo",
                authority,
            )
            .expect("source"),
        )
        .with_source_content(content)
        .with_evidence(evidence)
}

fn citation() -> Rule {
    Rule::Cite {
        evidence_id: "ev_1".to_string(),
        proposition: Proposition::new("Acme", "says", "Contoso"),
    }
}

/// The reading a proposer wants: from what the memo says, to what holds.
fn reading(rationale: &str) -> Proposal {
    let conclusion = Proposition::new("Acme", "has-an-active-contract-with", "Contoso");
    Proposal {
        goal: conclusion.clone(),
        derivation: Rule::Interpret {
            premise: Box::new(citation()),
            proposition: conclusion,
            rationale: rationale.to_string(),
        },
    }
}

fn decide(proposal: &Proposal, authority: Authority) -> Certificate {
    check(proposal, &snapshot(authority))
}

fn codes(certificate: &Certificate) -> Vec<&str> {
    certificate
        .errors
        .iter()
        .map(|error| error.code.as_str())
        .collect()
}

#[test]
fn a_reading_with_a_rationale_is_conditional_and_leaves_a_residual() {
    let certificate = decide(
        &reading("the memo records a renewal, so the contract is active"),
        Authority::High,
    );

    assert_eq!(certificate.verdict, Verdict::Conditional);
    assert_eq!(certificate.residuals.len(), 1);
}

#[test]
fn a_reading_without_a_rationale_is_refused() {
    // A residual is a question put to a person. One that does not say what was
    // read, or why, is not a question — it is a blank the reviewer is asked to
    // sign. The rule that discharges nothing has to at least say what it did.
    let certificate = decide(&reading("   "), Authority::High);

    assert_eq!(certificate.verdict, Verdict::Rejected);
    assert!(
        codes(&certificate).contains(&"interpretation_without_a_rationale"),
        "expected the missing rationale to be the reason, got {:?}",
        codes(&certificate)
    );
}

#[test]
fn the_residual_names_the_premise_the_conclusion_and_the_evidence() {
    let certificate = decide(
        &reading("the memo records a renewal, so the contract is active"),
        Authority::High,
    );
    let residual = &certificate.residuals[0];

    assert!(
        residual.from.contains("Contoso"),
        "the residual should say what was read from: {:?}",
        residual.from
    );
    assert_eq!(
        residual.to,
        Proposition::new("Acme", "has-an-active-contract-with", "Contoso"),
        "and what was read out of it"
    );
    assert_eq!(
        residual.evidence_ids,
        vec!["ev_1".to_string()],
        "and where to go to check"
    );
    assert!(!residual.rationale.trim().is_empty());
}

#[test]
fn a_reading_never_reaches_accepted_however_authoritative_the_source() {
    // `Trust` clears an authority floor and reaches `accepted`. `Interpret`
    // reaches the same shape of conclusion and cannot: authority says a source
    // is worth believing, not that a particular reading of it is right.
    for authority in [Authority::Low, Authority::Medium, Authority::High] {
        let certificate = decide(&reading("a reading"), authority);
        assert_eq!(
            certificate.verdict,
            Verdict::Conditional,
            "authority {authority:?} should not discharge a reading"
        );
    }
}

#[test]
fn a_chain_of_readings_is_bounded() {
    // Each reading adds a residual, so an unbounded chain is an unbounded pile
    // of questions nobody will ever work through. The derivation bound caps it.
    let conclusion = Proposition::new("Acme", "has-an-active-contract-with", "Contoso");
    let mut derivation = citation();
    for _ in 0..=MAX_DERIVATION_DEPTH {
        derivation = Rule::Interpret {
            premise: Box::new(derivation),
            proposition: conclusion.clone(),
            rationale: "another reading".to_string(),
        };
    }

    let certificate = decide(
        &Proposal {
            goal: conclusion,
            derivation,
        },
        Authority::High,
    );

    assert_eq!(certificate.verdict, Verdict::Rejected);
    assert!(codes(&certificate).contains(&"derivation_too_deep"));
}
