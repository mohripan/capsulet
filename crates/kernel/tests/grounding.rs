//! What it takes for a citation to be grounded.
//!
//! `Cite` establishes only that a source *said* something, and only when the
//! proposition's endpoints appear literally in the cited span. Literally is the
//! load-bearing word, and these tests pin what it does and does not forgive.

use capsulet_core::{
    Authority, Evidence, EvidenceId, EvidenceSpan, MemoryScope, Source, SourceContent, SourceId,
};
use capsulet_kernel::{
    MAX_CITED_EXCERPT_BYTES, Proposal, Proposition, Rule, Snapshot, Verdict, check,
};

fn scope() -> MemoryScope {
    MemoryScope::new("acme", "prod").expect("scope")
}

fn source_id() -> SourceId {
    SourceId::new("src_1").expect("source id")
}

/// A snapshot in which `document` is the stored text and the whole of it is
/// cited verbatim.
fn snapshot_over(document: &str) -> Snapshot {
    let content = SourceContent::new(source_id(), document).expect("content");
    let evidence = Evidence::new(
        EvidenceId::new("ev_1").expect("evidence id"),
        scope(),
        source_id(),
        "para-1",
        document,
        "2026-03-02T00:00:00Z",
    )
    .expect("evidence")
    .with_span(EvidenceSpan::new(0, document.len(), content.content_hash()).expect("span"));

    Snapshot::new()
        .with_source(
            Source::new(
                source_id(),
                scope(),
                "document",
                None,
                "Renewal memo",
                Authority::High,
            )
            .expect("source"),
        )
        .with_source_content(content)
        .with_evidence(evidence)
}

fn cite(subject: &str, object: &str) -> Proposal {
    Proposal {
        goal: Proposition::new(subject, "says", object),
        derivation: Rule::Cite {
            evidence_id: "ev_1".to_string(),
            proposition: Proposition::new(subject, "says", object),
        },
    }
}

/// `Says(source, P)` is what `Cite` concludes, so the goal has to be stated that
/// way for the derivation to reach it.
fn decide(document: &str, subject: &str, object: &str) -> Verdict {
    let snapshot = snapshot_over(document);
    let proposal = cite(subject, object);
    check(&proposal, &snapshot).verdict
}

#[test]
fn composition_does_not_change_what_a_document_says() {
    // The same word, written two ways Unicode considers equivalent: "é" as one
    // codepoint, and "e" followed by a combining acute. A model quoting from a
    // decomposed source and a store holding the composed form are quoting the
    // same text, and a citation that turns on which encoding was used is
    // rejecting a true statement about the document.
    let composed = "Acme renewed the caf\u{00E9} contract.";
    let decomposed_object = "cafe\u{0301}";

    assert!(
        !composed.contains(decomposed_object),
        "the fixture is only meaningful if the two forms differ byte for byte"
    );

    assert_eq!(
        decide(composed, "Acme", decomposed_object),
        Verdict::Accepted,
        "a citation should survive a difference in Unicode composition"
    );
}

#[test]
fn composition_is_normalised_in_both_directions() {
    let decomposed = "Acme renewed the cafe\u{0301} contract.";
    assert_eq!(decide(decomposed, "Acme", "caf\u{00E9}"), Verdict::Accepted);
}

#[test]
fn case_and_whitespace_still_do_not_matter() {
    // Models reflow whitespace and change case when quoting; neither changes
    // what the source says.
    let document = "Acme   renewed\nthe Contoso contract.";
    assert_eq!(
        decide(document, "acme", "THE   CONTOSO   CONTRACT"),
        Verdict::Accepted
    );
}

#[test]
fn a_word_the_document_does_not_contain_is_still_refused() {
    let document = "Acme renewed the Contoso contract.";
    assert_eq!(decide(document, "Acme", "Fabrikam"), Verdict::Rejected);
}

#[test]
fn an_excerpt_longer_than_the_bound_is_not_a_citation() {
    // A citation points at a span. A span the size of a document points at
    // nothing: every short phrase is "contained" in it, so containment stops
    // being evidence of anything.
    let mut document = String::from("Acme renewed the Contoso contract. ");
    while document.len() <= MAX_CITED_EXCERPT_BYTES {
        document.push_str("Filler sentence that says nothing in particular. ");
    }

    assert_eq!(
        decide(&document, "Acme", "Contoso"),
        Verdict::Rejected,
        "the phrase is present, and the span is too large to be pointing at it"
    );
}

#[test]
fn an_excerpt_at_the_bound_is_still_a_citation() {
    let mut document = String::from("Acme renewed the Contoso contract. ");
    while document.len() < MAX_CITED_EXCERPT_BYTES {
        document.push('.');
    }
    assert_eq!(document.len(), MAX_CITED_EXCERPT_BYTES);

    assert_eq!(decide(&document, "Acme", "Contoso"), Verdict::Accepted);
}
