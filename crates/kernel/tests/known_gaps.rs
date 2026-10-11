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
use capsulet_kernel::{
    Phrasebook, Phrasing, Proposal, Proposition, Rule, Snapshot, Verdict, check,
};

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
        schema_version: Proposal::current_schema_version(),
        goal,
        derivation: Rule::Trust {
            premise: Box::new(cite(cited)),
            min_authority: min_authority.to_string(),
        },
    }
}

/// Whether citing `document` for `subject predicate object` fails because a
/// term was not found in it.
///
/// Asked of the term step rather than of the verdict. With no phrasing declared
/// the verdict would stay `rejected` for another reason — the relation is not
/// stated — after the term starts being found, and a tripwire on the verdict
/// would then never go red.
fn term_not_found(document: &str, subject: &str, predicate: &str, object: &str) -> bool {
    let proposition = Proposition::new(subject, predicate, object);
    let proposal = quoted(proposition.clone(), proposition, "low");
    check(&proposal, &snapshot_over(document, Authority::High))
        .errors
        .iter()
        .any(|error| error.code == "term_not_in_span")
}

// --- A value written another way is not found (CAP-CORRECTNESS-004) ----------

#[test]
fn a_number_written_another_way_is_not_found() {
    assert!(
        term_not_found(
            "Revenue rose to $1.2 million in 2025.",
            "Revenue",
            "rose_to",
            "$1,200,000"
        ),
        "closed by Task 7 of the citation grounding plan; move this to grounding.rs inverted"
    );
}

#[test]
fn a_date_written_another_way_is_not_found() {
    assert!(
        term_not_found(
            "The contract was signed on March 1, 2026.",
            "contract",
            "signed_on",
            "2026-03-01"
        ),
        "closed by Task 7 of the citation grounding plan; move this to grounding.rs inverted"
    );
}

#[test]
fn another_name_for_the_entity_is_not_found() {
    assert!(
        term_not_found(
            "IBM acquired Red Hat in 2019.",
            "International Business Machines",
            "acquired",
            "Red Hat"
        ),
        "closed by Task 8 of the citation grounding plan; move this to grounding.rs inverted"
    );
}

// --- A denial the cue list does not know (CAP-CORRECTNESS-012) --------------

#[test]
fn a_doubt_the_cue_list_does_not_know_still_states_the_relation() {
    // The phrasing is there word for word, and the sentence doubts it in a word
    // no cue list carries. Cue lists are finite; this is what that costs.
    let proposition = Proposition::new("Fabrikam", "acquired", "Contoso");
    let phrasebook = Phrasebook::new().with_relation(
        "acquired",
        vec![Phrasing::parse("{from} acquired {to}").expect("a phrasing")],
    );
    let certificate = check(
        &quoted(proposition.clone(), proposition, "low"),
        &snapshot_over(
            "Analysts doubted that Fabrikam acquired Contoso.",
            Authority::High,
        )
        .with_phrasebook(phrasebook),
    );

    assert_eq!(
        certificate.verdict,
        Verdict::Accepted,
        "a known cost of finite cue lists; if this changes, restate CAP-CORRECTNESS-012"
    );
}
