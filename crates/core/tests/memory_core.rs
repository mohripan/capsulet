use capsulet_core::{
    Authority, Claim, ClaimId, ClaimStatus, Confidence, EntityId, EvidenceId, MemoryScope,
};

fn scope() -> MemoryScope {
    MemoryScope::new("tenant_alpha", "project_atlas").expect("valid scope")
}

#[test]
fn claim_new_rejects_claim_without_evidence() {
    let result = Claim::new(
        ClaimId::new("claim_missing_evidence").expect("claim id"),
        scope(),
        EntityId::new("project_atlas").expect("entity id"),
        "launch_date",
        "2026-08-01",
        Vec::new(),
        Confidence::new(0.91).expect("confidence"),
        Authority::High,
        "2026-07-02T10:00:00Z",
        None,
        None,
    );

    assert!(result.is_err());
}

#[test]
fn claim_status_transition_marks_superseded_without_changing_evidence() {
    let claim = Claim::new(
        ClaimId::new("claim_launch_july").expect("claim id"),
        scope(),
        EntityId::new("project_atlas").expect("entity id"),
        "launch_date",
        "2026-07-15",
        vec![EvidenceId::new("evidence_roadmap_v3").expect("evidence id")],
        Confidence::new(0.82).expect("confidence"),
        Authority::Medium,
        "2026-07-02T10:00:00Z",
        Some("2026-06-20T00:00:00Z"),
        None,
    )
    .expect("claim");

    let original_evidence = claim.evidence_ids().to_vec();
    let superseded = claim.with_status(ClaimStatus::Superseded);

    assert_eq!(superseded.status(), ClaimStatus::Superseded);
    assert_eq!(superseded.evidence_ids(), original_evidence);
}

#[test]
fn active_conflicting_claims_can_coexist() {
    let first = Claim::new(
        ClaimId::new("claim_launch_july").expect("claim id"),
        scope(),
        EntityId::new("project_atlas").expect("entity id"),
        "launch_date",
        "2026-07-15",
        vec![EvidenceId::new("evidence_roadmap_v3").expect("evidence id")],
        Confidence::new(0.82).expect("confidence"),
        Authority::Medium,
        "2026-07-02T10:00:00Z",
        Some("2026-06-20T00:00:00Z"),
        None,
    )
    .expect("first claim");
    let second = Claim::new(
        ClaimId::new("claim_launch_august").expect("claim id"),
        scope(),
        EntityId::new("project_atlas").expect("entity id"),
        "launch_date",
        "2026-08-01",
        vec![EvidenceId::new("evidence_exec_update").expect("evidence id")],
        Confidence::new(0.93).expect("confidence"),
        Authority::High,
        "2026-07-05T10:00:00Z",
        Some("2026-07-05T00:00:00Z"),
        None,
    )
    .expect("second claim");

    assert!(first.conflicts_with(&second));
}
