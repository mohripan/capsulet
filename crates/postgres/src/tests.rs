use std::time::{SystemTime, UNIX_EPOCH};

use capsulet_application::{
    AgentRunRecord, AgentTraceRecord, JobRunLogRepository, JobRunRepository,
};
use capsulet_core::{
    AgentBudget, AgentDefinition, AgentId, AgentRunId, AgentRunStatus, AgentTerminationPolicy,
    ArtifactId, ArtifactObjectKind, Authority, Automation, AutomationId, AutomationStatus,
    AutomationTrigger, CanonicalEntity, CanonicalEntityId, Claim, ClaimId, ClaimStatus, Confidence,
    CustomTriggerPlugin, Entity, EntityGraphAttachment, EntityGraphAttachmentId,
    EntityGraphAttachmentType, EntityId, EntityResolution, EntityResolutionId,
    EntityResolutionStatus, Evidence, EvidenceId, ExecutionPoolName, GraphDefinition,
    GraphHyperedge, GraphId, GraphNode, GraphPort, GraphTransitionPolicy, HyperedgeEndpoint,
    HyperedgeId, IngestionConnector, IngestionConnectorConfig, IngestionConnectorId,
    IngestionConnectorKind, IngestionRun, IngestionRunId, IngestionRunOutputRecord, JobArtifact,
    JobDefinition, JobRun, JobRunId, JobRunLog, JobRunTransition, MemoryContract, MemoryContractId,
    MemoryMemberId, MemoryMemberKind, MemoryScope, MemorySubgraph, MemorySubgraphActivation,
    MemorySubgraphId, MemorySubgraphMember, MemorySubgraphMemberId, MemorySubgraphMemberRole,
    MemorySubgraphOwner, MemorySubgraphOwnerKind, MemorySubgraphPermissions, MemorySubgraphStatus,
    NodeId, NodeKind, PortDirection, PortId, PortValueType, Source, SourceId, SubgraphEdge,
    SubgraphEdgeId, SummaryTrace, SummaryTraceId, TriggerKind, TriggerName, WorkflowDefinition,
    WorkflowId, WorkflowStatus, WorkflowStep, WorkflowStepDependency, WorkflowStepId,
};

use capsulet_core::JobRunStatus;

use super::PostgresStore;

fn database_url() -> Option<String> {
    std::env::var("CAPSULET_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok()
}

#[tokio::test]
async fn saves_loads_and_cascades_workflow_dependencies_when_database_is_available() {
    let Some(database_url) = database_url() else {
        return;
    };
    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");
    let job = JobDefinition::hello_python();
    store
        .upsert_job_definition(&job)
        .await
        .expect("save job definition");
    let workflow_id = WorkflowId::new(unique_id("workflow_dag_test")).expect("workflow id");
    let root_a = WorkflowStepId::new(unique_id("dag_root_a")).expect("step id");
    let root_b = WorkflowStepId::new(unique_id("dag_root_b")).expect("step id");
    let merge = WorkflowStepId::new(unique_id("dag_merge")).expect("step id");
    let make_step = |id: WorkflowStepId, position, name| {
        WorkflowStep::new(
            id,
            workflow_id.clone(),
            position,
            name,
            job.id().clone(),
            ExecutionPoolName::new("mini").expect("pool"),
        )
    };
    let workflow = WorkflowDefinition::with_dependencies(
        workflow_id.clone(),
        "DAG",
        "",
        WorkflowStatus::Enabled,
        vec![
            make_step(root_a.clone(), 1, "A"),
            make_step(root_b.clone(), 2, "B"),
            make_step(merge.clone(), 3, "Merge"),
        ],
        vec![
            WorkflowStepDependency::new(root_a, merge.clone()),
            WorkflowStepDependency::new(root_b, merge),
        ],
    );
    store.upsert_workflow(&workflow).await.expect("save DAG");
    let persisted = store
        .find_workflow(&workflow_id)
        .await
        .expect("load DAG")
        .expect("workflow exists");
    assert_eq!(persisted.dependencies(), workflow.dependencies());

    sqlx::query("DELETE FROM workflow_definitions WHERE id = $1")
        .bind(workflow_id.as_str())
        .execute(store.pool())
        .await
        .expect("delete workflow");
    let edge_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workflow_step_dependencies WHERE workflow_id = $1",
    )
    .bind(workflow_id.as_str())
    .fetch_one(store.pool())
    .await
    .expect("count edges");
    assert_eq!(edge_count, 0);
}

fn unique_id(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after epoch")
        .as_nanos();
    format!("{prefix}_{nanos}")
}

#[test]
fn parses_known_status() {
    assert!("queued".parse::<JobRunStatus>().is_ok());
    assert!("leased".parse::<JobRunStatus>().is_ok());
    assert!("not-real".parse::<JobRunStatus>().is_err());
}

#[tokio::test]
async fn migrates_and_persists_job_runs_when_database_is_available() {
    let Some(database_url) = database_url() else {
        return;
    };

    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");

    let definition = JobDefinition::hello_python();
    store
        .upsert_job_definition(&definition)
        .await
        .expect("upsert job definition");

    let pool_name = unique_id("persistence_pool");
    let run = JobRun::new(
        JobRunId::new(unique_id("run_persistence_test")).expect("valid run id"),
        definition.id().clone(),
        ExecutionPoolName::new(pool_name.clone()).expect("valid pool"),
    );
    store.save(&run).await.expect("save run");

    let persisted = store
        .find_by_id(run.id())
        .await
        .expect("find run")
        .expect("run exists");

    assert_eq!(persisted.id(), run.id());
    assert_eq!(persisted.status(), run.status());

    let leased = store
        .lease_next_queued_run_with_pool_limits("worker-test", 60, &[(pool_name, 1)])
        .await
        .expect("lease next run")
        .expect("queued run available");

    assert_eq!(leased.id(), run.id());
}

fn graph_id(value: &str) -> GraphId {
    GraphId::new(value).expect("graph id")
}

fn node_id(value: &str) -> NodeId {
    NodeId::new(value).expect("node id")
}

fn port_id(value: &str) -> PortId {
    PortId::new(value).expect("port id")
}

fn hyperedge_id(value: &str) -> HyperedgeId {
    HyperedgeId::new(value).expect("hyperedge id")
}

fn sample_graph(prefix: &str) -> GraphDefinition {
    let prompt = node_id(&format!("{prefix}_prompt"));
    let llm = node_id(&format!("{prefix}_llm"));
    GraphDefinition::new(
        graph_id(&unique_id(prefix)),
        "RAG graph",
        vec![
            GraphNode::new(
                prompt.clone(),
                "Prompt",
                NodeKind::PromptBuilder,
                vec![GraphPort::new(
                    port_id("prompt.out"),
                    PortDirection::Output,
                    PortValueType::Prompt,
                )],
            ),
            GraphNode::new(
                llm.clone(),
                "LLM",
                NodeKind::Llm,
                vec![
                    GraphPort::new(
                        port_id("llm.prompt"),
                        PortDirection::Input,
                        PortValueType::Prompt,
                    ),
                    GraphPort::new(
                        port_id("llm.answer"),
                        PortDirection::Output,
                        PortValueType::FinalAnswer,
                    ),
                ],
            ),
        ],
        vec![GraphHyperedge::new(
            hyperedge_id(&format!("{prefix}_prompt_llm")),
            vec![HyperedgeEndpoint::port(
                prompt.clone(),
                port_id("prompt.out"),
            )],
            vec![HyperedgeEndpoint::port(llm.clone(), port_id("llm.prompt"))],
        )],
        GraphTransitionPolicy::planner(vec![prompt, llm]).with_cycles_allowed(true),
    )
    .expect("valid graph")
}

#[tokio::test]
async fn saves_and_lists_memory_claims_when_database_is_available() {
    let Some(database_url) = database_url() else {
        return;
    };
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
async fn saves_ingestion_connector_run_and_outputs_when_database_is_available() {
    let Some(database_url) = database_url() else {
        return;
    };
    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");

    let scope =
        MemoryScope::new("tenant_ingestion", unique_id("project_ingestion")).expect("scope");
    let connector = IngestionConnector::new(
        IngestionConnectorId::new(unique_id("connector_ingestion")).expect("connector id"),
        scope.clone(),
        "Project notes",
        IngestionConnectorKind::LocalText,
        IngestionConnectorConfig::local_text(
            "Project Atlas Notes",
            "- Project Atlas is blocked by Legal Review",
            "text/markdown",
            Some("local://project-atlas.md".to_string()),
            Authority::High,
        ),
        true,
    )
    .expect("connector");
    let run = IngestionRun::queued(
        IngestionRunId::new(unique_id("ingestion_run")).expect("run id"),
        scope.clone(),
        connector.id().clone(),
    )
    .succeeded(1, 1, 1, 1);
    let output = IngestionRunOutputRecord::new(run.id().clone(), "claim", "claim_ingestion_1")
        .expect("output");

    store
        .upsert_ingestion_connector(&connector)
        .await
        .expect("save connector");
    store.upsert_ingestion_run(&run).await.expect("save run");
    store
        .upsert_ingestion_run_output(&output)
        .await
        .expect("save output");

    let connectors = store
        .list_ingestion_connectors(scope.tenant_id(), scope.project_id(), 10)
        .await
        .expect("list connectors");
    let runs = store
        .list_ingestion_runs(scope.tenant_id(), scope.project_id(), 10)
        .await
        .expect("list runs");
    let outputs = store
        .list_ingestion_run_outputs(run.id())
        .await
        .expect("list outputs");

    assert_eq!(connectors[0].id(), connector.id());
    assert_eq!(runs[0].claim_count(), 1);
    assert_eq!(outputs, vec![output]);
}

#[tokio::test]
async fn saves_and_finds_memory_contract_when_database_is_available() {
    let Some(database_url) = database_url() else {
        return;
    };
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
    let Some(database_url) = database_url() else {
        return;
    };
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

#[tokio::test]
async fn saves_and_finds_graph_definition_when_database_is_available() {
    let Some(database_url) = database_url() else {
        return;
    };
    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");
    let graph = sample_graph("graph_round_trip");

    store.upsert_graph(&graph).await.expect("save graph");
    let persisted = store
        .find_graph(graph.id())
        .await
        .expect("load graph")
        .expect("graph exists");

    assert_eq!(persisted.hyperedges(), graph.hyperedges());
}

#[tokio::test]
async fn saves_and_finds_agent_definition_when_database_is_available() {
    let Some(database_url) = database_url() else {
        return;
    };
    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");
    let graph = sample_graph("agent_graph_round_trip");
    let agent = AgentDefinition::new(
        AgentId::new(unique_id("agent_round_trip")).expect("agent id"),
        "Support agent",
        graph,
        Some(AgentBudget::new(8, 8_000, 60, 2_500).expect("budget")),
        Some(AgentTerminationPolicy::default_rag()),
    )
    .expect("valid agent");

    store.upsert_agent(&agent).await.expect("save agent");
    let persisted = store
        .find_agent(agent.id())
        .await
        .expect("load agent")
        .expect("agent exists");

    assert_eq!(persisted.budget().max_steps(), agent.budget().max_steps());
}

#[tokio::test]
async fn saves_and_finds_agent_run_when_database_is_available() {
    let Some(database_url) = database_url() else {
        return;
    };
    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");
    let graph = sample_graph("agent_run_graph");
    let agent = AgentDefinition::new(
        AgentId::new(unique_id("agent_run_agent")).expect("agent id"),
        "Support agent",
        graph,
        Some(AgentBudget::new(8, 8_000, 60, 2_500).expect("budget")),
        Some(AgentTerminationPolicy::default_rag()),
    )
    .expect("valid agent");
    store.upsert_agent(&agent).await.expect("save agent");
    let run = AgentRunRecord {
        id: AgentRunId::new(unique_id("agent_run")).expect("run id"),
        agent_id: agent.id().clone(),
        status: AgentRunStatus::Queued,
        state_version: 0,
        state_json: r#"{"query":"reset MFA"}"#.to_string(),
    };

    store.upsert_agent_run(&run).await.expect("save run");
    let persisted = store
        .find_agent_run(&run.id)
        .await
        .expect("load run")
        .expect("run exists");

    assert_eq!(persisted.state_json, run.state_json);
}

#[tokio::test]
async fn saves_and_lists_agent_trace_events_when_database_is_available() {
    let Some(database_url) = database_url() else {
        return;
    };
    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");
    let graph = sample_graph("agent_trace_graph");
    let agent = AgentDefinition::new(
        AgentId::new(unique_id("agent_trace_agent")).expect("agent id"),
        "Support agent",
        graph,
        Some(AgentBudget::new(8, 8_000, 60, 2_500).expect("budget")),
        Some(AgentTerminationPolicy::default_rag()),
    )
    .expect("valid agent");
    store.upsert_agent(&agent).await.expect("save agent");
    let run = AgentRunRecord {
        id: AgentRunId::new(unique_id("agent_trace_run")).expect("run id"),
        agent_id: agent.id().clone(),
        status: AgentRunStatus::Queued,
        state_version: 0,
        state_json: r#"{"query":"reset MFA"}"#.to_string(),
    };
    store.upsert_agent_run(&run).await.expect("save run");

    store
        .append_agent_trace_event(&AgentTraceRecord {
            run_id: run.id.clone(),
            sequence: 0,
            event_type: "node_started".to_string(),
            payload_json: r#"{"node_id":"prompt"}"#.to_string(),
        })
        .await
        .expect("append first trace");
    store
        .append_agent_trace_event(&AgentTraceRecord {
            run_id: run.id.clone(),
            sequence: 1,
            event_type: "node_completed".to_string(),
            payload_json: r#"{"node_id":"prompt"}"#.to_string(),
        })
        .await
        .expect("append second trace");

    let traces = store
        .list_agent_trace_events(&run.id)
        .await
        .expect("list traces");

    assert_eq!(traces[1].event_type, "node_completed");
}

#[tokio::test]
async fn prometheus_metrics_include_queue_slo_gauges_when_database_is_available() {
    let Some(database_url) = database_url() else {
        return;
    };

    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");

    let definition = JobDefinition::hello_python();
    store
        .upsert_job_definition(&definition)
        .await
        .expect("upsert job definition");
    let pool_name = unique_id("metrics_pool");
    let run = JobRun::new(
        JobRunId::new(unique_id("metrics_run")).expect("run id"),
        definition.id().clone(),
        ExecutionPoolName::new(pool_name.clone()).expect("pool"),
    );
    store.save(&run).await.expect("save queued run");

    let metrics = store
        .prometheus_metrics()
        .await
        .expect("render prometheus metrics");
    assert!(metrics.contains("capsulet_job_queue_oldest_age_seconds"));
    assert!(metrics.contains(&format!("pool=\"{pool_name}\"")));
    assert!(metrics.contains("capsulet_execution_pool_saturation"));
    assert!(metrics.contains("capsulet_workflow_critical_path_latency_seconds"));
}

#[tokio::test]
async fn lease_query_does_not_hand_out_same_run_twice_when_database_is_available() {
    let Some(database_url) = database_url() else {
        return;
    };

    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");
    let definition = JobDefinition::hello_python();
    store
        .upsert_job_definition(&definition)
        .await
        .expect("upsert job definition");

    let pool_name = unique_id("lease_pool");
    let run = JobRun::new(
        JobRunId::new(unique_id("run_lease_test")).expect("valid run id"),
        definition.id().clone(),
        ExecutionPoolName::new(pool_name.clone()).expect("valid pool"),
    );
    store.save(&run).await.expect("save run");

    let first = store
        .lease_next_queued_run_with_pool_limits("worker-a", 60, &[(pool_name.clone(), 1)])
        .await
        .expect("lease first")
        .expect("run available");
    let second = store
        .lease_next_queued_run_with_pool_limits("worker-b", 60, &[(pool_name, 1)])
        .await
        .expect("lease second");

    assert_eq!(first.id(), run.id());
    assert!(second.is_none());
}

#[tokio::test]
async fn pool_limit_prevents_concurrent_leases_when_database_is_available() {
    let Some(database_url) = database_url() else {
        return;
    };
    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");
    let definition = JobDefinition::hello_python();
    store
        .upsert_job_definition(&definition)
        .await
        .expect("upsert job definition");
    let pool_name = unique_id("capacity_pool");
    for suffix in ["a", "b"] {
        let run = JobRun::new(
            JobRunId::new(unique_id(&format!("capacity_run_{suffix}"))).expect("run id"),
            definition.id().clone(),
            ExecutionPoolName::new(pool_name.clone()).expect("pool"),
        );
        store.save(&run).await.expect("save run");
    }
    let limits = vec![(pool_name, 1)];
    let first = store
        .lease_next_queued_run_with_pool_limits("capacity-worker-a", 60, &limits)
        .await
        .expect("first lease");
    let second = store
        .lease_next_queued_run_with_pool_limits("capacity-worker-b", 60, &limits)
        .await
        .expect("second lease");
    assert!(first.is_some());
    assert!(second.is_none());
}

#[tokio::test]
async fn expired_running_lease_is_adopted_without_new_attempt_when_database_is_available() {
    let Some(database_url) = database_url() else {
        return;
    };
    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect");
    store.migrate().await.expect("migrate");
    let definition = JobDefinition::hello_python();
    store
        .upsert_job_definition(&definition)
        .await
        .expect("definition");
    let pool = unique_id("reattach_pool");
    let run = JobRun::new(
        JobRunId::new(unique_id("reattach_run")).expect("run id"),
        definition.id().clone(),
        ExecutionPoolName::new(pool.clone()).expect("pool"),
    );
    store.save(&run).await.expect("save");
    let mut running = store
        .lease_next_queued_run_with_pool_limits("worker-before-crash", 60, &[(pool.clone(), 1)])
        .await
        .expect("lease")
        .expect("run");
    running
        .apply(JobRunTransition::StartAttempt)
        .expect("start");
    store.save(&running).await.expect("save running");
    sqlx::query("UPDATE job_runs SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(running.id().as_str())
        .execute(store.pool())
        .await
        .expect("expire lease");

    store
        .recover_expired_leases_for_runner(true)
        .await
        .expect("recover");
    let adopted = store
        .lease_next_queued_run_with_pool_limits_and_reattach(
            "worker-after-crash",
            60,
            &[(pool, 1)],
            true,
        )
        .await
        .expect("adopt")
        .expect("running run");
    assert_eq!(adopted.status(), JobRunStatus::Running);
    assert_eq!(adopted.attempt_count(), running.attempt_count());
}

#[tokio::test]
async fn finds_job_definition_when_database_is_available() {
    let Some(database_url) = database_url() else {
        return;
    };

    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");

    let definition = JobDefinition::hello_python();
    store
        .upsert_job_definition(&definition)
        .await
        .expect("upsert job definition");

    let persisted = store
        .find_job_definition(definition.id())
        .await
        .expect("find definition")
        .expect("definition exists");

    assert_eq!(persisted, definition);
}

#[tokio::test]
async fn saves_and_finds_automation_when_database_is_available() {
    let Some(database_url) = database_url() else {
        return;
    };

    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");

    let workflow = WorkflowDefinition::new(
        WorkflowId::new(unique_id("workflow_automation_test")).expect("workflow id"),
        "Automation persistence workflow",
        "",
        WorkflowStatus::Enabled,
        Vec::new(),
    );
    store
        .upsert_workflow(&workflow)
        .await
        .expect("save workflow");

    let automation = Automation::new(
        AutomationId::new(unique_id("automation_interval_test")).expect("automation id"),
        "Interval automation",
        "",
        workflow.id().clone(),
        "{}",
        AutomationStatus::Enabled,
    );
    store
        .upsert_automation(&automation)
        .await
        .expect("save automation");

    let persisted = store
        .find_automation(automation.id())
        .await
        .expect("find automation")
        .expect("automation exists");

    assert_eq!(persisted.status(), AutomationStatus::Enabled);
}

#[tokio::test]
async fn due_schedule_trigger_creates_workflow_run_when_database_is_available() {
    let Some(database_url) = database_url() else {
        return;
    };

    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");

    let workflow = WorkflowDefinition::new(
        WorkflowId::new(unique_id("workflow_schedule_trigger_test")).expect("workflow id"),
        "Schedule trigger workflow",
        "",
        WorkflowStatus::Enabled,
        Vec::new(),
    );
    store
        .upsert_workflow(&workflow)
        .await
        .expect("save workflow");

    let automation = Automation::new(
        AutomationId::new(unique_id("automation_schedule_trigger_test")).expect("automation id"),
        "Schedule trigger automation",
        "",
        workflow.id().clone(),
        "{}",
        AutomationStatus::Enabled,
    );
    store
        .upsert_automation(&automation)
        .await
        .expect("save automation");
    store
        .replace_automation_triggers(
            automation.id(),
            &[AutomationTrigger::new(
                automation.id().clone(),
                TriggerName::new("schedule_ready").expect("trigger name"),
                TriggerKind::Schedule,
                "{\"interval_seconds\":30}",
                None,
                true,
            )],
            "{\"trigger\":\"schedule_ready\"}",
        )
        .await
        .expect("save trigger");

    let triggered = store
        .trigger_due_interval_automations()
        .await
        .expect("trigger due schedule automations");
    let created_for_automation: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workflow_runs WHERE automation_id = $1 AND workflow_id = $2",
    )
    .bind(automation.id().as_str())
    .bind(workflow.id().as_str())
    .fetch_one(store.pool())
    .await
    .expect("count workflow runs for automation");

    assert!(triggered >= 1);
    assert_eq!(created_for_automation, 1);
}

#[tokio::test]
async fn custom_trigger_claim_is_exclusive_when_database_is_available() {
    let Some(database_url) = database_url() else {
        return;
    };
    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");
    let workflow = WorkflowDefinition::new(
        WorkflowId::new(unique_id("workflow_custom_trigger")).expect("workflow id"),
        "Custom trigger workflow",
        "",
        WorkflowStatus::Enabled,
        Vec::new(),
    );
    store.upsert_workflow(&workflow).await.expect("workflow");
    let automation = Automation::new(
        AutomationId::new(unique_id("automation_custom_trigger")).expect("automation id"),
        "Custom trigger automation",
        "",
        workflow.id().clone(),
        "{}",
        AutomationStatus::Enabled,
    );
    store
        .upsert_automation(&automation)
        .await
        .expect("automation");
    let plugin_id = unique_id("plugin_custom_trigger");
    store
        .upsert_custom_trigger_plugin(&CustomTriggerPlugin::new(
            plugin_id.clone(),
            "Plugin",
            "",
            "example.invalid/plugin@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            vec!["/plugin".to_string()],
            "{}",
        ))
        .await
        .expect("plugin");
    store
        .replace_automation_triggers(
            automation.id(),
            &[AutomationTrigger::new(
                automation.id().clone(),
                TriggerName::new("ready").expect("trigger name"),
                TriggerKind::Custom,
                "{\"poll_seconds\":60}",
                Some(plugin_id),
                true,
            )],
            "{\"trigger\":\"ready\"}",
        )
        .await
        .expect("trigger");

    let claimed = store
        .claim_custom_trigger("evaluator-a", 60)
        .await
        .expect("claim")
        .expect("due custom trigger");
    assert_eq!(claimed.trigger_name, "ready");
    assert_eq!(claimed.automation_id, automation.id().as_str());
    let second_claim = store
        .claim_custom_trigger("evaluator-b", 60)
        .await
        .expect("second claim");
    assert!(second_claim.as_ref().is_none_or(|trigger| {
        trigger.automation_id != claimed.automation_id
            || trigger.trigger_name != claimed.trigger_name
    }));
    store
        .complete_custom_trigger("evaluator-a", &claimed, 60, None, "delivery")
        .await
        .expect("complete");
    if let Some(trigger) = second_claim {
        store
            .complete_custom_trigger("evaluator-b", &trigger, 60, None, "delivery")
            .await
            .expect("complete unrelated trigger claimed during test");
    }
}

#[tokio::test]
async fn saves_and_finds_job_run_logs_when_database_is_available() {
    let Some(database_url) = database_url() else {
        return;
    };

    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");

    let definition = JobDefinition::hello_python();
    store
        .upsert_job_definition(&definition)
        .await
        .expect("upsert job definition");

    let run = JobRun::new(
        JobRunId::new(unique_id("run_log_test")).expect("valid run id"),
        definition.id().clone(),
        ExecutionPoolName::new("mini").expect("valid pool"),
    );
    store.save(&run).await.expect("save run");

    let log = JobRunLog::new(run.id().clone(), "hello from postgres logs\n").expect("valid log");
    store.save_log(&log).await.expect("save log");

    let persisted = store
        .find_log_by_run_id(run.id())
        .await
        .expect("find log")
        .expect("log exists");

    assert_eq!(persisted, log);
}

#[tokio::test]
async fn saves_lists_and_finds_artifacts_when_database_is_available() {
    let Some(database_url) = database_url() else {
        return;
    };

    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");

    let definition = JobDefinition::hello_python();
    store
        .upsert_job_definition(&definition)
        .await
        .expect("upsert job definition");

    let run = JobRun::new(
        JobRunId::new(unique_id("run_artifact_test")).expect("valid run id"),
        definition.id().clone(),
        ExecutionPoolName::new("mini").expect("valid pool"),
    );
    let other_run = JobRun::new(
        JobRunId::new(unique_id("run_artifact_other_test")).expect("valid run id"),
        definition.id().clone(),
        ExecutionPoolName::new("mini").expect("valid pool"),
    );
    store.save(&run).await.expect("save run");
    store.save(&other_run).await.expect("save other run");

    let artifact = JobArtifact::new(
        ArtifactId::new(unique_id("artifact_postgres_test")).expect("valid artifact id"),
        run.id().clone(),
        None,
        "report.txt",
        "artifacts/run/report.txt",
        "text/plain",
        12,
        Some("abc123".to_string()),
        ArtifactObjectKind::Artifact,
    )
    .expect("valid artifact");
    store
        .upsert_artifact(&artifact)
        .await
        .expect("save artifact");

    let artifacts = store
        .list_artifacts(run.id())
        .await
        .expect("list artifacts");
    assert_eq!(artifacts, vec![artifact.clone()]);

    let persisted = store
        .find_artifact(run.id(), artifact.id())
        .await
        .expect("find artifact")
        .expect("artifact exists");
    assert_eq!(persisted, artifact);

    let isolated = store
        .find_artifact(other_run.id(), artifact.id())
        .await
        .expect("find artifact for other run");
    assert!(isolated.is_none());
}
