use capsulet_core::{
    CanonicalEntity, CanonicalEntityId, ClaimId, Confidence, EntityGraphAttachment,
    EntityGraphAttachmentId, EntityGraphAttachmentType, EntityId, EntityResolution,
    EntityResolutionId, EntityResolutionStatus, EvidenceId, MemoryContractId, MemoryGraphError,
    MemoryMemberId, MemoryMemberKind, MemoryScope, MemorySubgraph, MemorySubgraphActivation,
    MemorySubgraphId, MemorySubgraphMember, MemorySubgraphMemberId, MemorySubgraphMemberRole,
    MemorySubgraphOwner, MemorySubgraphOwnerKind, MemorySubgraphPermissions, MemorySubgraphStatus,
    SubgraphEdge, SubgraphEdgeId, SummaryTrace, SummaryTraceId,
};

fn scope() -> MemoryScope {
    MemoryScope::new("tenant_alpha", "project_memory").expect("scope")
}

fn subgraph_id(value: &str) -> MemorySubgraphId {
    MemorySubgraphId::new(value).expect("subgraph id")
}

fn claim_id(value: &str) -> ClaimId {
    ClaimId::new(value).expect("claim id")
}

fn evidence_id(value: &str) -> EvidenceId {
    EvidenceId::new(value).expect("evidence id")
}

fn draft_subgraph() -> MemorySubgraph {
    MemorySubgraph::draft(
        subgraph_id("subgraph_sales"),
        scope(),
        None,
        "Sales memory",
        Some("Customer-facing commitments"),
    )
    .expect("draft subgraph")
}

#[test]
fn memory_subgraph_activate_rejects_missing_owner() {
    let activation = MemorySubgraphActivation::new(
        None,
        Some(MemoryContractId::new("contract_sales").expect("contract id")),
        Some(MemorySubgraphPermissions::new(r#"{"read":["sales"]}"#).expect("permissions")),
        Some(claim_id("claim_sales_summary")),
        vec![
            SummaryTrace::new(
                SummaryTraceId::new("trace_sales_summary").expect("trace id"),
                scope(),
                subgraph_id("subgraph_sales"),
                claim_id("claim_sales_summary"),
                vec![claim_id("claim_sales_august")],
                Vec::new(),
            )
            .expect("summary trace"),
        ],
    );

    let error = draft_subgraph()
        .activate(activation)
        .expect_err("activation fails");

    assert_eq!(error, MemoryGraphError::MissingOwner);
}

#[test]
fn memory_subgraph_activate_requires_summary_trace_for_summary_claim() {
    let activation = MemorySubgraphActivation::new(
        Some(MemorySubgraphOwner::new(MemorySubgraphOwnerKind::Team, "sales").expect("owner")),
        Some(MemoryContractId::new("contract_sales").expect("contract id")),
        Some(MemorySubgraphPermissions::new(r#"{"read":["sales"]}"#).expect("permissions")),
        Some(claim_id("claim_sales_summary")),
        Vec::new(),
    );

    let error = draft_subgraph()
        .activate(activation)
        .expect_err("activation fails");

    assert_eq!(error, MemoryGraphError::MissingSummaryTrace);
}

#[test]
fn memory_subgraph_activate_sets_active_status_when_required_invariants_exist() {
    let summary_claim_id = claim_id("claim_sales_summary");
    let activation = MemorySubgraphActivation::new(
        Some(MemorySubgraphOwner::new(MemorySubgraphOwnerKind::Team, "sales").expect("owner")),
        Some(MemoryContractId::new("contract_sales").expect("contract id")),
        Some(MemorySubgraphPermissions::new(r#"{"read":["sales"]}"#).expect("permissions")),
        Some(summary_claim_id.clone()),
        vec![
            SummaryTrace::new(
                SummaryTraceId::new("trace_sales_summary").expect("trace id"),
                scope(),
                subgraph_id("subgraph_sales"),
                summary_claim_id,
                vec![claim_id("claim_sales_august")],
                Vec::new(),
            )
            .expect("summary trace"),
        ],
    );

    let subgraph = draft_subgraph()
        .activate(activation)
        .expect("activated subgraph");

    assert_eq!(subgraph.status(), MemorySubgraphStatus::Active);
}

#[test]
fn summary_trace_rejects_empty_inner_claims_and_evidence() {
    let error = SummaryTrace::new(
        SummaryTraceId::new("trace_empty").expect("trace id"),
        scope(),
        subgraph_id("subgraph_sales"),
        claim_id("claim_sales_summary"),
        Vec::new(),
        Vec::new(),
    )
    .expect_err("trace requires support");

    assert_eq!(error, MemoryGraphError::MissingTraceSupport);
}

#[test]
fn confirmed_entity_resolution_requires_evidence() {
    let error = EntityResolution::new(
        EntityResolutionId::new("resolution_project").expect("resolution id"),
        scope(),
        subgraph_id("subgraph_sales"),
        EntityId::new("entity_project_atlas").expect("entity id"),
        CanonicalEntityId::new("canonical_project_atlas").expect("canonical entity id"),
        Confidence::new(0.91).expect("confidence"),
        EntityResolutionStatus::Confirmed,
        Vec::new(),
    )
    .expect_err("confirmed resolution requires evidence");

    assert_eq!(error, MemoryGraphError::MissingEvidence);
}

#[test]
fn subgraph_edge_requires_different_boundary_contexts() {
    let error = SubgraphEdge::new(
        SubgraphEdgeId::new("edge_invalid").expect("edge id"),
        scope(),
        "contradicts",
        subgraph_id("subgraph_sales"),
        subgraph_id("subgraph_sales"),
        MemoryMemberKind::Claim,
        MemoryMemberId::new("claim_sales_august").expect("member id"),
        MemoryMemberKind::Claim,
        MemoryMemberId::new("claim_engineering_september").expect("member id"),
        vec![claim_id("claim_boundary")],
        vec![evidence_id("evidence_boundary")],
    )
    .expect_err("same boundary rejected");

    assert_eq!(error, MemoryGraphError::SameSubgraphBoundary);
}

#[test]
fn canonical_entity_can_attach_primary_entity_graph() {
    let canonical = CanonicalEntity::new(
        CanonicalEntityId::new("canonical_customer_a").expect("canonical id"),
        scope(),
        "Customer",
        "Customer A",
        vec!["Acme".to_string()],
    )
    .expect("canonical entity");

    let attachment = EntityGraphAttachment::new(
        EntityGraphAttachmentId::new("attachment_customer_a").expect("attachment id"),
        scope(),
        canonical.id().clone(),
        subgraph_id("subgraph_customer_a"),
        EntityGraphAttachmentType::Primary,
    )
    .expect("attachment");

    assert_eq!(attachment.canonical_entity_id(), canonical.id());
}

#[test]
fn subgraph_member_records_claim_membership_role() {
    let member = MemorySubgraphMember::new(
        MemorySubgraphMemberId::new("member_claim_sales_august").expect("member id"),
        scope(),
        subgraph_id("subgraph_sales"),
        MemoryMemberKind::Claim,
        MemoryMemberId::new("claim_sales_august").expect("member id"),
        MemorySubgraphMemberRole::InnerClaim,
    )
    .expect("member");

    assert_eq!(member.role(), MemorySubgraphMemberRole::InnerClaim);
}
