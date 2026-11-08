//! Who owns what, for automations, trigger plugins, and the runs they lead to.
//!
//! Ownership is part of the write. Writing a resource and then assigning its owner in a
//! second statement leaves a window where a failure strands a resource in the wrong
//! project, and a colliding id takes over somebody else's. These tests run against a
//! real database, because that is where the guarantee lives.

mod support;

use capsulet_core::{
    Automation, AutomationId, AutomationStatus, AutomationTrigger, CustomTriggerPlugin,
    ExecutionPoolName, JobDefinition, JobDefinitionId, JobRun, JobRunId, TriggerKind, TriggerName,
    WorkflowDefinition, WorkflowId, WorkflowRunId, WorkflowStatus, WorkflowStep, WorkflowStepId,
};
use capsulet_postgres::{PostgresStore, PostgresStoreError};
use support::{fixture_id as unique_id, required_database_url as database_url};

async fn store() -> PostgresStore {
    let store = PostgresStore::connect(&database_url())
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");
    store
}

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

    let steps = store.list_workflow_step_runs(&run_id).await.expect("reads");
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
