use capsulet_core::{MemoryContract, MemoryContractId};

const CONTRACT: &str = r"
entity Project:
  aliases:
    - initiative
  fields:
    name: string
    status: enum[planned, active, blocked, completed]
    owner: Person

entity Person:
  fields:
    name: string

relation owns:
  from: Person
  to: Project

event DecisionMade:
  fields:
    summary: string
    decided_at: date

claim_policy:
  require_source: true
  store_confidence: true
  allow_contradictions: true
  min_confidence: 0.70

trust_policy:
  source_priority:
    - board_minutes
    - jira
    - slack

review_policy:
  require_human_review_if:
    - confidence < 0.75
    - relation in [approved, legally_obligated]
    - contradiction_detected == true

retrieval_policy customer_support:
  seed_from:
    - Customer
    - Product
    - Issue
  expand:
    max_hops: 3
    prefer_edges:
      - has_open_ticket
      - resolved_by
  include_claims:
    status: active
    min_confidence: 0.7
  exclude:
    stale: true
    restricted: true

contradiction_rule launch_date_conflict:
  applies_to: Project.launch_date
  if_multiple_active_values: true
  resolve_by:
    - highest_authority_source
    - newest_observed_at
  require_review: true
";

#[test]
fn memory_contract_parses_and_compiles_runtime_policy() {
    let contract = MemoryContract::parse(
        MemoryContractId::new("contract_project_memory").expect("contract id"),
        "Project memory",
        CONTRACT,
    )
    .expect("contract parses");

    let policy = contract.compile().expect("contract compiles");

    assert_eq!(policy.entity_types()[0].name(), "Project");
    assert!((policy.claim_policy().min_confidence().value() - 0.70).abs() < f64::EPSILON);
    assert_eq!(
        policy
            .retrieval_policies()
            .iter()
            .find(|policy| policy.name() == "customer_support")
            .expect("retrieval policy")
            .max_hops(),
        Some(3)
    );
}

#[test]
fn memory_contract_compile_rejects_unknown_relation_endpoints() {
    let source = r"
entity Project:
  fields:
    name: string

relation owns:
  from: Person
  to: Project
";

    let error = MemoryContract::parse(
        MemoryContractId::new("contract_invalid_relation").expect("contract id"),
        "Invalid relation",
        source,
    )
    .expect("contract parses")
    .compile()
    .expect_err("unknown relation endpoint is rejected");

    assert!(error.to_string().contains("unknown entity type Person"));
}

#[test]
fn memory_contract_parse_rejects_claim_policy_without_required_source() {
    let source = r"
entity Project:
  fields:
    name: string

claim_policy:
  require_source: false
  store_confidence: true
";

    let error = MemoryContract::parse(
        MemoryContractId::new("contract_missing_source_policy").expect("contract id"),
        "Missing source policy",
        source,
    )
    .expect("contract parses")
    .compile()
    .expect_err("missing source policy is rejected");

    assert!(
        error
            .to_string()
            .contains("claim policy must require sources")
    );
}

#[test]
fn bundled_memory_contract_examples_compile() {
    for (name, source) in [
        (
            "project-management",
            include_str!("../../../docs/memory-contracts/project-management.contract"),
        ),
        (
            "legal",
            include_str!("../../../docs/memory-contracts/legal.contract"),
        ),
        (
            "engineering",
            include_str!("../../../docs/memory-contracts/engineering.contract"),
        ),
        (
            "support",
            include_str!("../../../docs/memory-contracts/support.contract"),
        ),
    ] {
        let contract = MemoryContract::parse(
            MemoryContractId::new(format!("contract_{name}")).expect("contract id"),
            name,
            source,
        )
        .expect("example contract parses");

        contract.compile().expect("example contract compiles");
    }
}
