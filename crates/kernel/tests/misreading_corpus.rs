//! What catches a misreading, against what a reader would say.
//!
//! An answer can quote a contract exactly and still get it wrong, or be right and
//! answer nothing. Each case here is a clause of one agreement, a question, the
//! answer a proposer would give, a reader's label for that answer, and the
//! mechanism expected to catch it. Three things come out.
//!
//! - **The invariant:** no answer a reader labels misread or non-responsive
//!   reaches `accepted`, apart from the limitations named below. Asserted, not
//!   counted.
//! - **Attribution:** every case is caught by the mechanism it names, so a
//!   change that catches a case for the wrong reason shows up as a change.
//! - **The cost:** how many sound answers still reach `accepted`. A kernel that
//!   rejected everything would hold the invariant and be useless.
//!
//! This is a fixture written by the people who wrote the kernel, on one
//! agreement written for it. It bounds the mechanisms on sentences like these and
//! says nothing about real contracts or real questions (ADR 0021, #162).
//!
//! Every answer has the shape `capsulet-proposer` gives a direct quote:
//! `Trust { Cite(goal), min_authority: "low" }`, asked under a pinned query.
//!
//! The cases are data, in `fixtures/misreading-corpus.json`, so a domain pack
//! can be held to the same corpus under its own contract: the contract-clauses
//! pack's test runs every case here and pins what catches each one there.

use capsulet_core::{
    Authority, Evidence, EvidenceId, EvidenceSpan, MemoryScope, Source, SourceContent, SourceId,
};
use capsulet_kernel::{
    Certificate, ContextPolicy, Phrasebook, Phrasing, Proposal, Proposition, Query, Question, Rule,
    Slot, Snapshot, Verdict, check,
};
use serde::Deserialize;

/// What a reader says about the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Reader {
    /// It is what the agreement means, and it answers the question.
    Sound,
    /// The quote is exact, and read out of context it says something the
    /// agreement does not.
    Misread,
    /// It is supported, and it answers some other question.
    NonResponsive,
}
use Reader::{Misread, NonResponsive, Sound};

/// What stops an answer reaching `accepted`, read off its certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Caught {
    /// A qualified clause states nothing outright (`relation_qualified`).
    Qualifier,
    /// A defined term is used without its definition (a residual).
    Definition,
    /// The answer fills another query (`goal_not_responsive`).
    Query,
    /// No declared phrasing states it, a cue among them (`relation_not_stated`).
    Unstated,
    /// Nothing: the answer is accepted.
    Nothing,
}

#[derive(Debug, Deserialize)]
struct Corpus {
    agreement: String,
    cases: Vec<Case>,
    limitations: Vec<RecordedLimitation>,
}

#[derive(Debug, Deserialize)]
struct Case {
    clause: String,
    question: String,
    query: Asked,
    answer: Answer,
    reader: Reader,
    expected: Caught,
}

/// The query a case is asked under: a relation, and a term or `null` for the
/// wanted slot.
#[derive(Debug, Deserialize)]
struct Asked {
    relation: String,
    subject: Option<String>,
    object: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Answer {
    subject: String,
    predicate: String,
    object: String,
}

/// A misreading the mechanisms are known not to catch, kept in the corpus.
#[derive(Debug, Deserialize)]
struct RecordedLimitation {
    clause: String,
    why: String,
}

fn corpus() -> Corpus {
    serde_json::from_str(include_str!("fixtures/misreading-corpus.json")).expect("the corpus reads")
}

fn legal() -> ContextPolicy {
    ContextPolicy {
        provisions: vec!["{ref}.".to_string()],
        references: vec!["Section {ref}".to_string()],
        subject_to: vec![
            "subject to".to_string(),
            "except as provided in".to_string(),
        ],
        overrides: vec!["notwithstanding".to_string()],
        conditions: vec!["provided that".to_string()],
        definitions: vec!["\"{term}\" means".to_string()],
    }
}

fn phrasebook() -> Phrasebook {
    let phrasing = |template: &str| Phrasing::parse(template).expect("phrasing");
    Phrasebook::new()
        .with_relation("are", vec![phrasing("{from} are {to}")])
        .with_relation("liable_for", vec![phrasing("{from} is liable for {to}")])
        .with_relation("may_terminate", vec![phrasing("{from} may terminate {to}")])
        .with_relation(
            "may_assign_to",
            vec![phrasing("{from} may assign this agreement to an {to}")],
        )
        .with_relation("renews_for", vec![phrasing("{from} renews for {to}")])
        .with_relation("may_raise", vec![phrasing("{from} may raise {to}")])
        .with_relation(
            "pay_within",
            vec![phrasing("{from} shall pay the fees within {to}")],
        )
        .with_relation(
            "governed_by",
            vec![phrasing("{from} is governed by the laws of {to}")],
        )
        .with_context(legal())
}

fn certificate_of(agreement: &str, case: &Case) -> Certificate {
    let scope = MemoryScope::new("acme", "legal").expect("scope");
    let source_id = SourceId::new("agreement").expect("source id");
    let content = SourceContent::new(source_id.clone(), agreement).expect("content");
    let start = agreement
        .find(&case.clause)
        .expect("the clause is in the agreement");
    let evidence = Evidence::new(
        EvidenceId::new("ev_clause").expect("evidence id"),
        scope.clone(),
        source_id.clone(),
        "clause",
        &case.clause,
        "2026-11-13T00:00:00Z",
    )
    .expect("evidence")
    .with_span(
        EvidenceSpan::new(start, start + case.clause.len(), content.content_hash()).expect("span"),
    );
    let slot = |term: Option<&String>| term.map_or(Slot::Wanted, Slot::bound);
    let snapshot = Snapshot::new()
        .with_phrasebook(phrasebook())
        .with_source(
            Source::new(
                source_id,
                scope,
                "document",
                None,
                "Agreement",
                Authority::High,
            )
            .expect("source"),
        )
        .with_source_content(content)
        .with_evidence(evidence)
        .with_question(Question::asked(&case.question).with_query(Query::new(
            &case.query.relation,
            slot(case.query.subject.as_ref()),
            slot(case.query.object.as_ref()),
        )));
    let goal = Proposition::new(
        &case.answer.subject,
        &case.answer.predicate,
        &case.answer.object,
    );
    let proposal = Proposal {
        schema_version: Proposal::current_schema_version(),
        question_reading: None,
        derivation: Rule::Trust {
            premise: Box::new(Rule::Cite {
                evidence_id: "ev_clause".to_string(),
                proposition: goal.clone(),
                read_with: Vec::new(),
            }),
            min_authority: "low".to_string(),
        },
        goal,
    };
    check(&proposal, &snapshot)
}

/// What stopped the answer, read off its certificate alone.
fn caught(certificate: &Certificate) -> Caught {
    let has = |code: &str| certificate.errors.iter().any(|error| error.code == code);
    if has("goal_not_responsive") {
        Caught::Query
    } else if has("relation_qualified") {
        Caught::Qualifier
    } else if has("relation_not_stated") {
        Caught::Unstated
    } else if certificate
        .residuals
        .iter()
        .any(|residual| residual.rationale.contains("defined term"))
    {
        Caught::Definition
    } else {
        assert_eq!(
            certificate.verdict,
            Verdict::Accepted,
            "an answer stopped by nothing this corpus names: {certificate:?}"
        );
        Caught::Nothing
    }
}

#[test]
fn no_misread_or_non_responsive_answer_reaches_accepted_but_the_recorded_limitations() {
    let corpus = corpus();
    for case in corpus.cases.iter().filter(|case| case.reader != Sound) {
        let certificate = certificate_of(&corpus.agreement, case);
        let recorded = corpus
            .limitations
            .iter()
            .any(|limitation| limitation.clause == case.clause);
        assert_eq!(
            certificate.verdict == Verdict::Accepted,
            recorded,
            "{:?} answer from {:?}: {certificate:?}",
            case.reader,
            case.clause
        );
    }
}

#[test]
fn every_case_is_caught_by_the_mechanism_it_names() {
    let corpus = corpus();
    for case in &corpus.cases {
        assert_eq!(
            caught(&certificate_of(&corpus.agreement, case)),
            case.expected,
            "{:?}",
            case.clause
        );
    }
}

#[test]
fn every_recorded_limitation_is_still_the_limitation_it_says_it_is() {
    let corpus = corpus();
    for limitation in &corpus.limitations {
        let case = corpus
            .cases
            .iter()
            .find(|case| case.clause == limitation.clause)
            .expect("a recorded limitation is a case in the corpus");
        assert_eq!(case.reader, Misread, "{}", limitation.why);
        assert_eq!(
            certificate_of(&corpus.agreement, case).verdict,
            Verdict::Accepted,
            "fixed? then take it out of the corpus's limitations: {}",
            limitation.why
        );
    }
}

/// How many cases of each kind the corpus holds, and how they come out. Pinned,
/// so a change to any of them has to be looked at rather than absorbed.
#[test]
fn the_counts_are_what_was_recorded() {
    let corpus = corpus();
    let count = |reader: Reader, accepted: bool| {
        corpus
            .cases
            .iter()
            .filter(|case| case.reader == reader)
            .filter(|case| {
                (certificate_of(&corpus.agreement, case).verdict == Verdict::Accepted) == accepted
            })
            .count()
    };
    // Misreadings caught, and the one that is not.
    assert_eq!((count(Misread, false), count(Misread, true)), (5, 1));
    // Non-responsive answers caught.
    assert_eq!(
        (count(NonResponsive, false), count(NonResponsive, true)),
        (2, 0)
    );
    // Sound answers that still reach `accepted`: the cost side.
    assert_eq!((count(Sound, true), count(Sound, false)), (2, 0));
}
