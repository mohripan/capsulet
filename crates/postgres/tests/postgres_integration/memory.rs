//! Claim memory, memory contracts and the memory graph.

use super::*;

#[tokio::test]
async fn saves_and_lists_memory_claims_when_database_is_available() {
    let database_url = database_url();
    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");

    let scope = MemoryScope::new("tenant_memory", unique_id("project_memory")).expect("scope");
    let source = Source::new(
        SourceId::new(unique_id("source_memory")).expect("source id"),
        scope.clone(),
        "executive_update",
        Some("file:///updates/atlas.md".to_string()),
        "Atlas update",
        Authority::High,
    )
    .expect("source");
    store
        .upsert_memory_source(&source)
        .await
        .expect("save source");
    let evidence = Evidence::new(
        EvidenceId::new(unique_id("evidence_memory")).expect("evidence id"),
        scope.clone(),
        source.id().clone(),
        "updates/atlas.md#L12",
        "Project Atlas launch date moved to August 1.",
        "2026-07-05T10:00:00Z",
    )
    .expect("evidence");
    store
        .upsert_memory_evidence(&evidence)
        .await
        .expect("save evidence");
    let entity = Entity::new(
        EntityId::new(unique_id("entity_memory")).expect("entity id"),
        scope.clone(),
        "Project",
        "Project Atlas",
        vec!["Atlas".to_string()],
    )
    .expect("entity");
    store
        .upsert_memory_entity(&entity)
        .await
        .expect("save entity");
    let claim = Claim::new(
        ClaimId::new(unique_id("claim_memory")).expect("claim id"),
        scope.clone(),
        entity.id().clone(),
        "launch_date",
        "2026-08-01",
        vec![evidence.id().clone()],
        Confidence::new(0.93).expect("confidence"),
        Authority::High,
        "2026-07-05T10:00:00Z",
        Some("2026-07-05T00:00:00Z"),
        None,
    )
    .expect("claim")
    .with_status(ClaimStatus::Active);
    store.upsert_memory_claim(&claim).await.expect("save claim");

    let claims = store
        .list_memory_claims(scope.tenant_id(), scope.project_id(), 10)
        .await
        .expect("list claims");

    assert_eq!(claims[0].evidence_ids(), claim.evidence_ids());
}

#[tokio::test]
async fn saves_and_finds_memory_contract_when_database_is_available() {
    let database_url = database_url();
    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");

    let source = r"
entity Project:
  fields:
    name: string
    owner: Person

entity Person:
  fields:
    name: string

relation owns:
  from: Person
  to: Project

claim_policy:
  require_source: true
  store_confidence: true
  min_confidence: 0.8
";
    let contract = MemoryContract::parse_scoped(
        MemoryContractId::new(unique_id("contract_memory")).expect("contract id"),
        MemoryScope::new("tenant_contract", unique_id("project_contract")).expect("scope"),
        "Project contract",
        source,
    )
    .expect("contract");
    store
        .upsert_memory_contract(&contract)
        .await
        .expect("save contract");

    let persisted = store
        .find_memory_contract(contract.id())
        .await
        .expect("find contract")
        .expect("contract exists");

    assert_eq!(
        persisted.compile().expect("compiled").relation_types()[0].name(),
        "owns"
    );
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "nested memory graph persistence test covers the full activation and boundary-edge flow"
)]
async fn saves_and_activates_nested_memory_graphs_when_database_is_available() {
    let database_url = database_url();
    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");

    let scope = MemoryScope::new("tenant_nested", unique_id("project_nested")).expect("scope");
    let contract = MemoryContract::parse_scoped(
        MemoryContractId::new(unique_id("contract_nested")).expect("contract id"),
        scope.clone(),
        "Nested memory contract",
        r"
entity Project:
  fields:
    name: string

claim_policy:
  require_source: true
  store_confidence: true
",
    )
    .expect("contract");
    store
        .upsert_memory_contract(&contract)
        .await
        .expect("save contract");
    let source = Source::new(
        SourceId::new(unique_id("source_nested")).expect("source id"),
        scope.clone(),
        "roadmap",
        None,
        "Roadmap",
        Authority::High,
    )
    .expect("source");
    store.upsert_memory_source(&source).await.expect("source");
    let evidence = Evidence::new(
        EvidenceId::new(unique_id("evidence_nested")).expect("evidence id"),
        scope.clone(),
        source.id().clone(),
        "roadmap.md#L1",
        "Sales says August and engineering says September.",
        "2026-07-02T00:00:00Z",
    )
    .expect("evidence");
    store
        .upsert_memory_evidence(&evidence)
        .await
        .expect("evidence");
    let entity = Entity::new(
        EntityId::new(unique_id("entity_nested")).expect("entity id"),
        scope.clone(),
        "Project",
        "Capsulet",
        Vec::new(),
    )
    .expect("entity");
    store.upsert_memory_entity(&entity).await.expect("entity");
    let summary_claim = Claim::new(
        ClaimId::new(unique_id("claim_summary")).expect("claim id"),
        scope.clone(),
        entity.id().clone(),
        "summary",
        "Sales and engineering disagree on target month.",
        vec![evidence.id().clone()],
        Confidence::new(0.9).expect("confidence"),
        Authority::High,
        "2026-07-02T00:00:00Z",
        None,
        None,
    )
    .expect("claim")
    .with_status(ClaimStatus::Active);
    store
        .upsert_memory_claim(&summary_claim)
        .await
        .expect("summary claim");

    let sales_subgraph = MemorySubgraph::draft(
        MemorySubgraphId::new(unique_id("subgraph_sales")).expect("subgraph id"),
        scope.clone(),
        None,
        "Sales memory",
        Some("Sales commitments"),
    )
    .expect("draft");
    store
        .upsert_memory_subgraph(&sales_subgraph)
        .await
        .expect("save draft");
    let member = MemorySubgraphMember::new(
        MemorySubgraphMemberId::new(unique_id("member_summary")).expect("member id"),
        scope.clone(),
        sales_subgraph.id().clone(),
        MemoryMemberKind::Claim,
        MemoryMemberId::new(summary_claim.id().as_str()).expect("member id"),
        MemorySubgraphMemberRole::Summary,
    )
    .expect("member");
    store
        .upsert_memory_subgraph_member(&member)
        .await
        .expect("save member");
    let canonical = CanonicalEntity::new(
        CanonicalEntityId::new(unique_id("canonical_capsulet")).expect("canonical id"),
        scope.clone(),
        "Project",
        "Capsulet",
        Vec::new(),
    )
    .expect("canonical");
    store
        .upsert_memory_canonical_entity(&canonical)
        .await
        .expect("save canonical");
    let resolution = EntityResolution::new(
        EntityResolutionId::new(unique_id("resolution_capsulet")).expect("resolution id"),
        scope.clone(),
        sales_subgraph.id().clone(),
        entity.id().clone(),
        canonical.id().clone(),
        Confidence::new(0.95).expect("confidence"),
        EntityResolutionStatus::Confirmed,
        vec![evidence.id().clone()],
    )
    .expect("resolution");
    store
        .upsert_memory_entity_resolution(&resolution)
        .await
        .expect("save resolution");
    let trace = SummaryTrace::new(
        SummaryTraceId::new(unique_id("trace_summary")).expect("trace id"),
        scope.clone(),
        sales_subgraph.id().clone(),
        summary_claim.id().clone(),
        Vec::new(),
        vec![evidence.id().clone()],
    )
    .expect("trace");
    store
        .upsert_memory_summary_trace(&trace)
        .await
        .expect("save trace");
    let activated = sales_subgraph
        .activate(MemorySubgraphActivation::new(
            Some(MemorySubgraphOwner::new(MemorySubgraphOwnerKind::Team, "sales").expect("owner")),
            Some(contract.id().clone()),
            Some(MemorySubgraphPermissions::new(r#"{"read":["sales"]}"#).expect("permissions")),
            Some(summary_claim.id().clone()),
            vec![trace.clone()],
        ))
        .expect("activate");
    store
        .upsert_memory_subgraph(&activated)
        .await
        .expect("save active");
    let attachment = EntityGraphAttachment::new(
        EntityGraphAttachmentId::new(unique_id("attachment_capsulet")).expect("attachment id"),
        scope.clone(),
        canonical.id().clone(),
        activated.id().clone(),
        EntityGraphAttachmentType::Primary,
    )
    .expect("attachment");
    store
        .upsert_memory_entity_graph_attachment(&attachment)
        .await
        .expect("save attachment");
    let engineering_subgraph = MemorySubgraph::draft(
        MemorySubgraphId::new(unique_id("subgraph_engineering")).expect("subgraph id"),
        scope.clone(),
        None,
        "Engineering memory",
        None,
    )
    .expect("engineering draft");
    store
        .upsert_memory_subgraph(&engineering_subgraph)
        .await
        .expect("save engineering");
    let edge = SubgraphEdge::new(
        SubgraphEdgeId::new(unique_id("edge_contradicts")).expect("edge id"),
        scope.clone(),
        "contradicts",
        activated.id().clone(),
        engineering_subgraph.id().clone(),
        MemoryMemberKind::Claim,
        MemoryMemberId::new(summary_claim.id().as_str()).expect("from member"),
        MemoryMemberKind::Claim,
        MemoryMemberId::new("claim_engineering_target").expect("to member"),
        vec![summary_claim.id().clone()],
        vec![evidence.id().clone()],
    )
    .expect("edge");
    store
        .upsert_memory_subgraph_edge(&edge)
        .await
        .expect("save edge");

    let persisted = store
        .find_memory_subgraph(activated.id())
        .await
        .expect("find subgraph")
        .expect("subgraph exists");

    assert_eq!(persisted.status(), MemorySubgraphStatus::Active);
}
