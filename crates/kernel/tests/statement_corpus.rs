//! What the kernel concludes, against what a reader would.
//!
//! Each case is a sentence, a proposed fact, and a human's label for whether the
//! sentence states it. Two numbers come out, and they are not equally important.
//!
//! The invariant: **no case a reader marks not stated reaches `accepted`.** That is the
//! guarantee, and it is asserted rather than counted. The one known exception is named
//! in the corpus as a recorded limitation, and it stays in, because a corpus that
//! dropped its awkward case would be measuring its own editing.
//!
//! The ratio: how many *stated* cases reach `accepted`. A kernel that rejected
//! everything would hold the invariant perfectly and be useless, so this says how much
//! the guarantee costs. It is pinned so a change to it has to be looked at, and the
//! architecture document names it as the number that decides whether the approach works.
//!
//! This is a fixture written by the people who wrote the kernel, so the ratio says what
//! it does on sentences like these. It is not a measurement on real questions, which
//! needs the ask loop and a real model. See `docs/design/correctness-architecture.md`.
//!
//! Every proposal has the shape `capsulet-proposer` gives an answer the model calls a
//! direct quote: `Trust { Cite(goal), min_authority: "low" }`.

use capsulet_core::{
    Authority, Evidence, EvidenceId, EvidenceSpan, MemoryScope, Source, SourceContent, SourceId,
};
use capsulet_kernel::{
    NameGroup, Phrasebook, Phrasing, Proposal, Proposition, Rule, Snapshot, ValueType, Verdict,
    check,
};

/// What a reader says the sentence does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reader {
    Stated,
    NotStated,
}
use Reader::{NotStated, Stated};

struct Case {
    sentence: &'static str,
    subject: &'static str,
    predicate: &'static str,
    object: &'static str,
    reader: Reader,
}

const fn case(
    sentence: &'static str,
    subject: &'static str,
    predicate: &'static str,
    object: &'static str,
    reader: Reader,
) -> Case {
    Case {
        sentence,
        subject,
        predicate,
        object,
        reader,
    }
}

/// The corpus. Rows 1 to 11 of the plan's evidence table come first, because they are
/// what this work set out to change.
const CORPUS: &[Case] = &[
    // The plan's table.
    case(
        "Fabrikam did not acquire Contoso.",
        "Fabrikam",
        "acquired",
        "Contoso",
        NotStated,
    ),
    case(
        "Contoso acquired Fabrikam in 2024.",
        "Fabrikam",
        "acquired",
        "Contoso",
        NotStated,
    ),
    case(
        "Contoso sued Fabrikam over a patent.",
        "Contoso",
        "was_acquired_by",
        "Fabrikam",
        NotStated,
    ),
    case(
        "Fabrikam may acquire Contoso next year.",
        "Fabrikam",
        "acquired",
        "Contoso",
        NotStated,
    ),
    case(
        "The CEO announced the annual results.",
        "CEO",
        "is_named",
        "Ann",
        NotStated,
    ),
    case(
        "The notice period is 300 days.",
        "notice period",
        "in_days",
        "30",
        NotStated,
    ),
    case(
        "Revenue rose to $1.2 million in 2025.",
        "Revenue",
        "rose_to",
        "USD 1200000",
        Stated,
    ),
    case(
        "The contract was signed on March 1, 2026.",
        "contract",
        "signed_on",
        "2026-03-01",
        Stated,
    ),
    case(
        "IBM acquired Red Hat in 2019.",
        "International Business Machines",
        "acquired",
        "Red Hat",
        Stated,
    ),
    case(
        "Acme’s renewal was approved.",
        "Acme's renewal",
        "status",
        "approved",
        Stated,
    ),
    case(
        "Acme is located at Hauptstraße 5.",
        "Acme",
        "located_at",
        "HAUPTSTRASSE 5",
        Stated,
    ),
    // Stated plainly.
    case(
        "Fabrikam acquired Contoso in 2024.",
        "Fabrikam",
        "acquired",
        "Contoso",
        Stated,
    ),
    case(
        "Contoso was acquired by Fabrikam.",
        "Fabrikam",
        "acquired",
        "Contoso",
        Stated,
    ),
    case(
        "Last year, Fabrikam acquired Contoso for cash.",
        "Fabrikam",
        "acquired",
        "Contoso",
        Stated,
    ),
    case(
        "FABRIKAM ACQUIRED CONTOSO.",
        "Fabrikam",
        "acquired",
        "Contoso",
        Stated,
    ),
    case(
        "The notice period is 30 days.",
        "notice period",
        "in_days",
        "30",
        Stated,
    ),
    case(
        "Revenue rose to $1,200,000 in 2025.",
        "Revenue",
        "rose_to",
        "USD 1200000",
        Stated,
    ),
    case(
        "The contract was signed on 2026-03-01.",
        "contract",
        "signed_on",
        "2026-03-01",
        Stated,
    ),
    case(
        "The contract was signed on 1 March 2026.",
        "contract",
        "signed_on",
        "2026-03-01",
        Stated,
    ),
    // Stated, in a way the phrasing does not cover. These are what the ratio costs.
    case(
        "Fabrikam's acquisition of Contoso closed in May.",
        "Fabrikam",
        "acquired",
        "Contoso",
        Stated,
    ),
    case(
        "Contoso now belongs to Fabrikam.",
        "Fabrikam",
        "acquired",
        "Contoso",
        Stated,
    ),
    // Not stated: denied, reversed, hedged, asked, or about something else.
    case(
        "Fabrikam never acquired Contoso.",
        "Fabrikam",
        "acquired",
        "Contoso",
        NotStated,
    ),
    case(
        "It is false that Fabrikam acquired Contoso.",
        "Fabrikam",
        "acquired",
        "Contoso",
        NotStated,
    ),
    case(
        "Fabrikam could have acquired Contoso.",
        "Fabrikam",
        "acquired",
        "Contoso",
        NotStated,
    ),
    case(
        "If Fabrikam acquired Contoso, the deal is void.",
        "Fabrikam",
        "acquired",
        "Contoso",
        NotStated,
    ),
    case(
        "Did Fabrikam acquire Contoso?",
        "Fabrikam",
        "acquired",
        "Contoso",
        NotStated,
    ),
    case(
        "Did Fabrikam acquired Contoso?",
        "Fabrikam",
        "acquired",
        "Contoso",
        NotStated,
    ),
    case(
        "Reportedly, Fabrikam acquired Contoso.",
        "Fabrikam",
        "acquired",
        "Contoso",
        NotStated,
    ),
    case(
        "Rumours that Fabrikam acquired Contoso spread.",
        "Fabrikam",
        "acquired",
        "Contoso",
        NotStated,
    ),
    case(
        "Fabrikam partnered with Contoso.",
        "Fabrikam",
        "acquired",
        "Contoso",
        NotStated,
    ),
    case(
        "Fabrikam acquired Northwind. Contoso filed a lawsuit.",
        "Fabrikam",
        "acquired",
        "Contoso",
        NotStated,
    ),
    // Not stated: terms that only look present.
    case(
        "Fabrikamia acquired Contoso.",
        "Fabrikam",
        "acquired",
        "Contoso",
        NotStated,
    ),
    case(
        "Fabrikam acquired Contosoville.",
        "Fabrikam",
        "acquired",
        "Contoso",
        NotStated,
    ),
    case(
        "Revenue rose to $1,200,001 in 2025.",
        "Revenue",
        "rose_to",
        "USD 1200000",
        NotStated,
    ),
    case(
        "Revenue rose to $1.2 billion in 2025.",
        "Revenue",
        "rose_to",
        "USD 1200000",
        NotStated,
    ),
    case(
        "The contract was signed on March 2, 2026.",
        "contract",
        "signed_on",
        "2026-03-01",
        NotStated,
    ),
    case(
        "The contract was signed in March 2026.",
        "contract",
        "signed_on",
        "2026-03-01",
        NotStated,
    ),
    // A name nobody reviewed.
    case(
        "Big Blue acquired Red Hat in 2019.",
        "International Business Machines",
        "acquired",
        "Red Hat",
        NotStated,
    ),
];

/// A case a reader marks not stated that the kernel accepts, and why that is allowed
/// to be true.
struct RecordedLimitation {
    sentence: &'static str,
    claim: &'static str,
}

/// Cue lists are finite. This sentence doubts the phrasing in a word no list carries,
/// so it states the relation as far as the kernel can tell. Listed so the invariant
/// names the exception out loud, rather than the corpus quietly leaving it out.
const RECORDED_LIMITATIONS: &[RecordedLimitation] = &[RecordedLimitation {
    sentence: "Analysts doubted that Fabrikam acquired Contoso.",
    claim: "CAP-CORRECTNESS-012",
}];

fn phrasebook() -> Phrasebook {
    let phrasing = |template: &str| Phrasing::parse(template).expect("a phrasing");
    Phrasebook::new()
        .with_relation(
            "acquired",
            vec![
                phrasing("{from} acquired {to}"),
                phrasing("{to} was acquired by {from}"),
            ],
        )
        .with_relation(
            "was_acquired_by",
            vec![phrasing("{from} was acquired by {to}")],
        )
        .with_relation("is_named", vec![phrasing("{from} is {to}")])
        .with_relation("in_days", vec![phrasing("{from} is {to} days")])
        .with_relation("status", vec![phrasing("{from} was {to}")])
        .with_relation("located_at", vec![phrasing("{from} is located at {to}")])
        .with_relation("rose_to", vec![phrasing("{from} rose to {to}")])
        .with_relation("signed_on", vec![phrasing("{from} was signed on {to}")])
        .with_object_type("in_days", ValueType::parse("number").expect("a type"))
        .with_object_type("rose_to", ValueType::parse("money").expect("a type"))
        .with_object_type("signed_on", ValueType::parse("date").expect("a type"))
        .with_name_groups(vec![
            NameGroup::new(
                [
                    "International Business Machines".to_string(),
                    "IBM".to_string(),
                ],
                ["res_ibm".to_string()],
            )
            .expect("a group"),
        ])
        .with_value_policy(Some("USD"), None)
        .expect("a valid policy")
}

fn verdict_of(sentence: &str, subject: &str, predicate: &str, object: &str) -> Verdict {
    let source = SourceId::new("src").expect("id");
    let scope = MemoryScope::new("acme", "prod").expect("scope");
    let content = SourceContent::new(source.clone(), sentence).expect("content");
    let evidence = Evidence::new(
        EvidenceId::new("ev").expect("id"),
        scope.clone(),
        source.clone(),
        "para",
        sentence,
        "2026-03-02T00:00:00Z",
    )
    .expect("evidence")
    .with_span(EvidenceSpan::new(0, sentence.len(), content.content_hash()).expect("span"));
    let snapshot = Snapshot::new()
        .with_phrasebook(phrasebook())
        .with_source(
            Source::new(source, scope, "document", None, "Memo", Authority::High).expect("source"),
        )
        .with_source_content(content)
        .with_evidence(evidence);

    let goal = Proposition::new(subject, predicate, object);
    let proposal = Proposal {
        schema_version: Proposal::current_schema_version(),
        question_reading: None,
        goal: goal.clone(),
        derivation: Rule::Trust {
            premise: Box::new(Rule::Cite {
                evidence_id: "ev".to_string(),
                proposition: goal,
            }),
            min_authority: "low".to_string(),
        },
    };
    check(&proposal, &snapshot).verdict
}

#[test]
fn no_case_a_reader_marks_not_stated_reaches_accepted() {
    let breaches: Vec<&str> = CORPUS
        .iter()
        .filter(|case| case.reader == NotStated)
        .filter(|case| {
            verdict_of(case.sentence, case.subject, case.predicate, case.object)
                == Verdict::Accepted
        })
        .map(|case| case.sentence)
        .collect();
    assert!(
        breaches.is_empty(),
        "these were accepted and a reader says they do not state it: {breaches:#?}"
    );
}

#[test]
fn every_recorded_limitation_is_still_the_limitation_it_says_it_is() {
    for limitation in RECORDED_LIMITATIONS {
        assert_eq!(
            verdict_of(limitation.sentence, "Fabrikam", "acquired", "Contoso"),
            Verdict::Accepted,
            "if this changes, restate {} and move the case into the corpus",
            limitation.claim
        );
    }
}

#[test]
fn a_doubt_the_cue_list_does_not_know_still_states_the_relation() {
    // The named evidence for CAP-CORRECTNESS-012. A known cost of finite cue lists.
    assert_eq!(
        verdict_of(
            "Analysts doubted that Fabrikam acquired Contoso.",
            "Fabrikam",
            "acquired",
            "Contoso"
        ),
        Verdict::Accepted,
    );
}

#[test]
fn the_share_of_stated_cases_that_reach_accepted_is_what_was_recorded() {
    let stated: Vec<&Case> = CORPUS.iter().filter(|case| case.reader == Stated).collect();
    let missed: Vec<&str> = stated
        .iter()
        .filter(|case| {
            verdict_of(case.sentence, case.subject, case.predicate, case.object)
                != Verdict::Accepted
        })
        .map(|case| case.sentence)
        .collect();

    // Pinned, so moving it means looking at it and updating the architecture document.
    // Each miss is a sentence that states the fact in words no phrasing declares, and a
    // contract that declared them would change that.
    assert_eq!(
        (stated.len() - missed.len(), stated.len()),
        (ACCEPTED_STATED, STATED_TOTAL),
        "missed: {missed:#?}"
    );
    assert_eq!(
        missed,
        [
            "Fabrikam's acquisition of Contoso closed in May.",
            "Contoso now belongs to Fabrikam.",
        ]
    );
}

const ACCEPTED_STATED: usize = 13;
const STATED_TOTAL: usize = 15;
