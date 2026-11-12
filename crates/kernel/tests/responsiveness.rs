//! Whether an answer answers the question it was asked.
//!
//! A grounded, accurately cited claim can still be beside the point. These tests
//! pin what the kernel holds an answer to once a question is pinned: the goal
//! against a typed query, mechanically; the reading of a question asked in words,
//! as a residual; and nothing at all when no question was pinned, which decides
//! exactly as before.

use capsulet_core::{
    Authority, Evidence, EvidenceId, EvidenceSpan, MemoryScope, Source, SourceContent, SourceId,
};
use capsulet_kernel::{
    Certificate, NameGroup, Phrasebook, Phrasing, Proposal, Proposition, Query, Question,
    QuestionReading, ResidualKind, Rule, Slot, Snapshot, SnapshotDocument, ValueType, Verdict,
    check,
};

const DOCUMENT: &str = "Fabrikam acquired Contoso in 2024. Dana Whitfield owns the Contoso \
                        account. Acme paid 1,000 dollars.";

fn phrasebook() -> Phrasebook {
    let phrasing = |template: &str| Phrasing::parse(template).expect("phrasing");
    Phrasebook::new()
        .with_relation("acquired", vec![phrasing("{from} acquired {to}")])
        .with_relation(
            "owns_account",
            vec![phrasing("{from} owns the {to} account")],
        )
        .with_relation("paid", vec![phrasing("{from} paid {to}")])
        .with_object_type("paid", ValueType::Number)
}

fn snapshot() -> Snapshot {
    let scope = MemoryScope::new("acme", "prod").expect("scope");
    let source_id = SourceId::new("src_1").expect("source id");
    let content = SourceContent::new(source_id.clone(), DOCUMENT).expect("content");
    let evidence = Evidence::new(
        EvidenceId::new("ev_1").expect("evidence id"),
        scope.clone(),
        source_id.clone(),
        "para-1",
        DOCUMENT,
        "2026-11-13T00:00:00Z",
    )
    .expect("evidence")
    .with_span(EvidenceSpan::new(0, DOCUMENT.len(), content.content_hash()).expect("span"));
    Snapshot::new()
        .with_phrasebook(phrasebook())
        .with_source(
            Source::new(
                source_id,
                scope,
                "document",
                None,
                "Deal memo",
                Authority::High,
            )
            .expect("source"),
        )
        .with_source_content(content)
        .with_evidence(evidence)
}

/// A trusted citation of `subject relation object` from the whole document.
fn answer(subject: &str, relation: &str, object: &str) -> Proposal {
    let goal = Proposition::new(subject, relation, object);
    Proposal {
        schema_version: Proposal::current_schema_version(),
        question_reading: None,
        goal: goal.clone(),
        derivation: Rule::Trust {
            premise: Box::new(Rule::Cite {
                evidence_id: "ev_1".to_string(),
                proposition: goal,
            }),
            min_authority: "low".to_string(),
        },
    }
}

fn reading(mut proposal: Proposal, query: Query, rationale: &str) -> Proposal {
    proposal.question_reading = Some(QuestionReading {
        query,
        rationale: rationale.to_string(),
    });
    proposal
}

fn who_acquired_contoso() -> Query {
    Query::new("acquired", Slot::Wanted, Slot::bound("Contoso"))
}

fn codes(certificate: &Certificate) -> Vec<&str> {
    certificate
        .errors
        .iter()
        .map(|error| error.code.as_str())
        .collect()
}

#[test]
fn a_grounded_fact_that_answers_nothing_is_rejected_once_the_question_is_pinned() {
    let owner = answer("Dana Whitfield", "owns_account", "Contoso");
    // Unchanged without a question: a certificate about a claim, which is all it
    // ever said.
    assert_eq!(check(&owner, &snapshot()).verdict, Verdict::Accepted);

    let asked = snapshot().with_question(
        Question::asked("Which company acquired Contoso?").with_query(who_acquired_contoso()),
    );
    let certificate = check(&owner, &asked);
    assert_eq!(certificate.verdict, Verdict::Rejected);
    assert_eq!(codes(&certificate), ["goal_not_responsive"]);
    assert_eq!(certificate.errors[0].repair_owner, "proposer");
    assert!(
        certificate.errors[0]
            .message
            .contains("acquired(?, Contoso)"),
        "{}",
        certificate.errors[0].message
    );
}

#[test]
fn an_answer_that_fills_the_pinned_query_is_accepted_and_says_so() {
    let asked = snapshot().with_question(
        Question::asked("Which company acquired Contoso?").with_query(who_acquired_contoso()),
    );
    let certificate = check(&answer("Fabrikam", "acquired", "Contoso"), &asked);
    assert_eq!(certificate.verdict, Verdict::Accepted, "{certificate:?}");
    let step = certificate
        .discharged
        .iter()
        .find(|step| step.rule == "answer")
        .expect("an answer step");
    assert!(
        step.detail.contains("acquired(?, Contoso)"),
        "{}",
        step.detail
    );
    assert!(step.detail.contains("pinned"), "{}", step.detail);
}

#[test]
fn a_question_asked_in_words_leaves_the_proposers_reading_of_it_to_review() {
    let asked = snapshot().with_question(Question::asked("Which company acquired Contoso?"));
    let honest = reading(
        answer("Fabrikam", "acquired", "Contoso"),
        who_acquired_contoso(),
        "asks for the acquirer of Contoso",
    );
    let certificate = check(&honest, &asked);
    assert_eq!(certificate.verdict, Verdict::Conditional, "{certificate:?}");
    let [residual] = certificate.residuals.as_slice() else {
        panic!("one residual: {:?}", certificate.residuals);
    };
    assert_eq!(residual.kind, ResidualKind::Question);
    assert_eq!(residual.from, "asked(\"Which company acquired Contoso?\")");
    assert!(residual.rationale.contains("acquired(?, Contoso)"));
    assert!(residual.rationale.contains("asks for the acquirer"));

    // Misreading the question to fit the answer is not hidden: the goal fills the
    // reading, and the reading is exactly what the reviewer is shown.
    let convenient = reading(
        answer("Dana Whitfield", "owns_account", "Contoso"),
        Query::new("owns_account", Slot::Wanted, Slot::bound("Contoso")),
        "asks who holds Contoso",
    );
    let certificate = check(&convenient, &asked);
    assert_eq!(certificate.verdict, Verdict::Conditional);
    assert!(
        certificate.residuals[0]
            .rationale
            .contains("owns_account(?, Contoso)")
    );

    // And an answer that does not fit even its own reading is rejected.
    let mismatched = reading(
        answer("Dana Whitfield", "owns_account", "Contoso"),
        who_acquired_contoso(),
        "asks for the acquirer of Contoso",
    );
    assert_eq!(codes(&check(&mismatched, &asked)), ["goal_not_responsive"]);
}

#[test]
fn an_answer_to_a_question_in_words_must_say_how_it_read_it_and_why() {
    let asked = snapshot().with_question(Question::asked("Which company acquired Contoso?"));
    let unread = answer("Fabrikam", "acquired", "Contoso");
    let certificate = check(&unread, &asked);
    assert_eq!(certificate.verdict, Verdict::Rejected);
    assert_eq!(codes(&certificate), ["question_not_read"]);

    let unexplained = reading(unread, who_acquired_contoso(), "   ");
    assert_eq!(
        codes(&check(&unexplained, &asked)),
        ["question_reading_without_a_rationale"]
    );
}

#[test]
fn the_callers_query_is_what_counts_whatever_the_proposer_read() {
    let asked = snapshot().with_question(
        Question::asked("Which company acquired Contoso?").with_query(who_acquired_contoso()),
    );
    let proposal = reading(
        answer("Dana Whitfield", "owns_account", "Contoso"),
        Query::new("owns_account", Slot::Wanted, Slot::bound("Contoso")),
        "asks who holds Contoso",
    );
    let certificate = check(&proposal, &asked);
    assert_eq!(codes(&certificate), ["goal_not_responsive"]);
    assert!(
        certificate.residuals.is_empty(),
        "{:?}",
        certificate.residuals
    );
}

#[test]
fn a_bound_term_matches_under_a_confirmed_name_and_says_which() {
    let query = Query::new("acquired", Slot::Wanted, Slot::bound("Contoso Ltd"));
    let question = Question::asked("Who acquired Contoso Ltd?").with_query(query);
    let unreviewed = snapshot().with_question(question.clone());
    assert_eq!(
        codes(&check(
            &answer("Fabrikam", "acquired", "Contoso"),
            &unreviewed
        )),
        ["goal_not_responsive"]
    );

    let group = NameGroup::new(
        ["Contoso".to_string(), "Contoso Ltd".to_string()],
        ["res_contoso".to_string()],
    )
    .expect("group");
    let reviewed = snapshot()
        .with_phrasebook(phrasebook().with_name_groups(vec![group]))
        .with_question(question);
    let certificate = check(&answer("Fabrikam", "acquired", "Contoso"), &reviewed);
    assert_eq!(certificate.verdict, Verdict::Accepted, "{certificate:?}");
    let step = certificate
        .discharged
        .iter()
        .find(|step| step.rule == "answer")
        .expect("an answer step");
    assert!(step.detail.contains("res_contoso"), "{}", step.detail);
}

#[test]
fn a_bound_typed_object_matches_by_exact_value() {
    let question = |amount: &str| {
        Question::asked("Did Acme pay that much?").with_query(Query::new(
            "paid",
            Slot::bound("Acme"),
            Slot::bound(amount),
        ))
    };
    let paid = answer("Acme", "paid", "1000");
    assert_eq!(
        check(&paid, &snapshot().with_question(question("1,000"))).verdict,
        Verdict::Accepted
    );
    assert_eq!(
        codes(&check(&paid, &snapshot().with_question(question("1,001")))),
        ["goal_not_responsive"]
    );
}

#[test]
fn a_pinned_query_that_asks_nothing_is_refused_as_policy() {
    let asked =
        snapshot().with_question(Question::asked("Who acquired whom?").with_query(Query::new(
            "acquired",
            Slot::Wanted,
            Slot::Wanted,
        )));
    let certificate = check(&answer("Fabrikam", "acquired", "Contoso"), &asked);
    assert_eq!(codes(&certificate), ["query_malformed"]);
    assert_eq!(certificate.errors[0].repair_owner, "policy");
}

#[test]
fn a_proposal_that_reads_no_question_encodes_as_it_always_did() {
    let encoded = serde_json::to_value(answer("Fabrikam", "acquired", "Contoso")).expect("encodes");
    assert!(encoded.get("question_reading").is_none(), "{encoded}");
    let certificate = check(&answer("Fabrikam", "acquired", "Contoso"), &snapshot());
    let encoded = serde_json::to_string(&certificate).expect("encodes");
    assert!(!encoded.contains("\"kind\""), "{encoded}");
}

#[test]
fn a_pinned_question_travels_in_the_snapshot_document_and_a_hollow_one_is_refused() {
    let json = r#"{"sources":[],"evidence":[],"claims":[],
        "question":{"text":"Which company acquired Contoso?",
                    "query":{"relation":"acquired","subject":{"slot":"wanted"},
                             "object":{"slot":"bound","term":"Contoso"}}}}"#;
    let document: SnapshotDocument = serde_json::from_str(json).expect("document");
    let rebuilt = document.rebuild().expect("rebuilds");
    assert_eq!(
        rebuilt
            .question()
            .and_then(|question| question.query.clone()),
        Some(who_acquired_contoso())
    );
    let reencoded: serde_json::Value = serde_json::to_value(&document).expect("encodes");
    assert_eq!(reencoded["question"]["query"]["subject"]["slot"], "wanted");

    let hollow = json.replace(
        r#"{"slot":"bound","term":"Contoso"}"#,
        r#"{"slot":"wanted"}"#,
    );
    let document: SnapshotDocument = serde_json::from_str(&hollow).expect("document");
    assert!(document.rebuild().is_err());
}
