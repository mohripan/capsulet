use capsulet_core::{
    Authority, Claim, ClaimId, ClaimStatus, Confidence, EntityId, Evidence, EvidenceId,
    EvidenceSpan, MemoryScope, Source, SourceContent, SourceId,
};

use crate::{ArithOp, Proposal, Proposition, Rule, Snapshot, Verdict, check, error::RepairOwner};

const DOC: &str = "Acme renewed the Contoso contract on 2026-03-01. Notice is 30 days.";

fn scope() -> MemoryScope {
    MemoryScope::new("acme", "prod").expect("scope")
}

fn source(authority: Authority) -> Source {
    Source::new(
        SourceId::new("src_1").expect("source id"),
        scope(),
        "document",
        None,
        "Renewal memo",
        authority,
    )
    .expect("source")
}

fn content() -> SourceContent {
    SourceContent::new(SourceId::new("src_1").expect("source id"), DOC).expect("content")
}

/// Evidence quoting bytes `start..end` of the document, verbatim.
fn honest_evidence(start: usize, end: usize) -> Evidence {
    let content = content();
    Evidence::new(
        EvidenceId::new("ev_1").expect("evidence id"),
        scope(),
        SourceId::new("src_1").expect("source id"),
        "para-1",
        &DOC[start..end],
        "2026-03-02T00:00:00Z",
    )
    .expect("evidence")
    .with_span(EvidenceSpan::new(start, end, content.content_hash()).expect("span"))
}

/// Evidence whose excerpt is not what the span actually says: a fabricated quote.
fn fabricated_evidence(excerpt: &str, start: usize, end: usize) -> Evidence {
    let content = content();
    Evidence::new(
        EvidenceId::new("ev_1").expect("evidence id"),
        scope(),
        SourceId::new("src_1").expect("source id"),
        "para-1",
        excerpt,
        "2026-03-02T00:00:00Z",
    )
    .expect("evidence")
    .with_span(EvidenceSpan::new(start, end, content.content_hash()).expect("span"))
}

fn base_snapshot(evidence: Evidence, authority: Authority) -> Snapshot {
    Snapshot::new()
        .with_source(source(authority))
        .with_source_content(content())
        .with_evidence(evidence)
}

fn claim_with_status(status: ClaimStatus) -> Claim {
    Claim::new(
        ClaimId::new("claim_1").expect("claim id"),
        scope(),
        EntityId::new("entity_acme").expect("entity id"),
        "renewed",
        "the Contoso contract",
        vec![EvidenceId::new("ev_1").expect("evidence id")],
        Confidence::new(0.9).expect("confidence"),
        Authority::High,
        "2026-03-02T00:00:00Z",
        None,
        None,
    )
    .expect("claim")
    .with_status(status)
}

#[test]
fn accepts_a_citation_that_re_derives_and_contains_its_object() {
    let evidence = honest_evidence(0, 46);
    let snapshot = base_snapshot(evidence, Authority::High);
    let goal = Proposition::new("Acme", "renewed", "the Contoso contract");
    let proposal = Proposal {
        goal: goal.clone(),
        derivation: Rule::Trust {
            premise: Box::new(Rule::Cite {
                evidence_id: "ev_1".to_string(),
                proposition: goal,
            }),
            min_authority: "medium".to_string(),
        },
    };

    let certificate = check(&proposal, &snapshot);

    assert_eq!(certificate.verdict, Verdict::Accepted);
    assert!(certificate.residuals.is_empty());
    assert_eq!(certificate.discharged.len(), 2);
    assert!(!certificate.replay_digest.is_empty());
}

#[test]
fn rejects_a_fabricated_quotation() {
    // The span is real, but the excerpt claims the document says the opposite.
    let evidence = fabricated_evidence("Acme terminated the Contoso contract", 0, 46);
    let snapshot = base_snapshot(evidence, Authority::High);
    let goal = Proposition::new("Acme", "terminated", "the Contoso contract");
    let proposal = Proposal {
        goal: goal.clone(),
        derivation: Rule::Cite {
            evidence_id: "ev_1".to_string(),
            proposition: goal,
        },
    };

    let certificate = check(&proposal, &snapshot);

    assert_eq!(certificate.verdict, Verdict::Rejected);
    assert_eq!(certificate.errors[0].code, "provenance_failed");
    assert_eq!(certificate.errors[0].repair_owner, "ingestion");
}

#[test]
fn rejects_a_triple_whose_subject_is_absent_from_the_cited_span() {
    // Regression: a live run against qwen2.5:1.5b answered "which company
    // acquired Contoso?" by attaching the subject "Contoso" to a sentence about
    // anniversary dates, purely because the object string appeared there.
    // Grounding only the object accepted it; grounding both endpoints does not.
    let evidence = honest_evidence(48, 66);
    let snapshot = base_snapshot(evidence, Authority::High);
    let goal = Proposition::new("Contoso", "acquired_by", "30 days");
    let proposal = Proposal {
        goal: goal.clone(),
        derivation: Rule::Cite {
            evidence_id: "ev_1".to_string(),
            proposition: goal,
        },
    };

    let certificate = check(&proposal, &snapshot);

    assert_eq!(certificate.verdict, Verdict::Rejected);
    assert_eq!(certificate.errors[0].code, "term_not_in_span");
    assert!(certificate.errors[0].message.contains("subject"));
}

#[test]
fn rejects_an_object_the_cited_span_does_not_contain() {
    // Honest quotation, but the proposer read something into it that is not there.
    let evidence = honest_evidence(0, 46);
    let snapshot = base_snapshot(evidence, Authority::High);
    let goal = Proposition::new("Acme", "acquired", "Contoso Ltd");
    let proposal = Proposal {
        goal: goal.clone(),
        derivation: Rule::Cite {
            evidence_id: "ev_1".to_string(),
            proposition: goal,
        },
    };

    let certificate = check(&proposal, &snapshot);

    assert_eq!(certificate.verdict, Verdict::Rejected);
    assert_eq!(certificate.errors[0].code, "term_not_in_span");
    assert_eq!(certificate.errors[0].repair_owner, "proposer");
}

#[test]
fn rejects_a_citation_to_evidence_that_is_not_in_the_snapshot() {
    let snapshot = Snapshot::new().with_source(source(Authority::High));
    let goal = Proposition::new("Acme", "renewed", "the Contoso contract");
    let proposal = Proposal {
        goal: goal.clone(),
        derivation: Rule::Cite {
            evidence_id: "ev_missing".to_string(),
            proposition: goal,
        },
    };

    let certificate = check(&proposal, &snapshot);

    assert_eq!(certificate.verdict, Verdict::Rejected);
    assert_eq!(certificate.errors[0].code, "dangling_evidence");
    // Retrieval failed, not the model: re-running the retriever costs no tokens.
    assert_eq!(certificate.errors[0].repair_owner, "retrieval");
}

#[test]
fn returns_conditional_when_the_derivation_needed_a_reading() {
    let evidence = honest_evidence(48, 66);
    let snapshot = base_snapshot(evidence, Authority::High);
    let goal = Proposition::new("Contoso contract", "notice_period_days", "30");
    let proposal = Proposal {
        goal: goal.clone(),
        derivation: Rule::Interpret {
            premise: Box::new(Rule::Cite {
                evidence_id: "ev_1".to_string(),
                proposition: Proposition::new("Notice", "text", "30 days"),
            }),
            proposition: goal,
            rationale: "\"Notice is 30 days\" states the contract's notice period".to_string(),
        },
    };

    let certificate = check(&proposal, &snapshot);

    assert_eq!(certificate.verdict, Verdict::Conditional);
    assert_eq!(certificate.residuals.len(), 1);
    assert_eq!(certificate.residuals[0].evidence_ids, vec!["ev_1"]);
    assert!(certificate.errors.is_empty());
}

#[test]
fn a_reading_of_a_fabricated_citation_is_still_rejected() {
    // Interpretation must not launder a bad premise into a conditional pass.
    let evidence = fabricated_evidence("Notice is 90 days", 48, 66);
    let snapshot = base_snapshot(evidence, Authority::High);
    let goal = Proposition::new("Contoso contract", "notice_period_days", "90");
    let proposal = Proposal {
        goal: goal.clone(),
        derivation: Rule::Interpret {
            premise: Box::new(Rule::Cite {
                evidence_id: "ev_1".to_string(),
                proposition: Proposition::new("Notice", "text", "90 days"),
            }),
            proposition: goal,
            rationale: "the memo sets a 90 day notice period".to_string(),
        },
    };

    let certificate = check(&proposal, &snapshot);

    assert_eq!(certificate.verdict, Verdict::Rejected);
    assert!(certificate.residuals.is_empty());
}

#[test]
fn recomputes_arithmetic_and_reports_the_correct_value() {
    let snapshot = Snapshot::new();
    let goal = Proposition::new("Q1", "total_contract_value", "47");
    let proposal = Proposal {
        goal: goal.clone(),
        derivation: Rule::Arith {
            op: ArithOp::Sum,
            operands: vec![20.0, 26.0],
            claimed: 47.0,
            proposition: goal,
        },
    };

    let certificate = check(&proposal, &snapshot);

    assert_eq!(certificate.verdict, Verdict::Rejected);
    assert_eq!(certificate.errors[0].code, "arith_mismatch");
    // The kernel already knows the answer, so no model call is needed to fix it.
    assert_eq!(certificate.errors[0].corrected_value, Some(46.0));
    assert!(certificate.is_auto_repairable());
}

#[test]
fn accepts_arithmetic_that_recomputes() {
    let snapshot = Snapshot::new();
    let goal = Proposition::new("Q1", "total_contract_value", "46");
    let proposal = Proposal {
        goal: goal.clone(),
        derivation: Rule::Arith {
            op: ArithOp::Sum,
            operands: vec![20.0, 26.0],
            claimed: 46.0,
            proposition: goal,
        },
    };

    assert_eq!(check(&proposal, &snapshot).verdict, Verdict::Accepted);
}

#[test]
fn refuses_to_trust_a_source_below_the_authority_floor() {
    let evidence = honest_evidence(0, 46);
    let snapshot = base_snapshot(evidence, Authority::Low);
    let goal = Proposition::new("Acme", "renewed", "the Contoso contract");
    let proposal = Proposal {
        goal: goal.clone(),
        derivation: Rule::Trust {
            premise: Box::new(Rule::Cite {
                evidence_id: "ev_1".to_string(),
                proposition: goal,
            }),
            min_authority: "high".to_string(),
        },
    };

    let certificate = check(&proposal, &snapshot);

    assert_eq!(certificate.verdict, Verdict::Rejected);
    assert_eq!(certificate.errors[0].code, "authority_below_floor");
    assert_eq!(certificate.errors[0].repair_owner, "policy");
}

#[test]
fn refuses_to_attest_a_claim_that_is_not_active() {
    let snapshot = Snapshot::new().with_claim(claim_with_status(ClaimStatus::Candidate));
    let goal = Proposition::new("entity_acme", "renewed", "the Contoso contract");
    let proposal = Proposal {
        goal,
        derivation: Rule::Attest {
            claim_id: "claim_1".to_string(),
        },
    };

    let certificate = check(&proposal, &snapshot);

    assert_eq!(certificate.verdict, Verdict::Rejected);
    assert_eq!(certificate.errors[0].code, "claim_not_active");
    assert_eq!(certificate.errors[0].repair_owner, "memory");
}

#[test]
fn attests_an_active_claim() {
    let snapshot = Snapshot::new().with_claim(claim_with_status(ClaimStatus::Active));
    let goal = Proposition::new("entity_acme", "renewed", "the Contoso contract");
    let proposal = Proposal {
        goal,
        derivation: Rule::Attest {
            claim_id: "claim_1".to_string(),
        },
    };

    assert_eq!(check(&proposal, &snapshot).verdict, Verdict::Accepted);
}

#[test]
fn rejects_a_derivation_that_proves_something_other_than_the_goal() {
    let evidence = honest_evidence(0, 46);
    let snapshot = base_snapshot(evidence, Authority::High);
    let proposal = Proposal {
        goal: Proposition::new("Acme", "renewed", "the Fabrikam contract"),
        derivation: Rule::Trust {
            premise: Box::new(Rule::Cite {
                evidence_id: "ev_1".to_string(),
                proposition: Proposition::new("Acme", "renewed", "the Contoso contract"),
            }),
            min_authority: "low".to_string(),
        },
    };

    let certificate = check(&proposal, &snapshot);

    assert_eq!(certificate.verdict, Verdict::Rejected);
    assert_eq!(certificate.errors[0].code, "goal_not_derived");
}

#[test]
fn tolerates_reflowed_whitespace_and_case_when_matching_the_object() {
    let evidence = honest_evidence(0, 46);
    let snapshot = base_snapshot(evidence, Authority::High);
    let goal = Proposition::new("Acme", "renewed", "The   Contoso\n Contract");
    let proposal = Proposal {
        goal: goal.clone(),
        derivation: Rule::Cite {
            evidence_id: "ev_1".to_string(),
            proposition: goal,
        },
    };

    assert!(!check(&proposal, &snapshot).is_rejected());
}

#[test]
fn a_stale_content_digest_rejects_rather_than_silently_repointing() {
    let content = content();
    let evidence = Evidence::new(
        EvidenceId::new("ev_1").expect("evidence id"),
        scope(),
        SourceId::new("src_1").expect("source id"),
        "para-1",
        "Acme renewed the Contoso contract",
        "2026-03-02T00:00:00Z",
    )
    .expect("evidence")
    .with_span(EvidenceSpan::new(0, 33, "digest-of-an-older-version").expect("span"));
    let snapshot = Snapshot::new()
        .with_source(source(Authority::High))
        .with_source_content(content)
        .with_evidence(evidence);
    let goal = Proposition::new("Acme", "renewed", "the Contoso contract");
    let proposal = Proposal {
        goal: goal.clone(),
        derivation: Rule::Cite {
            evidence_id: "ev_1".to_string(),
            proposition: goal,
        },
    };

    let certificate = check(&proposal, &snapshot);

    assert_eq!(certificate.verdict, Verdict::Rejected);
    assert_eq!(certificate.errors[0].code, "source_content_missing");
}

#[test]
fn every_error_routes_to_a_repair_owner() {
    // The taxonomy is only useful if no failure falls through to "just retry".
    let evidence = honest_evidence(0, 46);
    let snapshot = base_snapshot(evidence, Authority::High);
    let goal = Proposition::new("Acme", "acquired", "Contoso Ltd");
    let proposal = Proposal {
        goal: goal.clone(),
        derivation: Rule::Cite {
            evidence_id: "ev_1".to_string(),
            proposition: goal,
        },
    };

    let certificate = check(&proposal, &snapshot);

    assert!(!certificate.errors.is_empty());
    for error in &certificate.errors {
        assert!(
            [
                RepairOwner::Retrieval,
                RepairOwner::Ingestion,
                RepairOwner::Memory,
                RepairOwner::AutoRepairable,
                RepairOwner::Policy,
                RepairOwner::Proposer,
            ]
            .iter()
            .any(|owner| owner.as_str() == error.repair_owner),
            "unrouted error: {error:?}"
        );
    }
}

#[test]
fn the_replay_digest_is_stable_for_the_same_proposal() {
    let evidence = honest_evidence(0, 46);
    let snapshot = base_snapshot(evidence, Authority::High);
    let goal = Proposition::new("Acme", "renewed", "the Contoso contract");
    let proposal = Proposal {
        goal: goal.clone(),
        derivation: Rule::Cite {
            evidence_id: "ev_1".to_string(),
            proposition: goal,
        },
    };

    assert_eq!(
        check(&proposal, &snapshot).replay_digest,
        check(&proposal, &snapshot).replay_digest
    );
}
