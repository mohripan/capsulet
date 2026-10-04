//! Tripwires: what the kernel decides today that it should not.
//!
//! Every test here asserts the current, wrong behaviour, so that a gap nothing
//! fails on still has a shape CI holds. Each goes red exactly when the task that
//! closes it lands — the plan is
//! `docs/superpowers/plans/2026-10-04-citation-grounding.md` — and then moves,
//! inverted, to the test file where the behaviour belongs, taking its claim in
//! the registry with it.
//!
//! Every proposal is shaped the way `capsulet-proposer` shapes an answer the
//! model says it only quoted: `Trust { Cite(goal), min_authority: "low" }`.

use capsulet_core::{
    Authority, Evidence, EvidenceId, EvidenceSpan, MemoryScope, Source, SourceContent, SourceId,
};
use capsulet_kernel::{Proposal, Proposition, Rule, Snapshot, Verdict, check};

fn source_id() -> SourceId {
    SourceId::new("src_1").expect("source id")
}

/// A snapshot in which `document` is the stored text and the whole of it is
/// cited verbatim.
fn snapshot_over(document: &str, authority: Authority) -> Snapshot {
    let scope = MemoryScope::new("acme", "prod").expect("scope");
    let content = SourceContent::new(source_id(), document).expect("content");
    let evidence = Evidence::new(
        EvidenceId::new("ev_1").expect("evidence id"),
        scope.clone(),
        source_id(),
        "para-1",
        document,
        "2026-03-02T00:00:00Z",
    )
    .expect("evidence")
    .with_span(EvidenceSpan::new(0, document.len(), content.content_hash()).expect("span"));

    Snapshot::new()
        .with_source(
            Source::new(source_id(), scope, "document", None, "Memo", authority).expect("source"),
        )
        .with_source_content(content)
        .with_evidence(evidence)
}

fn cite(proposition: Proposition) -> Rule {
    Rule::Cite {
        evidence_id: "ev_1".to_string(),
        proposition,
    }
}

/// The derivation the proposer builds for an answer the model calls a quote.
fn quoted(goal: Proposition, cited: Proposition, min_authority: &str) -> Proposal {
    Proposal {
        goal,
        derivation: Rule::Trust {
            premise: Box::new(cite(cited)),
            min_authority: min_authority.to_string(),
        },
    }
}

/// Decides `subject predicate object` as a quotation of `document`, from a
/// high-authority source.
fn decide(document: &str, subject: &str, predicate: &str, object: &str) -> Verdict {
    let proposition = Proposition::new(subject, predicate, object);
    let proposal = quoted(proposition.clone(), proposition, "low");
    check(&proposal, &snapshot_over(document, Authority::High)).verdict
}

// --- The predicate is never checked (CAP-CORRECTNESS-009) -------------------

#[test]
fn a_negated_statement_is_accepted_as_the_statement() {
    // A reader says the source states the opposite. Both terms occur, and the
    // relation between them is not looked at.
    assert_eq!(
        decide(
            "Fabrikam did not acquire Contoso.",
            "Fabrikam",
            "acquired",
            "Contoso"
        ),
        Verdict::Accepted,
        "closed by Task 5 of the citation grounding plan; move this to grounding.rs inverted"
    );
}

#[test]
fn reversed_roles_are_accepted_as_the_statement() {
    assert_eq!(
        decide(
            "Contoso acquired Fabrikam in 2024.",
            "Fabrikam",
            "acquired",
            "Contoso"
        ),
        Verdict::Accepted,
        "closed by Task 5 of the citation grounding plan; move this to grounding.rs inverted"
    );
}

#[test]
fn a_different_relation_between_the_same_terms_is_accepted() {
    assert_eq!(
        decide(
            "Contoso sued Fabrikam over a patent.",
            "Contoso",
            "was_acquired_by",
            "Fabrikam"
        ),
        Verdict::Accepted,
        "closed by Task 5 of the citation grounding plan; move this to grounding.rs inverted"
    );
}

#[test]
fn a_hedged_statement_is_accepted_as_the_statement() {
    assert_eq!(
        decide(
            "Fabrikam may acquire Contoso next year.",
            "Fabrikam",
            "acquired",
            "Contoso"
        ),
        Verdict::Accepted,
        "closed by Task 5 of the citation grounding plan; move this to grounding.rs inverted"
    );
}

// --- Terms are substrings of lowercased text (CAP-CORRECTNESS-004) ----------

#[test]
fn a_term_inside_a_longer_word_is_found() {
    assert_eq!(
        decide(
            "The CEO announced the annual results.",
            "CEO",
            "is_named",
            "Ann"
        ),
        Verdict::Accepted,
        "closed by Task 4 of the citation grounding plan; move this to grounding.rs inverted"
    );
}

#[test]
fn a_number_inside_a_longer_number_is_found() {
    assert_eq!(
        decide(
            "The notice period is 300 days.",
            "notice period",
            "in_days",
            "30"
        ),
        Verdict::Accepted,
        "closed by Task 4 of the citation grounding plan; move this to grounding.rs inverted"
    );
}

#[test]
fn a_number_written_another_way_is_not_found() {
    assert_eq!(
        decide(
            "Revenue rose to $1.2 million in 2025.",
            "Revenue",
            "rose_to",
            "$1,200,000"
        ),
        Verdict::Rejected,
        "closed by Task 7 of the citation grounding plan; move this to grounding.rs inverted"
    );
}

#[test]
fn a_date_written_another_way_is_not_found() {
    assert_eq!(
        decide(
            "The contract was signed on March 1, 2026.",
            "contract",
            "signed_on",
            "2026-03-01"
        ),
        Verdict::Rejected,
        "closed by Task 7 of the citation grounding plan; move this to grounding.rs inverted"
    );
}

#[test]
fn another_name_for_the_entity_is_not_found() {
    assert_eq!(
        decide(
            "IBM acquired Red Hat in 2019.",
            "International Business Machines",
            "acquired",
            "Red Hat"
        ),
        Verdict::Rejected,
        "closed by Task 8 of the citation grounding plan; move this to grounding.rs inverted"
    );
}

#[test]
fn a_typographic_apostrophe_is_not_an_apostrophe() {
    assert_eq!(
        decide(
            "Acme\u{2019}s renewal was approved.",
            "Acme's renewal",
            "status",
            "approved"
        ),
        Verdict::Rejected,
        "closed by Task 4 of the citation grounding plan; move this to grounding.rs inverted"
    );
}

#[test]
fn sharp_s_does_not_match_double_s() {
    // Lowercasing leaves `ß` as `ß` and turns `SS` into `ss`. Case folding, the
    // operation built for matching, turns both into `ss`.
    assert_eq!(
        decide(
            "Acme is located at Hauptstra\u{00DF}e 5.",
            "Acme",
            "located_at",
            "HAUPTSTRASSE 5"
        ),
        Verdict::Rejected,
        "closed by Task 4 of the citation grounding plan; move this to grounding.rs inverted"
    );
}

// --- The proposer chooses the floor (CAP-CORRECTNESS-011) -------------------

#[test]
fn the_authority_floor_is_whatever_the_proposal_asks_for() {
    // The same source, the same citation: refused when the proposal asks for a
    // high floor and accepted when it asks for a low one. The shipped proposer
    // always asks for `low`.
    let document = "Acme renewed the Contoso contract.";
    let proposition = Proposition::new("Acme", "renewed", "Contoso");
    let decide_at = |floor: &str| {
        check(
            &quoted(proposition.clone(), proposition.clone(), floor),
            &snapshot_over(document, Authority::Low),
        )
        .verdict
    };

    assert_eq!(decide_at("high"), Verdict::Rejected);
    assert_eq!(
        decide_at("low"),
        Verdict::Accepted,
        "closed by Task 3 of the citation grounding plan; move this to tests.rs inverted"
    );
}
