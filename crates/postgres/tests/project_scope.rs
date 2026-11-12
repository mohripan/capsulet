//! Who owns what, for automations, trigger plugins, the runs they lead to, and memory.
//!
//! Ownership is part of the write. Writing a resource and then assigning its owner in a
//! second statement leaves a window where a failure strands a resource in the wrong
//! project, and a colliding id takes over somebody else's. These tests run against a
//! real database, because that is where the guarantee lives.

mod support;

use capsulet_core::{
    Automation, AutomationId, AutomationStatus, AutomationTrigger, CustomTriggerPlugin,
    ExecutionPoolName, JobDefinition, JobDefinitionId, JobRun, JobRunId, TriggerKind, TriggerName,
    WorkflowDefinition, WorkflowId, WorkflowRunId, WorkflowRunStatus, WorkflowStatus, WorkflowStep,
    WorkflowStepId,
};
use capsulet_postgres::{PostgresStore, PostgresStoreError};
use support::{fixture_id as unique_id, required_database_url as database_url, store};

async fn job(store: &PostgresStore, project: &str) -> JobDefinition {
    let job = JobDefinition::new(
        JobDefinitionId::new(unique_id("scope_job")).expect("id"),
        "A job",
        "python:3.12-slim",
        vec!["python".to_string()],
        Vec::new(),
        "job.py",
        "{}",
        capsulet_core::RetryPolicy::no_retry(),
    )
    .expect("job");
    store
        .upsert_job_definition_in_project(&job, "default", project)
        .await
        .expect("job");
    job
}

async fn workflow(store: &PostgresStore, project: &str) -> WorkflowDefinition {
    let job = job(store, project).await;
    let id = WorkflowId::new(unique_id("scope_workflow")).expect("id");
    let workflow = WorkflowDefinition::new(
        id.clone(),
        "A workflow",
        "",
        WorkflowStatus::Enabled,
        vec![WorkflowStep::new(
            WorkflowStepId::new(unique_id("scope_step")).expect("id"),
            id,
            1,
            "A step",
            job.id().clone(),
            ExecutionPoolName::new("mini").expect("pool"),
        )],
    );
    store
        .upsert_workflow_in_project(&workflow, "default", project)
        .await
        .expect("workflow");
    workflow
}

fn automation(workflow: &WorkflowDefinition, id: &str) -> Automation {
    Automation::new(
        AutomationId::new(id).expect("id"),
        "An automation",
        "",
        workflow.id().clone(),
        "{}",
        AutomationStatus::Enabled,
    )
}

fn manual_trigger(id: &AutomationId) -> AutomationTrigger {
    AutomationTrigger::new(
        id.clone(),
        TriggerName::new("manual").expect("name"),
        TriggerKind::Manual,
        "{}",
        None,
        true,
    )
}

fn plugin(id: &str) -> CustomTriggerPlugin {
    CustomTriggerPlugin::new(
        id,
        "A plugin",
        "",
        "plugin:1",
        vec!["run".to_string()],
        "{}",
    )
}

#[tokio::test]
async fn an_automation_and_its_triggers_are_written_together_with_their_owner() {
    let store = store().await;
    let project = unique_id("auto_owner");
    let workflow = workflow(&store, &project).await;
    let id = unique_id("auto");
    let automation = automation(&workflow, &id);

    store
        .save_automation_in_project(
            &automation,
            &[manual_trigger(automation.id())],
            "{}",
            "default",
            &project,
        )
        .await
        .expect("saved");

    assert_eq!(
        store
            .resource_project("automations", &id)
            .await
            .expect("reads"),
        Some(("default".to_string(), project.clone()))
    );
    let (triggers, _) = store
        .list_automation_triggers(automation.id())
        .await
        .expect("reads");
    assert_eq!(triggers.len(), 1);
}

#[tokio::test]
async fn an_automation_cannot_bind_another_projects_workflow_or_leave_a_partial_row() {
    let store = store().await;
    let workflow = workflow(&store, &unique_id("auto_foreign")).await;
    let automation = automation(&workflow, &unique_id("auto_borrow"));
    let project = unique_id("auto_owner");

    let refused = store
        .save_automation_in_project(
            &automation,
            &[manual_trigger(automation.id())],
            "{}",
            "default",
            &project,
        )
        .await
        .expect_err("the workflow belongs to another project");
    assert!(matches!(refused, PostgresStoreError::ScopeMismatch));
    assert!(
        store
            .find_automation_in_project(automation.id(), "default", &project)
            .await
            .expect("reads")
            .is_none()
    );
}

#[tokio::test]
async fn an_automation_cannot_be_taken_over_by_another_project() {
    let store = store().await;
    let owner = unique_id("auto_owner");
    let other = unique_id("auto_other");
    let id = unique_id("auto_taken");
    let original = automation(&workflow(&store, &owner).await, &id);
    store
        .save_automation_in_project(
            &original,
            &[manual_trigger(original.id())],
            "{}",
            "default",
            &owner,
        )
        .await
        .expect("saved");

    let takeover = automation(&workflow(&store, &other).await, &id);
    let refused = store
        .save_automation_in_project(&takeover, &[], "{}", "default", &other)
        .await
        .expect_err("another project owns this id");
    assert!(matches!(refused, PostgresStoreError::ScopeMismatch));

    let kept = store
        .find_automation_in_project(original.id(), "default", &owner)
        .await
        .expect("reads")
        .expect("the original survives");
    assert_eq!(kept.workflow_id(), original.workflow_id());
    let (triggers, _) = store
        .list_automation_triggers(original.id())
        .await
        .expect("reads");
    assert_eq!(triggers.len(), 1, "a refused write must not touch triggers");
}

#[tokio::test]
async fn an_automation_cannot_use_another_projects_plugin() {
    let store = store().await;
    let project = unique_id("auto_owner");
    let foreign = unique_id("auto_foreign");
    let plugin_id = unique_id("plugin_foreign");
    store
        .upsert_custom_trigger_plugin_in_project(&plugin(&plugin_id), "default", &foreign)
        .await
        .expect("plugin");

    let workflow = workflow(&store, &project).await;
    let automation = automation(&workflow, &unique_id("auto_plugin"));
    let trigger = AutomationTrigger::new(
        automation.id().clone(),
        TriggerName::new("custom").expect("name"),
        TriggerKind::Custom,
        "{}",
        Some(plugin_id),
        true,
    );

    let refused = store
        .save_automation_in_project(&automation, &[trigger], "{}", "default", &project)
        .await
        .expect_err("the plugin belongs to another project");
    assert!(matches!(refused, PostgresStoreError::ScopeMismatch));
}

#[tokio::test]
async fn automation_listing_applies_scope_before_the_limit() {
    let store = store().await;
    let project = unique_id("auto_list");
    let owned = automation(&workflow(&store, &project).await, &unique_id("auto_listed"));
    store
        .save_automation_in_project(&owned, &[], "{}", "default", &project)
        .await
        .expect("saved");
    let other = unique_id("auto_list_other");
    let newer = automation(&workflow(&store, &other).await, &unique_id("auto_newer"));
    store
        .save_automation_in_project(&newer, &[], "{}", "default", &other)
        .await
        .expect("saved");

    let listed = store
        .list_automations_in_project("default", &project, 1)
        .await
        .expect("reads");
    assert_eq!(listed.len(), 1, "foreign rows must not consume the limit");
    assert_eq!(listed[0].id(), owned.id());
}

#[tokio::test]
async fn a_trigger_plugin_cannot_be_taken_over_and_is_listed_within_its_project() {
    let store = store().await;
    let owner = unique_id("plugin_owner");
    let other = unique_id("plugin_other");
    let id = unique_id("plugin");
    store
        .upsert_custom_trigger_plugin_in_project(&plugin(&id), "default", &owner)
        .await
        .expect("plugin");

    let refused = store
        .upsert_custom_trigger_plugin_in_project(&plugin(&id), "default", &other)
        .await
        .expect_err("another project owns this id");
    assert!(matches!(refused, PostgresStoreError::ScopeMismatch));
    assert!(
        store
            .find_custom_trigger_plugin_in_project(&id, "default", &other)
            .await
            .expect("reads")
            .is_none()
    );

    store
        .upsert_custom_trigger_plugin_in_project(
            &plugin(&unique_id("plugin_newer")),
            "default",
            &other,
        )
        .await
        .expect("plugin");
    let listed = store
        .list_custom_trigger_plugins_in_project("default", &owner, 1)
        .await
        .expect("reads");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id(), id);
}

#[tokio::test]
async fn a_workflow_run_is_created_with_its_owner_or_not_at_all() {
    let store = store().await;
    let project = unique_id("wfrun_owner");
    let foreign = unique_id("wfrun_foreign");
    let workflow = workflow(&store, &project).await;

    let run_id = WorkflowRunId::new(unique_id("wfrun")).expect("id");
    store
        .create_workflow_run_in_project(workflow.id(), None, &run_id, "{}", "default", &project)
        .await
        .expect("created");
    assert_eq!(
        store
            .resource_project("workflow_runs", run_id.as_str())
            .await
            .expect("reads"),
        Some(("default".to_string(), project.clone()))
    );

    let refused_id = WorkflowRunId::new(unique_id("wfrun_refused")).expect("id");
    let refused = store
        .create_workflow_run_in_project(workflow.id(), None, &refused_id, "{}", "default", &foreign)
        .await
        .expect_err("the workflow belongs to another project");
    assert!(matches!(refused, PostgresStoreError::ScopeMismatch));
    assert!(
        store
            .find_workflow_run(&refused_id)
            .await
            .expect("reads")
            .is_none()
    );
}

#[tokio::test]
async fn a_workflow_steps_job_run_belongs_to_the_workflows_project() {
    let store = store().await;
    let project = unique_id("step_owner");
    let workflow = workflow(&store, &project).await;
    let run_id = WorkflowRunId::new(unique_id("step_run")).expect("id");
    store
        .create_workflow_run_in_project(workflow.id(), None, &run_id, "{}", "default", &project)
        .await
        .expect("created");

    store.advance_workflow_runs().await.expect("advances");

    let steps = store
        .list_workflow_step_runs(&run_id, "default", &project)
        .await
        .expect("reads");
    assert_eq!(steps.len(), 1, "the step was started");
    let job_run = steps[0].job_run_id();
    assert_eq!(
        store
            .resource_project("job_runs", job_run.as_str())
            .await
            .expect("reads"),
        Some(("default".to_string(), project)),
        "a step's logs and artifacts must not land in the default project"
    );
}

#[tokio::test]
async fn a_job_run_is_created_with_its_owner_and_never_overwrites() {
    let store = store().await;
    let project = unique_id("jobrun_owner");
    let foreign = unique_id("jobrun_foreign");
    let job = job(&store, &project).await;
    let id = JobRunId::new(unique_id("jobrun")).expect("id");
    let run = JobRun::new(
        id.clone(),
        job.id().clone(),
        ExecutionPoolName::new("mini").expect("pool"),
    );

    assert!(
        store
            .create_job_run_if_absent_in_project(&run, "default", &project)
            .await
            .expect("created"),
        "the first call creates it"
    );
    assert!(
        !store
            .create_job_run_if_absent_in_project(&run, "default", &project)
            .await
            .expect("found"),
        "the second call finds what the first one made"
    );
    let refused = store
        .create_job_run_if_absent_in_project(&run, "default", &foreign)
        .await
        .expect_err("the definition and the id belong to another project");
    assert!(matches!(refused, PostgresStoreError::ScopeMismatch));
}

#[tokio::test]
async fn run_listings_apply_scope_before_the_limit() {
    let store = store().await;
    let project = unique_id("list_owner");
    let other = unique_id("list_other");

    let owned_job = job(&store, &project).await;
    let owned = JobRunId::new(unique_id("list_jobrun")).expect("id");
    store
        .create_job_run_if_absent_in_project(
            &JobRun::new(
                owned.clone(),
                owned_job.id().clone(),
                ExecutionPoolName::new("mini").expect("pool"),
            ),
            "default",
            &project,
        )
        .await
        .expect("created");
    let owned_workflow = workflow(&store, &project).await;
    let owned_workflow_run = WorkflowRunId::new(unique_id("list_wfrun")).expect("id");
    store
        .create_workflow_run_in_project(
            owned_workflow.id(),
            None,
            &owned_workflow_run,
            "{}",
            "default",
            &project,
        )
        .await
        .expect("created");

    let foreign_job = job(&store, &other).await;
    store
        .create_job_run_if_absent_in_project(
            &JobRun::new(
                JobRunId::new(unique_id("list_jobrun_other")).expect("id"),
                foreign_job.id().clone(),
                ExecutionPoolName::new("mini").expect("pool"),
            ),
            "default",
            &other,
        )
        .await
        .expect("created");
    let foreign_workflow = workflow(&store, &other).await;
    store
        .create_workflow_run_in_project(
            foreign_workflow.id(),
            None,
            &WorkflowRunId::new(unique_id("list_wfrun_other")).expect("id"),
            "{}",
            "default",
            &other,
        )
        .await
        .expect("created");

    let runs = store
        .list_job_runs_in_project("default", &project, 1)
        .await
        .expect("reads");
    assert_eq!(runs.len(), 1, "foreign rows must not consume the limit");
    assert_eq!(runs[0].id(), &owned);

    let workflow_runs = store
        .list_workflow_runs_in_project("default", &project, 1)
        .await
        .expect("reads");
    assert_eq!(workflow_runs.len(), 1);
    assert_eq!(workflow_runs[0].id(), &owned_workflow_run);
}

#[tokio::test]
async fn another_project_can_neither_read_nor_change_a_run_by_its_id() {
    let store = store().await;
    let project = unique_id("run_owner");
    let foreign = unique_id("run_stranger");
    let workflow = workflow(&store, &project).await;
    let run_id = WorkflowRunId::new(unique_id("scoped_wfrun")).expect("id");
    store
        .create_workflow_run_in_project(workflow.id(), None, &run_id, "{}", "default", &project)
        .await
        .expect("created");
    store.advance_workflow_runs().await.expect("advances");
    let steps = store
        .list_workflow_step_runs(&run_id, "default", &project)
        .await
        .expect("reads");
    let job_run = steps[0].job_run_id().clone();

    // From another project, nothing is there, and nothing it asks for happens.
    assert!(
        store
            .find_workflow_run_in_project(&run_id, "default", &foreign)
            .await
            .expect("reads")
            .is_none()
    );
    assert_eq!(
        store
            .list_workflow_step_runs(&run_id, "default", &foreign)
            .await
            .expect("reads"),
        [] as [capsulet_core::WorkflowStepRun; 0]
    );
    assert!(
        store
            .cancel_running_workflow_run(&run_id, "default", &foreign)
            .await
            .expect("asks")
            .is_none()
    );
    assert!(
        store
            .resume_workflow_run(&run_id, "default", &foreign)
            .await
            .expect("asks")
            .is_none()
    );
    assert!(
        store
            .remove_queued_workflow_run(&run_id, "default", &foreign)
            .await
            .expect("asks")
            .is_none()
    );
    assert!(
        store
            .find_job_run_in_project(&job_run, "default", &foreign)
            .await
            .expect("reads")
            .is_none()
    );
    assert!(
        store
            .cancel_job_run_in_project(&job_run, "default", &foreign)
            .await
            .expect("asks")
            .is_none()
    );
    assert!(
        store
            .find_job_run_log_in_project(&job_run, "default", &foreign)
            .await
            .expect("reads")
            .is_none()
    );
    assert_eq!(
        store
            .list_job_artifacts_in_project(&job_run, "default", &foreign)
            .await
            .expect("reads"),
        [] as [capsulet_core::JobArtifact; 0]
    );

    // The owner sees it as it was left, and can act on it.
    let owned = store
        .find_workflow_run_in_project(&run_id, "default", &project)
        .await
        .expect("reads")
        .expect("the owner sees its run");
    assert_eq!(owned.status(), WorkflowRunStatus::Running);
    let job = store
        .find_job_run_in_project(&job_run, "default", &project)
        .await
        .expect("reads")
        .expect("the owner sees the step's job");
    assert!(
        !job.status().is_terminal(),
        "the stranger's cancel did nothing"
    );
    let cancelled = store
        .cancel_running_workflow_run(&run_id, "default", &project)
        .await
        .expect("cancels")
        .expect("the owner's run");
    assert_eq!(cancelled.status(), WorkflowRunStatus::Cancelled);
}

#[tokio::test]
async fn a_memory_record_is_found_only_in_its_own_project() {
    use capsulet_core::{Authority, MemoryScope, Source, SourceId};

    let store = store().await;
    let project = unique_id("memory_owner");
    let foreign = unique_id("memory_stranger");
    let source = Source::new(
        SourceId::new(unique_id("scoped_source")).expect("id"),
        MemoryScope::new("default", project.clone()).expect("scope"),
        "executive_update",
        None,
        "An update",
        Authority::High,
    )
    .expect("a source");
    store.upsert_memory_source(&source).await.expect("stored");

    assert!(
        store
            .find_memory_source_in_project(source.id(), "default", &project)
            .await
            .expect("reads")
            .is_some()
    );
    assert!(
        store
            .find_memory_source_in_project(source.id(), "default", &foreign)
            .await
            .expect("reads")
            .is_none(),
        "another project's record matches no row"
    );

    every_scoped_memory_lookup_runs(&store, &project).await;
}

/// Every scoped lookup is a query the database accepts: each table carries its owner,
/// so none of them can fall back to an unscoped read.
async fn every_scoped_memory_lookup_runs(store: &PostgresStore, project: &str) {
    use capsulet_core::{
        ClaimConflictId, ClaimId, EntityId, EntityResolutionId, EventId, EvidenceId,
        MemoryContractId, MemorySubgraphId, RelationshipId,
    };

    macro_rules! absent {
        ($find:ident, $id:ty) => {
            let id = <$id>::new(unique_id("nothing_here")).expect("an id");
            let found = store
                .$find(&id, "default", project)
                .await
                .expect(concat!(stringify!($find), " runs"));
            assert!(found.is_none(), stringify!($find));
        };
    }
    absent!(find_memory_evidence_in_project, EvidenceId);
    absent!(find_memory_entity_in_project, EntityId);
    absent!(find_memory_claim_in_project, ClaimId);
    absent!(find_memory_claim_conflict_in_project, ClaimConflictId);
    absent!(find_memory_event_in_project, EventId);
    absent!(find_memory_relationship_in_project, RelationshipId);
    absent!(find_memory_contract_in_project, MemoryContractId);
    absent!(find_memory_subgraph_in_project, MemorySubgraphId);
    absent!(find_memory_entity_resolution_in_project, EntityResolutionId);
}

/// One project's memory: a claim with its evidence and subject, a canonical entity, and
/// a draft subgraph, so a write that names one of them has a real row to name.
struct ProjectMemory {
    scope: capsulet_core::MemoryScope,
    evidence: capsulet_core::EvidenceId,
    entity: capsulet_core::EntityId,
    claim: capsulet_core::ClaimId,
    canonical: capsulet_core::CanonicalEntityId,
    subgraph: capsulet_core::MemorySubgraphId,
}

async fn project_memory(store: &PostgresStore, tenant: &str, project: &str) -> ProjectMemory {
    use capsulet_core::{
        Authority, CanonicalEntity, CanonicalEntityId, Claim, ClaimId, ClaimStatus, Confidence,
        Entity, EntityId, Evidence, EvidenceId, MemoryScope, Source, SourceId,
    };

    let scope = MemoryScope::new(tenant, project).expect("scope");
    let source = Source::new(
        SourceId::new(unique_id("mem_source")).expect("id"),
        scope.clone(),
        "notes",
        None,
        "Notes",
        Authority::High,
    )
    .expect("source");
    store.upsert_memory_source(&source).await.expect("source");
    let evidence = Evidence::new(
        EvidenceId::new(unique_id("mem_evidence")).expect("id"),
        scope.clone(),
        source.id().clone(),
        "notes.md#L1",
        "Atlas ships in August.",
        "2026-11-08T00:00:00Z",
    )
    .expect("evidence");
    store
        .upsert_memory_evidence(&evidence)
        .await
        .expect("evidence");
    let entity = Entity::new(
        EntityId::new(unique_id("mem_entity")).expect("id"),
        scope.clone(),
        "Project",
        "Atlas",
        Vec::new(),
    )
    .expect("entity");
    store.upsert_memory_entity(&entity).await.expect("entity");
    let claim = Claim::new(
        ClaimId::new(unique_id("mem_claim")).expect("id"),
        scope.clone(),
        entity.id().clone(),
        "summary",
        "Atlas ships in August.",
        vec![evidence.id().clone()],
        Confidence::new(0.9).expect("confidence"),
        Authority::High,
        "2026-11-08T00:00:00Z",
        None,
        None,
    )
    .expect("claim")
    .with_status(ClaimStatus::Active);
    store.upsert_memory_claim(&claim).await.expect("claim");
    let canonical = CanonicalEntity::new(
        CanonicalEntityId::new(unique_id("mem_canonical")).expect("id"),
        scope.clone(),
        "Project",
        "Atlas",
        Vec::new(),
    )
    .expect("canonical");
    store
        .upsert_memory_canonical_entity(&canonical)
        .await
        .expect("canonical");
    let subgraph = draft_subgraph(&scope, None);
    store
        .upsert_memory_subgraph(&subgraph)
        .await
        .expect("subgraph");
    ProjectMemory {
        evidence: evidence.id().clone(),
        entity: entity.id().clone(),
        claim: claim.id().clone(),
        canonical: canonical.id().clone(),
        subgraph: subgraph.id().clone(),
        scope,
    }
}

fn draft_subgraph(
    scope: &capsulet_core::MemoryScope,
    parent: Option<capsulet_core::MemorySubgraphId>,
) -> capsulet_core::MemorySubgraph {
    capsulet_core::MemorySubgraph::draft(
        capsulet_core::MemorySubgraphId::new(unique_id("mem_subgraph")).expect("id"),
        scope.clone(),
        parent,
        "Sales memory",
        None,
    )
    .expect("draft")
}

fn subgraph_member(
    memory: &ProjectMemory,
    subgraph: &capsulet_core::MemorySubgraphId,
) -> capsulet_core::MemorySubgraphMember {
    use capsulet_core::{
        MemoryMemberId, MemoryMemberKind, MemorySubgraphMember, MemorySubgraphMemberId,
        MemorySubgraphMemberRole,
    };

    MemorySubgraphMember::new(
        MemorySubgraphMemberId::new(unique_id("mem_member")).expect("id"),
        memory.scope.clone(),
        subgraph.clone(),
        MemoryMemberKind::Claim,
        MemoryMemberId::new(memory.claim.as_str()).expect("id"),
        MemorySubgraphMemberRole::Summary,
    )
    .expect("member")
}

fn subgraph_edge(
    memory: &ProjectMemory,
    from: &capsulet_core::MemorySubgraphId,
    to: &capsulet_core::MemorySubgraphId,
) -> capsulet_core::SubgraphEdge {
    use capsulet_core::{MemoryMemberId, MemoryMemberKind, SubgraphEdge, SubgraphEdgeId};

    SubgraphEdge::new(
        SubgraphEdgeId::new(unique_id("mem_edge")).expect("id"),
        memory.scope.clone(),
        "contradicts",
        from.clone(),
        to.clone(),
        MemoryMemberKind::Claim,
        MemoryMemberId::new(memory.claim.as_str()).expect("id"),
        MemoryMemberKind::Claim,
        MemoryMemberId::new(memory.claim.as_str()).expect("id"),
        vec![memory.claim.clone()],
        vec![memory.evidence.clone()],
    )
    .expect("edge")
}

/// How many rows of a table carry this id, read past the store so a refused write is
/// seen to have left nothing behind.
async fn rows_with_id(table: &str, id: &str) -> i64 {
    let pool = sqlx::PgPool::connect(&database_url())
        .await
        .expect("connect to postgres");
    let count =
        sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table} WHERE id = $1"))
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("count rows");
    pool.close().await;
    count
}

#[tokio::test]
async fn memory_contract_listing_applies_scope_before_the_limit() {
    use capsulet_core::{MemoryContract, MemoryContractId, MemoryScope};

    let store = store().await;
    let project = unique_id("contract_owner");
    let contract = |scope: MemoryScope| {
        MemoryContract::parse_scoped(
            MemoryContractId::new(unique_id("scope_contract")).expect("id"),
            scope,
            "A contract",
            "entity Project:\n  fields:\n    name: string\n",
        )
        .expect("contract")
    };
    let owned = contract(MemoryScope::new("default", project.clone()).expect("scope"));
    store.upsert_memory_contract(&owned).await.expect("saved");
    // Both written later, so an unscoped listing ordered by recency puts them first.
    for scope in [
        MemoryScope::new("default", unique_id("contract_stranger")).expect("scope"),
        MemoryScope::new("other_tenant", project.clone()).expect("scope"),
    ] {
        store
            .upsert_memory_contract(&contract(scope))
            .await
            .expect("saved");
    }

    let listed = store
        .list_memory_contracts("default", &project, 1)
        .await
        .expect("reads");
    assert_eq!(listed.len(), 1, "foreign rows must not consume the limit");
    assert_eq!(listed[0].id(), owned.id());
}

#[tokio::test]
async fn a_subgraph_member_cannot_attach_to_another_projects_subgraph() {
    let store = store().await;
    let memory = project_memory(&store, "default", &unique_id("member_owner")).await;
    let stranger = project_memory(&store, "default", &unique_id("member_stranger")).await;

    let foreign = subgraph_member(&memory, &stranger.subgraph);
    let refused = store
        .upsert_memory_subgraph_member(&foreign)
        .await
        .expect_err("the subgraph belongs to another project");
    assert!(matches!(refused, PostgresStoreError::ScopeMismatch));
    assert_eq!(
        rows_with_id("memory_subgraph_members", foreign.id().as_str()).await,
        0,
        "a refused member leaves no row"
    );

    store
        .upsert_memory_subgraph_member(&subgraph_member(&memory, &memory.subgraph))
        .await
        .expect("a member of the project's own subgraph");
    let other_tenant = project_memory(&store, "other_tenant", memory.scope.project_id()).await;
    let refused = store
        .upsert_memory_subgraph_member(&subgraph_member(&memory, &other_tenant.subgraph))
        .await
        .expect_err("the same project name in another tenant is another project");
    assert!(matches!(refused, PostgresStoreError::ScopeMismatch));
}

#[tokio::test]
async fn a_subgraph_edge_cannot_reach_another_projects_subgraph() {
    let store = store().await;
    let memory = project_memory(&store, "default", &unique_id("edge_owner")).await;
    let stranger = project_memory(&store, "default", &unique_id("edge_stranger")).await;
    let second = draft_subgraph(&memory.scope, None);
    store
        .upsert_memory_subgraph(&second)
        .await
        .expect("a second subgraph");

    for (from, to) in [
        (&memory.subgraph, &stranger.subgraph),
        (&stranger.subgraph, &memory.subgraph),
    ] {
        let edge = subgraph_edge(&memory, from, to);
        let refused = store
            .upsert_memory_subgraph_edge(&edge)
            .await
            .expect_err("one end belongs to another project");
        assert!(matches!(refused, PostgresStoreError::ScopeMismatch));
        assert_eq!(
            rows_with_id("memory_subgraph_edges", edge.id().as_str()).await,
            0,
            "a refused edge leaves no row"
        );
    }

    store
        .upsert_memory_subgraph_edge(&subgraph_edge(&memory, &memory.subgraph, second.id()))
        .await
        .expect("an edge between the project's own subgraphs");
}

#[tokio::test]
async fn nothing_else_names_another_projects_subgraph() {
    use capsulet_core::{
        Confidence, EntityGraphAttachment, EntityGraphAttachmentId, EntityGraphAttachmentType,
        EntityResolution, EntityResolutionId, EntityResolutionStatus, SummaryTrace, SummaryTraceId,
    };

    let store = store().await;
    let memory = project_memory(&store, "default", &unique_id("child_owner")).await;
    let stranger = project_memory(&store, "default", &unique_id("child_stranger")).await;
    let foreign = &stranger.subgraph;

    let child = draft_subgraph(&memory.scope, Some(foreign.clone()));
    let refused = store
        .upsert_memory_subgraph(&child)
        .await
        .expect_err("a child subgraph under another project's subgraph");
    assert!(matches!(refused, PostgresStoreError::ScopeMismatch));
    assert_eq!(
        rows_with_id("memory_subgraphs", child.id().as_str()).await,
        0
    );

    let trace = SummaryTrace::new(
        SummaryTraceId::new(unique_id("mem_trace")).expect("id"),
        memory.scope.clone(),
        foreign.clone(),
        memory.claim.clone(),
        Vec::new(),
        vec![memory.evidence.clone()],
    )
    .expect("trace");
    let refused = store
        .upsert_memory_summary_trace(&trace)
        .await
        .expect_err("a summary trace for another project's subgraph");
    assert!(matches!(refused, PostgresStoreError::ScopeMismatch));
    assert_eq!(
        rows_with_id("memory_summary_traces", trace.id().as_str()).await,
        0
    );

    let resolution = EntityResolution::new(
        EntityResolutionId::new(unique_id("mem_resolution")).expect("id"),
        memory.scope.clone(),
        foreign.clone(),
        memory.entity.clone(),
        memory.canonical.clone(),
        Confidence::new(0.9).expect("confidence"),
        EntityResolutionStatus::Candidate,
        vec![memory.evidence.clone()],
    )
    .expect("resolution");
    let refused = store
        .upsert_memory_entity_resolution(&resolution)
        .await
        .expect_err("an entity resolution in another project's subgraph");
    assert!(matches!(refused, PostgresStoreError::ScopeMismatch));
    assert_eq!(
        rows_with_id("memory_entity_resolutions", resolution.id().as_str()).await,
        0
    );

    let attachment = EntityGraphAttachment::new(
        EntityGraphAttachmentId::new(unique_id("mem_attachment")).expect("id"),
        memory.scope.clone(),
        memory.canonical.clone(),
        foreign.clone(),
        EntityGraphAttachmentType::Supporting,
    )
    .expect("attachment");
    let refused = store
        .upsert_memory_entity_graph_attachment(&attachment)
        .await
        .expect_err("an attachment to another project's subgraph");
    assert!(matches!(refused, PostgresStoreError::ScopeMismatch));
    assert_eq!(
        rows_with_id("memory_entity_graph_attachments", attachment.id().as_str()).await,
        0
    );

    store
        .upsert_memory_subgraph(&draft_subgraph(
            &memory.scope,
            Some(memory.subgraph.clone()),
        ))
        .await
        .expect("a child of the project's own subgraph");
}

#[tokio::test]
async fn summary_traces_are_listed_within_their_project() {
    use capsulet_core::{SummaryTrace, SummaryTraceId};

    let store = store().await;
    let memory = project_memory(&store, "default", &unique_id("trace_owner")).await;
    let stranger = project_memory(&store, "default", &unique_id("trace_stranger")).await;
    let owned = SummaryTrace::new(
        SummaryTraceId::new(unique_id("mem_trace")).expect("id"),
        memory.scope.clone(),
        memory.subgraph.clone(),
        memory.claim.clone(),
        Vec::new(),
        vec![memory.evidence.clone()],
    )
    .expect("trace");
    store
        .upsert_memory_summary_trace(&owned)
        .await
        .expect("saved");
    // A trace another project wrote against this subgraph before writes were checked.
    let pool = sqlx::PgPool::connect(&database_url())
        .await
        .expect("connect to postgres");
    sqlx::query(
        r"
        INSERT INTO memory_summary_traces (
            id, tenant_id, project_id, subgraph_id, summary_claim_id, inner_claim_ids, evidence_ids
        )
        VALUES ($1, $2, $3, $4, $5, '{}', $6)
        ",
    )
    .bind(unique_id("mem_trace_planted"))
    .bind(stranger.scope.tenant_id())
    .bind(stranger.scope.project_id())
    .bind(memory.subgraph.as_str())
    .bind(memory.claim.as_str())
    .bind(vec![stranger.evidence.as_str().to_string()])
    .execute(&pool)
    .await
    .expect("planted");
    pool.close().await;

    let listed = store
        .list_memory_summary_traces(
            &memory.subgraph,
            &memory.claim,
            memory.scope.tenant_id(),
            memory.scope.project_id(),
        )
        .await
        .expect("reads");
    assert_eq!(
        listed,
        vec![owned],
        "another project's trace cannot support this project's summary"
    );
}

#[tokio::test]
async fn ingestion_run_outputs_are_listed_within_their_project() {
    use capsulet_core::{
        Authority, IngestionConnector, IngestionConnectorConfig, IngestionConnectorId,
        IngestionConnectorKind, IngestionRun, IngestionRunId, IngestionRunOutputRecord,
        MemoryScope,
    };

    let store = store().await;
    let project = unique_id("ingest_owner");
    let scope = MemoryScope::new("default", project.clone()).expect("scope");
    let connector = IngestionConnector::new(
        IngestionConnectorId::new(unique_id("scope_connector")).expect("id"),
        scope.clone(),
        "Notes",
        IngestionConnectorKind::LocalText,
        IngestionConnectorConfig::local_text(
            "Atlas notes",
            "- Atlas ships in August",
            "text/markdown",
            None,
            Authority::High,
        ),
        true,
    )
    .expect("connector");
    store
        .upsert_ingestion_connector(&connector)
        .await
        .expect("connector");
    let run = IngestionRun::queued(
        IngestionRunId::new(unique_id("scope_ingestion_run")).expect("id"),
        scope,
        connector.id().clone(),
    )
    .succeeded(1, 1, 1, 1);
    store.upsert_ingestion_run(&run).await.expect("run");
    let output =
        IngestionRunOutputRecord::new(run.id().clone(), "claim", "claim_ingested").expect("output");
    store
        .upsert_ingestion_run_output(&output)
        .await
        .expect("output");

    assert_eq!(
        store
            .list_ingestion_run_outputs(run.id(), "default", &project)
            .await
            .expect("reads"),
        vec![output]
    );
    for (tenant, other) in [
        ("default", unique_id("ingest_stranger")),
        ("other_tenant", project.clone()),
    ] {
        assert!(
            store
                .list_ingestion_run_outputs(run.id(), tenant, &other)
                .await
                .expect("reads")
                .is_empty(),
            "another project lists none of the run's outputs"
        );
    }
}
