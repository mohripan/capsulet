//! A cited clause read with what qualifies it, what it refers to, and what
//! defines its terms.
//!
//! An answer can quote a contract exactly and still misread it, because the
//! clause it quotes is qualified two provisions later, or uses a term the
//! contract defines. Whether the reading is right is not decided here. These
//! tests pin what is: a qualified clause states nothing outright, and every
//! provision or definition it depends on and the derivation leaves out is a
//! residual naming it.

use capsulet_core::{
    Authority, Evidence, EvidenceId, EvidenceSpan, MemoryScope, Source, SourceContent, SourceId,
};
use capsulet_kernel::{
    Certificate, ContextPolicy, Phrasebook, Phrasing, Proposal, Proposition, Rule, Snapshot,
    SnapshotDocument, Verdict, check,
};

const AGREEMENT: &str = "1. Definitions\n\
\"Affiliate\" means an entity that controls a party.\n\
9. Termination\n\
Subject to Section 12, either party may terminate this agreement on thirty days' notice.\n\
12. Minimum term\n\
Notwithstanding Section 9, no party may terminate this agreement in its first year.\n\
14. Assignment\n\
Acme may assign this agreement to an Affiliate.\n\
15. Renewal\n\
Acme may renew this agreement, provided that it gives notice.\n";

const TERMINATION: &str =
    "Subject to Section 12, either party may terminate this agreement on thirty days' notice.";
const MINIMUM_TERM: &str =
    "Notwithstanding Section 9, no party may terminate this agreement in its first year.";
const ASSIGNMENT: &str = "Acme may assign this agreement to an Affiliate.";
const DEFINITION: &str = "\"Affiliate\" means an entity that controls a party.";
const RENEWAL: &str = "Acme may renew this agreement, provided that it gives notice.";

fn legal() -> ContextPolicy {
    ContextPolicy {
        provisions: vec!["{ref}.".to_string()],
        references: vec!["Section {ref}".to_string()],
        subject_to: vec!["subject to".to_string()],
        overrides: vec!["notwithstanding".to_string()],
        conditions: vec!["provided that".to_string()],
        definitions: vec!["\"{term}\" means".to_string()],
    }
}

fn phrasebook(context: ContextPolicy) -> Phrasebook {
    let phrasing = |template: &str| Phrasing::parse(template).expect("phrasing");
    Phrasebook::new()
        .with_relation("may_terminate", vec![phrasing("{from} may terminate {to}")])
        .with_relation(
            "may_assign_to",
            vec![phrasing("{from} may assign this agreement to an {to}")],
        )
        .with_relation("may_renew", vec![phrasing("{from} may renew {to}")])
        .with_context(context)
}

fn source_id() -> SourceId {
    SourceId::new("contract").expect("source id")
}

fn evidence(id: &str, sentence: &str, source: &SourceContent) -> Evidence {
    let start = source
        .text()
        .find(sentence)
        .expect("the sentence is in the text");
    Evidence::new(
        EvidenceId::new(id).expect("evidence id"),
        MemoryScope::new("acme", "legal").expect("scope"),
        source.source_id().clone(),
        id,
        sentence,
        "2026-11-13T00:00:00Z",
    )
    .expect("evidence")
    .with_span(
        EvidenceSpan::new(start, start + sentence.len(), source.content_hash()).expect("span"),
    )
}

fn snapshot(policy: ContextPolicy) -> Snapshot {
    let content = SourceContent::new(source_id(), AGREEMENT).expect("content");
    let mut snapshot = Snapshot::new()
        .with_phrasebook(phrasebook(policy))
        .with_source(
            Source::new(
                source_id(),
                MemoryScope::new("acme", "legal").expect("scope"),
                "document",
                None,
                "Master agreement",
                Authority::High,
            )
            .expect("source"),
        );
    for (id, sentence) in [
        ("ev_termination", TERMINATION),
        ("ev_minimum_term", MINIMUM_TERM),
        ("ev_assignment", ASSIGNMENT),
        ("ev_definition", DEFINITION),
        ("ev_renewal", RENEWAL),
    ] {
        snapshot = snapshot.with_evidence(evidence(id, sentence, &content));
    }
    snapshot.with_source_content(content)
}

fn cite(evidence_id: &str, proposition: &Proposition, read_with: &[&str]) -> Rule {
    Rule::Cite {
        evidence_id: evidence_id.to_string(),
        proposition: proposition.clone(),
        read_with: read_with.iter().map(ToString::to_string).collect(),
    }
}

fn trusted(evidence_id: &str, goal: Proposition, read_with: &[&str]) -> Proposal {
    Proposal {
        schema_version: Proposal::current_schema_version(),
        question_reading: None,
        derivation: Rule::Trust {
            premise: Box::new(cite(evidence_id, &goal, read_with)),
            min_authority: "low".to_string(),
        },
        goal,
    }
}

fn read(evidence_id: &str, goal: Proposition, read_with: &[&str]) -> Proposal {
    Proposal {
        schema_version: Proposal::current_schema_version(),
        question_reading: None,
        derivation: Rule::Interpret {
            premise: Box::new(cite(evidence_id, &goal, read_with)),
            proposition: goal.clone(),
            rationale: "the clause gives either party a termination right".to_string(),
        },
        goal,
    }
}

fn termination() -> Proposition {
    Proposition::new("either party", "may_terminate", "this agreement")
}

fn codes(certificate: &Certificate) -> Vec<&str> {
    certificate
        .errors
        .iter()
        .map(|error| error.code.as_str())
        .collect()
}

#[test]
fn an_exactly_quoted_clause_that_another_qualifies_states_nothing_outright() {
    // Without the contract's structure declared, the quote is a statement and
    // the trusted answer is accepted, which is the misreading.
    let plain = check(
        &trusted("ev_termination", termination(), &[]),
        &snapshot(ContextPolicy::default()),
    );
    assert_eq!(plain.verdict, Verdict::Accepted, "{plain:?}");

    let certificate = check(
        &trusted("ev_termination", termination(), &[]),
        &snapshot(legal()),
    );
    assert_eq!(certificate.verdict, Verdict::Rejected);
    assert_eq!(codes(&certificate), ["relation_qualified"]);
    assert!(
        certificate.errors[0].message.contains("provision 12"),
        "{}",
        certificate.errors[0].message
    );
}

#[test]
fn a_reading_of_a_qualified_clause_names_every_qualifier_it_left_out() {
    let certificate = check(
        &read("ev_termination", termination(), &[]),
        &snapshot(legal()),
    );
    assert_eq!(certificate.verdict, Verdict::Conditional, "{certificate:?}");
    let rationales: Vec<&str> = certificate
        .residuals
        .iter()
        .map(|r| r.rationale.as_str())
        .collect();
    assert!(
        rationales
            .iter()
            .any(|r| r.contains("qualified by provision 12 (\"subject to\")")),
        "{rationales:?}"
    );
    assert!(
        rationales
            .iter()
            .any(|r| r.contains("provision 12 prevails over the cited clause")),
        "{rationales:?}"
    );

    // Read with provision 12, the residuals for leaving it out go, and the
    // reading itself stays: a qualified clause is still a reading.
    let with = check(
        &read("ev_termination", termination(), &["ev_minimum_term"]),
        &snapshot(legal()),
    );
    assert_eq!(with.verdict, Verdict::Conditional, "{with:?}");
    let [reading] = with.residuals.as_slice() else {
        panic!("only the reading should remain: {:?}", with.residuals);
    };
    assert_eq!(reading.evidence_ids, ["ev_termination", "ev_minimum_term"]);
}

#[test]
fn a_defined_term_is_read_with_its_definition() {
    let goal = Proposition::new("Acme", "may_assign_to", "Affiliate");
    let without = check(
        &trusted("ev_assignment", goal.clone(), &[]),
        &snapshot(legal()),
    );
    assert_eq!(without.verdict, Verdict::Conditional, "{without:?}");
    assert!(
        without.residuals[0]
            .rationale
            .contains("defined term \"Affiliate\", defined in provision 1"),
        "{:?}",
        without.residuals
    );
    let with = check(
        &trusted("ev_assignment", goal, &["ev_definition"]),
        &snapshot(legal()),
    );
    assert_eq!(with.verdict, Verdict::Accepted, "{with:?}");
}

#[test]
fn a_declared_condition_in_the_cited_sentence_qualifies_it() {
    let goal = Proposition::new("Acme", "may_renew", "this agreement");
    let certificate = check(&trusted("ev_renewal", goal, &[]), &snapshot(legal()));
    assert_eq!(codes(&certificate), ["relation_qualified"]);
    assert!(certificate.errors[0].message.contains("provided that"));
}

#[test]
fn a_clause_is_read_only_with_spans_of_its_own_text() {
    let other =
        SourceContent::new(SourceId::new("letter").expect("id"), DEFINITION).expect("content");
    let stray = evidence("ev_elsewhere", DEFINITION, &other);
    let snapshot = snapshot(legal())
        .with_source_content(other)
        .with_evidence(stray);
    let goal = Proposition::new("Acme", "may_assign_to", "Affiliate");
    let certificate = check(
        &trusted("ev_assignment", goal, &["ev_elsewhere"]),
        &snapshot,
    );
    assert_eq!(codes(&certificate), ["read_with_another_source"]);
}

#[test]
fn a_citation_that_reads_nothing_else_encodes_as_it_always_did() {
    let encoded =
        serde_json::to_value(cite("ev_termination", &termination(), &[])).expect("encodes");
    assert!(encoded.get("read_with").is_none(), "{encoded}");
}

#[test]
fn the_context_policy_travels_in_the_snapshot_document_and_a_hollow_template_is_refused() {
    let document = SnapshotDocument {
        context: legal(),
        ..SnapshotDocument::default()
    };
    let encoded = serde_json::to_value(&document).expect("encodes");
    assert_eq!(encoded["context"]["subject_to"][0], "subject to");
    let read: SnapshotDocument = serde_json::from_value(encoded).expect("reads");
    assert_eq!(
        read.rebuild().expect("rebuilds").phrasebook().context(),
        &legal()
    );

    let hollow = SnapshotDocument {
        context: ContextPolicy {
            references: vec!["Section".to_string()],
            ..ContextPolicy::default()
        },
        ..SnapshotDocument::default()
    };
    assert!(hollow.rebuild().is_err());
    let plain = serde_json::to_value(SnapshotDocument::default()).expect("encodes");
    assert!(plain.get("context").is_none(), "{plain}");
}
