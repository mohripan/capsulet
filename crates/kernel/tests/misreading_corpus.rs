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

use capsulet_core::{
    Authority, Evidence, EvidenceId, EvidenceSpan, MemoryScope, Source, SourceContent, SourceId,
};
use capsulet_kernel::{
    Certificate, ContextPolicy, Phrasebook, Phrasing, Proposal, Proposition, Query, Question, Rule,
    Slot, Snapshot, Verdict, check,
};

const AGREEMENT: &str = "1. Definitions\n\
\"Affiliate\" means an entity that controls, or is controlled by, a party.\n\
4. Fees\n\
The Customer shall pay the fees within 30 days of each invoice. \
Fees are non-refundable, except as provided in Section 11.\n\
7. Liability\n\
Subject to Section 8, the Supplier is liable for all damages.\n\
8. Limitation\n\
Notwithstanding Section 7, the Supplier is not liable for indirect damages.\n\
9. Termination\n\
Either party may terminate this agreement, provided that all fees are paid.\n\
10. Assignment\n\
The Supplier may assign this agreement to an Affiliate.\n\
11. Refunds\n\
The Supplier shall refund prepaid fees if it ends this agreement early.\n\
12. Renewal\n\
This agreement renews for one year unless either party objects.\n\
13. Governing law\n\
This agreement is governed by the laws of Ontario.\n\
14. Prices\n\
The Supplier may raise prices once a year. Any increase needs the Customer's written consent.\n";

/// What a reader says about the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
use Caught::{Definition, Nothing, Qualifier, Query as ByQuery, Unstated};

struct Case {
    clause: &'static str,
    question: &'static str,
    query: (&'static str, Option<&'static str>, Option<&'static str>),
    answer: (&'static str, &'static str, &'static str),
    reader: Reader,
    expected: Caught,
}

const CORPUS: &[Case] = &[
    // An exception two provisions away.
    Case {
        clause: "Fees are non-refundable, except as provided in Section 11.",
        question: "Are fees refundable?",
        query: ("are", Some("Fees"), None),
        answer: ("Fees", "are", "non-refundable"),
        reader: Misread,
        expected: Qualifier,
    },
    // Subject to one provision, and overridden by it.
    Case {
        clause: "Subject to Section 8, the Supplier is liable for all damages.",
        question: "What is the Supplier liable for?",
        query: ("liable_for", Some("the Supplier"), None),
        answer: ("the Supplier", "liable_for", "all damages"),
        reader: Misread,
        expected: Qualifier,
    },
    // A condition in the same sentence.
    Case {
        clause: "Either party may terminate this agreement, provided that all fees are paid.",
        question: "May either party terminate this agreement?",
        query: (
            "may_terminate",
            Some("Either party"),
            Some("this agreement"),
        ),
        answer: ("Either party", "may_terminate", "this agreement"),
        reader: Misread,
        expected: Qualifier,
    },
    // A defined term read in its ordinary sense.
    Case {
        clause: "The Supplier may assign this agreement to an Affiliate.",
        question: "To whom may the Supplier assign this agreement?",
        query: ("may_assign_to", Some("The Supplier"), None),
        answer: ("The Supplier", "may_assign_to", "Affiliate"),
        reader: Misread,
        expected: Definition,
    },
    // A condition the kernel's own cues already know.
    Case {
        clause: "This agreement renews for one year unless either party objects.",
        question: "How long does this agreement renew for?",
        query: ("renews_for", Some("This agreement"), None),
        answer: ("This agreement", "renews_for", "one year"),
        reader: Misread,
        expected: Unstated,
    },
    // The qualification is a sentence of its own, with no declared cue or
    // reference. Nothing finds it: see RECORDED_LIMITATIONS.
    Case {
        clause: "The Supplier may raise prices once a year.",
        question: "May the Supplier raise prices?",
        query: ("may_raise", Some("The Supplier"), Some("prices")),
        answer: ("The Supplier", "may_raise", "prices"),
        reader: Misread,
        expected: Nothing,
    },
    // Supported, and beside the point.
    Case {
        clause: "The Customer shall pay the fees within 30 days of each invoice.",
        question: "Which law governs this agreement?",
        query: ("governed_by", Some("This agreement"), None),
        answer: ("The Customer", "pay_within", "30 days"),
        reader: NonResponsive,
        expected: ByQuery,
    },
    Case {
        clause: "This agreement is governed by the laws of Ontario.",
        question: "When must the Customer pay?",
        query: ("pay_within", Some("The Customer"), None),
        answer: ("This agreement", "governed_by", "Ontario"),
        reader: NonResponsive,
        expected: ByQuery,
    },
    // Sound answers.
    Case {
        clause: "This agreement is governed by the laws of Ontario.",
        question: "Which law governs this agreement?",
        query: ("governed_by", Some("This agreement"), None),
        answer: ("This agreement", "governed_by", "Ontario"),
        reader: Sound,
        expected: Nothing,
    },
    Case {
        clause: "The Customer shall pay the fees within 30 days of each invoice.",
        question: "When must the Customer pay?",
        query: ("pay_within", Some("The Customer"), None),
        answer: ("The Customer", "pay_within", "30 days"),
        reader: Sound,
        expected: Nothing,
    },
];

/// A misreading the mechanisms are known not to catch, kept in the corpus.
struct RecordedLimitation {
    clause: &'static str,
    why: &'static str,
}

const RECORDED_LIMITATIONS: &[RecordedLimitation] = &[RecordedLimitation {
    clause: "The Supplier may raise prices once a year.",
    why: "a qualification written as a sentence of its own, with no declared cue and no reference, \
          is not found: only what the contract declares is read with a clause",
}];

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

fn certificate_of(case: &Case) -> Certificate {
    let scope = MemoryScope::new("acme", "legal").expect("scope");
    let source_id = SourceId::new("agreement").expect("source id");
    let content = SourceContent::new(source_id.clone(), AGREEMENT).expect("content");
    let start = AGREEMENT
        .find(case.clause)
        .expect("the clause is in the agreement");
    let evidence = Evidence::new(
        EvidenceId::new("ev_clause").expect("evidence id"),
        scope.clone(),
        source_id.clone(),
        "clause",
        case.clause,
        "2026-11-13T00:00:00Z",
    )
    .expect("evidence")
    .with_span(
        EvidenceSpan::new(start, start + case.clause.len(), content.content_hash()).expect("span"),
    );
    let slot = |term: Option<&str>| term.map_or(Slot::Wanted, Slot::bound);
    let (relation, subject, object) = case.query;
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
        .with_question(Question::asked(case.question).with_query(Query::new(
            relation,
            slot(subject),
            slot(object),
        )));
    let goal = Proposition::new(case.answer.0, case.answer.1, case.answer.2);
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
        ByQuery
    } else if has("relation_qualified") {
        Qualifier
    } else if has("relation_not_stated") {
        Unstated
    } else if certificate
        .residuals
        .iter()
        .any(|residual| residual.rationale.contains("defined term"))
    {
        Definition
    } else {
        assert_eq!(
            certificate.verdict,
            Verdict::Accepted,
            "an answer stopped by nothing this corpus names: {certificate:?}"
        );
        Nothing
    }
}

#[test]
fn no_misread_or_non_responsive_answer_reaches_accepted_but_the_recorded_limitations() {
    for case in CORPUS.iter().filter(|case| case.reader != Sound) {
        let certificate = certificate_of(case);
        let recorded = RECORDED_LIMITATIONS
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
    for case in CORPUS {
        assert_eq!(
            caught(&certificate_of(case)),
            case.expected,
            "{:?}",
            case.clause
        );
    }
}

#[test]
fn every_recorded_limitation_is_still_the_limitation_it_says_it_is() {
    for limitation in RECORDED_LIMITATIONS {
        let case = CORPUS
            .iter()
            .find(|case| case.clause == limitation.clause)
            .expect("a recorded limitation is a case in the corpus");
        assert_eq!(case.reader, Misread, "{}", limitation.why);
        assert_eq!(
            certificate_of(case).verdict,
            Verdict::Accepted,
            "fixed? then take it out of RECORDED_LIMITATIONS: {}",
            limitation.why
        );
    }
}

/// How many cases of each kind the corpus holds, and how they come out. Pinned,
/// so a change to any of them has to be looked at rather than absorbed.
#[test]
fn the_counts_are_what_was_recorded() {
    let count = |reader: Reader, accepted: bool| {
        CORPUS
            .iter()
            .filter(|case| case.reader == reader)
            .filter(|case| (certificate_of(case).verdict == Verdict::Accepted) == accepted)
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
