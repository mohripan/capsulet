//! What it takes for a citation to be grounded.
//!
//! `Cite` establishes that the cited bytes are the source's and that the
//! proposition's endpoints appear in them literally. Literally is the
//! load-bearing word, and these tests pin what it does and does not forgive.

use capsulet_core::{
    Authority, Evidence, EvidenceId, EvidenceSpan, MemoryScope, Source, SourceContent, SourceId,
};
use capsulet_kernel::{
    Certificate, MAX_CITED_EXCERPT_BYTES, Phrasebook, Phrasing, Proposal, Proposition, Rule,
    Snapshot, Verdict, check,
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

/// Whether citing `document` for a proposition about `subject` and `object`
/// discharges the citation.
///
/// Asked of the step rather than of the verdict. These tests pin what a
/// citation matches; a verdict also depends on what is wrapped around the
/// citation and on the goal it is held to, and neither is under test here.
fn cites(document: &str, subject: &str, object: &str) -> bool {
    check(&cite(subject, object), &snapshot_over(document))
        .discharged
        .iter()
        .any(|step| step.rule == "cite")
}

#[test]
fn composition_does_not_change_what_a_document_says() {
    // The stored document is whatever was ingested, and a source that writes "é"
    // as "e" plus a combining acute says the same thing as one that writes it as
    // a single codepoint. A citation that turned on which encoding the document
    // happened to use would be rejecting a true statement about it.
    let decomposed_document = "Acme renewed the cafe\u{0301} contract.";
    let composed_object = "caf\u{00E9}";

    assert!(
        !decomposed_document.contains(composed_object),
        "the fixture is only meaningful if the two forms differ byte for byte"
    );

    assert!(
        cites(decomposed_document, "Acme", composed_object),
        "a citation should survive a difference in Unicode composition"
    );
}

#[test]
fn a_proposal_that_is_not_normalised_cannot_be_pinned() {
    // The other direction is not symmetric, and should not be. A *document* is
    // whatever it is; a *proposal* enters a digest, and the IR's canonical
    // encoding refuses text that is not NFC so that two spellings of one string
    // cannot produce two digests. A proposer emitting decomposed text is asked
    // to normalise it rather than having it normalised for them, which would
    // make the digest depend on who did the normalising.
    let document = "Acme renewed the caf\u{00E9} contract.";
    let decomposed_object = "cafe\u{0301}";

    let certificate = check(&cite("Acme", decomposed_object), &snapshot_over(document));

    assert_eq!(certificate.verdict, Verdict::Rejected);
    assert!(
        certificate
            .errors
            .iter()
            .any(|error| error.code == "proposal_not_encodable"),
        "expected the refusal to be about pinning, got {:?}",
        certificate
            .errors
            .iter()
            .map(|error| error.code.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn case_and_whitespace_still_do_not_matter() {
    // Models reflow whitespace and change case when quoting; neither changes
    // what the source says.
    let document = "Acme   renewed\nthe Contoso contract.";
    assert!(cites(document, "acme", "THE   CONTOSO   CONTRACT"));
}

#[test]
fn a_word_the_document_does_not_contain_is_still_refused() {
    let document = "Acme renewed the Contoso contract.";
    assert!(!cites(document, "Acme", "Fabrikam"));
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

    assert!(
        !cites(&document, "Acme", "Contoso"),
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

    assert!(cites(&document, "Acme", "Contoso"));
}

#[test]
fn a_term_is_not_found_inside_a_longer_word() {
    // A term is a run of whole words. `Ann` is not in "announced", whatever the
    // bytes say; `annual` is.
    let document = "The CEO announced the annual results.";
    assert!(!cites(document, "CEO", "Ann"));
    assert!(cites(document, "CEO", "annual"));
}

#[test]
fn a_number_is_not_found_inside_a_longer_number() {
    let document = "The notice period is 300 days.";
    assert!(!cites(document, "notice period", "30"));
    assert!(cites(document, "notice period", "300 days"));
}

#[test]
fn scripts_written_without_spaces_still_match_inside_a_sentence() {
    // The reason word boundaries waited: a rule that refuses a match flanked by
    // letters would refuse every one of these. Unicode's own boundaries give
    // each Han ideograph, Hiragana character and Thai grapheme a boundary of its
    // own, and keep a Katakana run whole.
    assert!(cites("我喜欢吃苹果。", "我", "苹果"));
    assert!(cites("ฉันชอบกินแอปเปิ้ล", "ฉัน", "แอปเปิ้ล"));
    assert!(cites(
        "コンピュータを使います。",
        "コンピュータ",
        "使います"
    ));
    assert!(
        !cites("コンピュータを使います。", "コン", "使います"),
        "a Katakana word is a word, and a piece of it is not"
    );
}

#[test]
fn case_is_folded_rather_than_lowercased() {
    // Lowercasing is for display: it leaves `ß` alone while `SS` becomes `ss`,
    // and the ligature `ﬁ` is not `fi`. Case folding is the operation built for
    // matching, and it brings each pair to the same letters.
    assert!(cites(
        "Acme is located at Hauptstra\u{00DF}e 5.",
        "Acme",
        "HAUPTSTRASSE 5"
    ));
    assert!(cites("Acme leads \u{FB01}nance.", "Acme", "finance"));
}

#[test]
fn typographic_quotes_and_dashes_are_their_plain_forms() {
    assert!(cites(
        "Acme\u{2019}s renewal was approved.",
        "Acme's renewal",
        "approved"
    ));
    assert!(cites(
        "Revenue fell by \u{2212}5 percent.",
        "Revenue",
        "-5 percent"
    ));
}

#[test]
fn an_invisible_character_does_not_hide_a_word() {
    // A soft hyphen or a zero-width space inside a word changes nothing a
    // reader sees, so it changes nothing a citation is judged on.
    assert!(cites(
        "Acme renewed the Con\u{00AD}toso contract.",
        "Acme",
        "Contoso"
    ));
    assert!(cites(
        "Acme renewed the Con\u{200B}toso contract.",
        "Acme",
        "Contoso"
    ));
}

#[test]
fn a_possessive_or_an_elision_does_not_hide_a_name() {
    // Unicode keeps "Acme's" and "l'accord" whole, as single words. Treated that
    // way, a possessive would hide the name it is attached to, and French
    // elision would hide half its nouns. An apostrophe is a boundary here.
    assert!(cites("Acme's renewal was approved.", "Acme", "renewal"));
    assert!(cites("Acme a sign\u{00E9} l'accord.", "Acme", "accord"));
}

#[test]
fn the_unicode_tables_a_citation_is_judged_by_are_pinned() {
    // Word boundaries and normalisation decide what a citation matches. A
    // dependency bump that moves their Unicode version changes decisions, so it
    // has to arrive with a bump of `CLAIM_REASONING_RULES` and of this pin. The
    // test is what makes that a decision rather than a side effect of
    // `cargo update`.
    assert_eq!(unicode_segmentation::UNICODE_VERSION, (17, 0, 0));
    assert_eq!(unicode_normalization::UNICODE_VERSION, (17, 0, 0));
}

// --- A citation states a relation only in a declared phrasing ---------------

/// The contract's phrasings for the two acquisition relations below.
fn acquisitions() -> Phrasebook {
    let phrasings = |templates: &[&str]| {
        templates
            .iter()
            .map(|template| Phrasing::parse(template).expect("a phrasing"))
            .collect()
    };
    Phrasebook::new()
        .with_relation(
            "acquired",
            phrasings(&[
                "{from} acquired {to}",
                "{to} was acquired by {from}",
                "{from}'s acquisition of {to}",
            ]),
        )
        .with_relation(
            "was_acquired_by",
            phrasings(&["{from} was acquired by {to}", "{to} acquired {from}"]),
        )
}

/// Decides `subject predicate object` as a trusted citation of `document` from
/// a high-authority source, judged by the acquisition phrasings.
fn trusted(document: &str, subject: &str, predicate: &str, object: &str) -> Certificate {
    let proposition = Proposition::new(subject, predicate, object);
    let proposal = Proposal {
        goal: proposition.clone(),
        derivation: Rule::Trust {
            premise: Box::new(Rule::Cite {
                evidence_id: "ev_1".to_string(),
                proposition,
            }),
            min_authority: "low".to_string(),
        },
    };
    check(
        &proposal,
        &snapshot_over(document).with_phrasebook(acquisitions()),
    )
}

/// Whether `document` states that Fabrikam acquired Contoso.
fn states_the_acquisition(document: &str) -> bool {
    trusted(document, "Fabrikam", "acquired", "Contoso").verdict == Verdict::Accepted
}

/// The routing a refused statement gets: it is the proposer's to re-propose
/// as a reading.
fn refused_as_unstated(certificate: &Certificate) -> bool {
    certificate.verdict == Verdict::Rejected
        && certificate.errors[0].code == "relation_not_stated"
        && certificate.errors[0].repair_owner == "proposer"
}

#[test]
fn a_declared_phrasing_states_the_relation() {
    assert!(states_the_acquisition("Fabrikam acquired Contoso in 2024."));
    assert!(states_the_acquisition(
        "Contoso was acquired by Fabrikam in 2024."
    ));
    assert!(states_the_acquisition(
        "Fabrikam\u{2019}s acquisition of Contoso closed in May."
    ));

    let certificate = trusted(
        "Fabrikam acquired Contoso in 2024.",
        "Fabrikam",
        "acquired",
        "Contoso",
    );
    let cite = certificate
        .discharged
        .iter()
        .find(|step| step.rule == "cite")
        .expect("the citation discharged");
    assert!(
        cite.detail.contains("{from} acquired {to}"),
        "the certificate should name the phrasing it relied on: {}",
        cite.detail
    );
}

#[test]
fn a_negated_statement_is_not_the_statement() {
    let certificate = trusted(
        "Fabrikam did not acquire Contoso.",
        "Fabrikam",
        "acquired",
        "Contoso",
    );
    assert!(refused_as_unstated(&certificate), "{certificate:?}");

    // The phrasing is there word for word; the sentence denies it.
    assert!(!states_the_acquisition(
        "It is not true that Fabrikam acquired Contoso."
    ));
}

#[test]
fn reversed_roles_are_not_the_statement() {
    let certificate = trusted(
        "Contoso acquired Fabrikam in 2024.",
        "Fabrikam",
        "acquired",
        "Contoso",
    );
    assert!(refused_as_unstated(&certificate), "{certificate:?}");
}

#[test]
fn a_different_relation_between_the_same_terms_is_not_the_statement() {
    let certificate = trusted(
        "Contoso sued Fabrikam over a patent.",
        "Contoso",
        "was_acquired_by",
        "Fabrikam",
    );
    assert!(refused_as_unstated(&certificate), "{certificate:?}");
}

#[test]
fn a_hedged_conditional_or_questioned_statement_is_not_the_statement() {
    assert!(!states_the_acquisition(
        "Fabrikam may acquire Contoso next year."
    ));
    assert!(!states_the_acquisition(
        "If Fabrikam acquired Contoso, prices would rise."
    ));
    assert!(!states_the_acquisition(
        "Reportedly, Fabrikam acquired Contoso."
    ));
    assert!(!states_the_acquisition("Fabrikam acquired Contoso?"));
}

#[test]
fn a_cue_in_another_sentence_does_not_block_a_statement() {
    // A cue speaks for its own sentence. The second sentence's denial is about
    // Northwind and says nothing about the first.
    assert!(states_the_acquisition(
        "Fabrikam acquired Contoso. It did not acquire Northwind."
    ));
}

#[test]
fn hard_wrapped_text_is_one_sentence() {
    // Extracted text breaks lines mid-sentence. A single line break is a space;
    // a blank line ends a paragraph, and the sentence with it.
    assert!(states_the_acquisition(
        "Fabrikam\nacquired Contoso in 2024."
    ));
    assert!(!states_the_acquisition(
        "Fabrikam\n\nacquired Contoso in 2024."
    ));
}

#[test]
fn a_relation_with_no_declared_phrasing_is_only_mentioned() {
    // Nothing declares how `renewed` is stated, so the citation mentions its
    // terms and no more: trusting it is refused, and reading it is conditional.
    let document = "Acme renewed the Contoso contract.";
    let certificate = trusted(document, "Acme", "renewed", "the Contoso contract");
    assert!(refused_as_unstated(&certificate), "{certificate:?}");

    let goal = Proposition::new("Acme", "renewed", "the Contoso contract");
    let reading = Proposal {
        goal: goal.clone(),
        derivation: Rule::Interpret {
            premise: Box::new(Rule::Cite {
                evidence_id: "ev_1".to_string(),
                proposition: goal.clone(),
            }),
            proposition: goal,
            rationale: "the sentence says Acme renewed it".to_string(),
        },
    };
    let certificate = check(
        &reading,
        &snapshot_over(document).with_phrasebook(acquisitions()),
    );
    assert_eq!(certificate.verdict, Verdict::Conditional);
    assert!(certificate.residuals[0].from.starts_with("mentions("));
}

#[test]
fn a_phrasing_needs_both_slots_and_a_word() {
    assert!(Phrasing::parse("{from} acquired {to}").is_ok());
    for template in [
        "{from} {to}",
        "{from}, {to}",
        "{from} acquired",
        "{from} acquired {to} and {to}",
        "{from} {verb} {to}",
    ] {
        assert!(
            Phrasing::parse(template).is_err(),
            "{template:?} should not be a phrasing"
        );
    }
}
