//! Typed graphs, agent definitions, agent runs and their trace events.

use super::*;

fn scoped_agent(id: &str, graph: GraphDefinition) -> AgentDefinition {
    AgentDefinition::new(
        AgentId::new(id).expect("agent id"),
        "Scoped agent",
        graph,
        Some(AgentBudget::new(4, 1_000, 60, 1_000).expect("budget")),
        Some(AgentTerminationPolicy::new(vec![
            capsulet_core::TerminationCondition::ValidatorPass,
        ])),
    )
    .expect("agent")
}

fn scoped_agent_run(id: &str, agent: &AgentDefinition) -> AgentRunRecord {
    AgentRunRecord {
        id: AgentRunId::new(id).expect("run id"),
        agent_id: agent.id().clone(),
        status: AgentRunStatus::Queued,
        state_version: 0,
        state_json: r#"{"query":"scoped"}"#.to_string(),
        execution: None,
    }
}

/// The smallest definition a run can be started against. These tests are about who
/// owns what, so nothing in them cares what it does.
fn minimal_definition() -> &'static capsulet_ir::Definition {
    static DEFINITION: std::sync::OnceLock<capsulet_ir::Definition> = std::sync::OnceLock::new();
    DEFINITION.get_or_init(|| capsulet_ir::Definition {
        schema_version: capsulet_ir::Definition::current_schema_version(),
        packs: Vec::new(),
        id: capsulet_ir::Identifier::parse("minimal").expect("valid"),
        version: "1".to_string(),
        name: "Minimal".to_string(),
        assurance: capsulet_ir::AssuranceMode::Observe,
        capabilities: capsulet_ir::CapabilitySet::empty(),
        budget: capsulet_ir::ResourceBudget::deterministic(1_000),
        graph: capsulet_ir::Graph::new(capsulet_ir::GraphBuilder::default()).expect("valid"),
        boundaries: vec![],
        contracts: vec![],
    })
}

fn minimal_admission() -> &'static capsulet_ir::admission::AdmissionRecord {
    static ADMISSION: std::sync::OnceLock<capsulet_ir::admission::AdmissionRecord> =
        std::sync::OnceLock::new();
    ADMISSION.get_or_init(|| capsulet_ir::admit(minimal_definition()).expect("admitted"))
}

fn no_inputs() -> &'static std::collections::BTreeMap<String, String> {
    static INPUTS: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    &INPUTS
}

fn minimal_start(run: &AgentRunRecord) -> capsulet_postgres::AgentRunStart<'_> {
    capsulet_postgres::AgentRunStart {
        run,
        definition: minimal_definition(),
        admission: minimal_admission(),
        inputs: no_inputs(),
        trace_parent: None,
        at: capsulet_ir::correctness::evidence::RecordedTime(0),
    }
}

#[tokio::test]
async fn graph_upsert_cannot_replace_another_projects_definition_or_parts() {
    let store = scoped_store().await;
    let project = unique_id("graph_owner");
    let foreign = unique_id("graph_foreign");
    let original = sample_graph("scoped_graph");
    store
        .upsert_graph_in_project(&original, "default", &project)
        .await
        .expect("create in the owning project");

    let replacement = GraphDefinition::new(
        original.id().clone(),
        "Taken over",
        vec![],
        vec![],
        GraphTransitionPolicy::static_acyclic(),
    )
    .expect("a valid empty graph");
    let refused = store
        .upsert_graph_in_project(&replacement, "default", &foreign)
        .await
        .expect_err("another project must not replace the graph");
    assert!(matches!(refused, PostgresStoreError::ScopeMismatch));

    let stored = store
        .find_graph_in_project(original.id(), "default", &project)
        .await
        .expect("find")
        .expect("the original survives");
    assert_eq!(stored.name(), original.name());
    assert_eq!(stored.nodes().len(), original.nodes().len());
    assert_eq!(stored.hyperedges().len(), original.hyperedges().len());
    assert!(
        store
            .find_graph_in_project(original.id(), "default", &foreign)
            .await
            .expect("find")
            .is_none(),
        "the graph must be invisible outside its project"
    );
    assert!(
        store
            .find_graph_in_project(original.id(), "other-tenant", &project)
            .await
            .expect("find")
            .is_none(),
        "the graph must be invisible outside its tenant"
    );
}

#[tokio::test]
async fn graph_listing_applies_scope_before_the_limit() {
    let store = scoped_store().await;
    let project = unique_id("graph_list");
    let owned = sample_graph("listed_graph");
    store
        .upsert_graph_in_project(&owned, "default", &project)
        .await
        .expect("owned");
    let newer_foreign = sample_graph("listed_foreign");
    store
        .upsert_graph_in_project(&newer_foreign, "default", &unique_id("graph_list_other"))
        .await
        .expect("foreign");

    let listed = store
        .list_graphs_in_project("default", &project, 1)
        .await
        .expect("list");
    assert_eq!(listed.len(), 1, "foreign rows must not consume the limit");
    assert_eq!(listed[0].id(), owned.id());
}

#[tokio::test]
async fn agent_upsert_refuses_a_graph_from_another_project_without_writing() {
    let store = scoped_store().await;
    let project = unique_id("agent_owner");
    let foreign = unique_id("agent_foreign");
    let graph = sample_graph("agent_graph");
    store
        .upsert_graph_in_project(&graph, "default", &foreign)
        .await
        .expect("foreign graph");

    let agent = scoped_agent(&unique_id("agent_borrowing"), graph);
    let refused = store
        .upsert_agent_in_project(&agent, "default", &project)
        .await
        .expect_err("an agent cannot bind a graph its project does not own");
    assert!(matches!(refused, PostgresStoreError::ScopeMismatch));
    assert!(
        store
            .find_agent_in_project(agent.id(), "default", &project)
            .await
            .expect("find")
            .is_none(),
        "a refused agent must not leave a partial row"
    );
}

#[tokio::test]
async fn agent_upsert_does_not_rewrite_its_graph_or_cross_projects() {
    let store = scoped_store().await;
    let project = unique_id("agent_keep");
    let graph = sample_graph("agent_keep_graph");
    store
        .upsert_graph_in_project(&graph, "default", &project)
        .await
        .expect("graph");

    // The agent carries a stale copy of the graph; saving it must not write that
    // copy back over what the graph's own endpoint last stored.
    let agent = scoped_agent(&unique_id("agent_keep"), graph.clone());
    let renamed = GraphDefinition::new(
        graph.id().clone(),
        "Renamed by its owner",
        graph.nodes().to_vec(),
        graph.hyperedges().to_vec(),
        graph.transition_policy().clone(),
    )
    .expect("graph");
    store
        .upsert_graph_in_project(&renamed, "default", &project)
        .await
        .expect("rename");
    store
        .upsert_agent_in_project(&agent, "default", &project)
        .await
        .expect("agent");
    let stored = store
        .find_graph_in_project(graph.id(), "default", &project)
        .await
        .expect("find")
        .expect("graph");
    assert_eq!(stored.name(), "Renamed by its owner");

    let other = unique_id("agent_keep_other");
    let other_graph = sample_graph("agent_keep_other_graph");
    store
        .upsert_graph_in_project(&other_graph, "default", &other)
        .await
        .expect("other graph");
    let takeover = scoped_agent(agent.id().as_str(), other_graph);
    let refused = store
        .upsert_agent_in_project(&takeover, "default", &other)
        .await
        .expect_err("another project must not replace the agent");
    assert!(matches!(refused, PostgresStoreError::ScopeMismatch));
    let kept = store
        .find_agent_in_project(agent.id(), "default", &project)
        .await
        .expect("find")
        .expect("the original agent survives");
    assert_eq!(kept.graph().id(), graph.id());
}

#[tokio::test]
async fn agent_listing_applies_scope_before_the_limit() {
    let store = scoped_store().await;
    let project = unique_id("agent_list");
    let graph = sample_graph("agent_list_graph");
    store
        .upsert_graph_in_project(&graph, "default", &project)
        .await
        .expect("graph");
    let owned = scoped_agent(&unique_id("agent_listed"), graph);
    store
        .upsert_agent_in_project(&owned, "default", &project)
        .await
        .expect("agent");

    let other = unique_id("agent_list_other");
    let foreign_graph = sample_graph("agent_list_foreign_graph");
    store
        .upsert_graph_in_project(&foreign_graph, "default", &other)
        .await
        .expect("foreign graph");
    store
        .upsert_agent_in_project(
            &scoped_agent(&unique_id("agent_foreign"), foreign_graph),
            "default",
            &other,
        )
        .await
        .expect("foreign agent");

    let listed = store
        .list_agents_in_project("default", &project, 1)
        .await
        .expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id(), owned.id());
}

#[tokio::test]
async fn agent_run_creation_is_scoped_and_never_overwrites() {
    let store = scoped_store().await;
    let project = unique_id("run_owner");
    let foreign = unique_id("run_foreign");
    let graph = sample_graph("run_graph");
    store
        .upsert_graph_in_project(&graph, "default", &project)
        .await
        .expect("graph");
    let agent = scoped_agent(&unique_id("run_agent"), graph);
    store
        .upsert_agent_in_project(&agent, "default", &project)
        .await
        .expect("agent");

    let run = scoped_agent_run(&unique_id("run_scoped"), &agent);
    store
        .start_agent_run_in_project(&minimal_start(&run), "default", &project)
        .await
        .expect("create");

    // The same id again is a conflict, not a reset of work already under way.
    let mut resubmitted = run.clone();
    resubmitted.state_json = r#"{"query":"overwritten"}"#.to_string();
    let conflict = store
        .start_agent_run_in_project(&minimal_start(&resubmitted), "default", &project)
        .await
        .expect_err("the id is taken");
    assert!(matches!(conflict, PostgresStoreError::AlreadyExists(_)));
    let stored = store
        .find_agent_run_in_project(&run.id, "default", &project)
        .await
        .expect("find")
        .expect("run");
    assert_eq!(stored.state_json, run.state_json);

    // Another project cannot learn the id is taken, or start this agent.
    let taken_elsewhere = store
        .start_agent_run_in_project(&minimal_start(&run), "default", &foreign)
        .await
        .expect_err("another project owns this run id");
    assert!(matches!(taken_elsewhere, PostgresStoreError::ScopeMismatch));
    let foreign_start = scoped_agent_run(&unique_id("run_foreign_start"), &agent);
    let refused = store
        .start_agent_run_in_project(&minimal_start(&foreign_start), "default", &foreign)
        .await
        .expect_err("the agent belongs to another project");
    assert!(matches!(refused, PostgresStoreError::ScopeMismatch));
    assert!(
        store
            .find_agent_run(&foreign_start.id)
            .await
            .expect("find")
            .is_none(),
        "a refused run must not leave a row"
    );
    assert!(
        store
            .find_agent_run_in_project(&run.id, "default", &foreign)
            .await
            .expect("find")
            .is_none()
    );
}

#[tokio::test]
async fn agent_run_listing_applies_scope_before_the_limit() {
    let store = scoped_store().await;
    let project = unique_id("run_list");
    let graph = sample_graph("run_list_graph");
    store
        .upsert_graph_in_project(&graph, "default", &project)
        .await
        .expect("graph");
    let agent = scoped_agent(&unique_id("run_list_agent"), graph);
    store
        .upsert_agent_in_project(&agent, "default", &project)
        .await
        .expect("agent");
    let owned = scoped_agent_run(&unique_id("run_listed"), &agent);
    store
        .start_agent_run_in_project(&minimal_start(&owned), "default", &project)
        .await
        .expect("owned");

    let other = unique_id("run_list_other");
    let foreign_graph = sample_graph("run_list_foreign_graph");
    store
        .upsert_graph_in_project(&foreign_graph, "default", &other)
        .await
        .expect("graph");
    let foreign_agent = scoped_agent(&unique_id("run_list_foreign_agent"), foreign_graph);
    store
        .upsert_agent_in_project(&foreign_agent, "default", &other)
        .await
        .expect("agent");
    store
        .start_agent_run_in_project(
            &minimal_start(&scoped_agent_run(&unique_id("run_foreign"), &foreign_agent)),
            "default",
            &other,
        )
        .await
        .expect("foreign");

    let listed = store
        .list_agent_runs_in_project("default", &project, 1)
        .await
        .expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, owned.id);
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
async fn saves_and_finds_graph_definition_when_database_is_available() {
    let database_url = database_url();
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
    let database_url = database_url();
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
    let database_url = database_url();
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
        execution: None,
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
    let database_url = database_url();
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
        execution: None,
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
