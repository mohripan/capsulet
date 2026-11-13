//! Workflow definitions, their steps and dependencies.

use super::*;

async fn project_workflow_fixture(store: &PostgresStore, project: &str) -> WorkflowDefinition {
    let job = JobDefinition::new(
        capsulet_core::JobDefinitionId::new(unique_id("workflow_fixture_job")).expect("job id"),
        "Workflow fixture job",
        "python:3.12-slim",
        vec!["python".to_string()],
        Vec::new(),
        "fixture.py",
        "{}",
        capsulet_core::RetryPolicy::no_retry(),
    )
    .expect("job");
    store
        .upsert_job_definition_in_project(&job, "default", project)
        .await
        .expect("job");
    let id = WorkflowId::new(unique_id("workflow_fixture")).expect("id");
    WorkflowDefinition::new(
        id.clone(),
        "Scoped workflow fixture",
        "",
        WorkflowStatus::Enabled,
        vec![WorkflowStep::new(
            WorkflowStepId::new(unique_id("workflow_fixture_step")).expect("step"),
            id,
            1,
            "Fixture step",
            job.id().clone(),
            ExecutionPoolName::new("mini").expect("pool"),
        )],
    )
}

#[tokio::test]
async fn scoped_workflow_delete_preserves_foreign_records_and_removes_owned_records() {
    let store = PostgresStore::connect(&database_url())
        .await
        .expect("connect");
    store.migrate().await.expect("migrate");
    let workflow = project_workflow_fixture(&store, "default").await;
    store.upsert_workflow(&workflow).await.expect("save");
    assert!(
        !store
            .delete_workflow_in_project(workflow.id(), "default", "foreign-project")
            .await
            .expect("delete")
    );
    assert!(
        !store
            .delete_workflow_in_project(workflow.id(), "foreign-tenant", "default")
            .await
            .expect("delete")
    );
    assert!(
        store
            .find_workflow(workflow.id())
            .await
            .expect("lookup")
            .is_some()
    );
    assert!(
        store
            .delete_workflow_in_project(workflow.id(), "default", "default")
            .await
            .expect("delete")
    );
    assert!(
        store
            .find_workflow(workflow.id())
            .await
            .expect("lookup")
            .is_none()
    );
}

#[tokio::test]
async fn scoped_workflow_lookup_hides_foreign_tenants_and_projects() {
    let store = PostgresStore::connect(&database_url())
        .await
        .expect("connect");
    store.migrate().await.expect("migrate");
    let workflow = project_workflow_fixture(&store, "default").await;
    store.upsert_workflow(&workflow).await.expect("save");
    assert!(
        store
            .find_workflow_in_project(workflow.id(), "default", "foreign-project")
            .await
            .expect("lookup")
            .is_none()
    );
    assert!(
        store
            .find_workflow_in_project(workflow.id(), "foreign-tenant", "default")
            .await
            .expect("lookup")
            .is_none()
    );
    assert!(
        store
            .find_workflow_in_project(workflow.id(), "default", "default")
            .await
            .expect("lookup")
            .is_some()
    );
}

#[tokio::test]
async fn workflow_listing_applies_tenant_and_project_scope_before_limit() {
    let store = PostgresStore::connect(&database_url())
        .await
        .expect("connect");
    store.migrate().await.expect("migrate");
    let project = unique_id("workflow_list_project");
    let owned = project_workflow_fixture(&store, &project).await;
    store
        .upsert_workflow_in_project(&owned, "default", &project)
        .await
        .expect("owned");
    let foreign = project_workflow_fixture(&store, "default").await;
    store
        .upsert_workflow(&foreign)
        .await
        .expect("newer foreign");
    let listed = store
        .list_workflows_in_project("default", &project, 1)
        .await
        .expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id(), owned.id());
    assert_eq!(
        store
            .list_workflows_in_project("foreign-tenant", &project, 10)
            .await
            .expect("list"),
        [] as [capsulet_core::WorkflowDefinition; 0]
    );
}

#[tokio::test]
async fn workflow_with_a_foreign_job_reference_is_rejected_without_partial_definition() {
    let store = PostgresStore::connect(&database_url())
        .await
        .expect("connect");
    store.migrate().await.expect("migrate");
    let job = JobDefinition::new(
        capsulet_core::JobDefinitionId::new(unique_id("foreign_dependency")).expect("id"),
        "Foreign job",
        "python:3.12-slim",
        vec!["python".to_string()],
        Vec::new(),
        "immutable.py",
        "{}",
        capsulet_core::RetryPolicy::no_retry(),
    )
    .expect("definition");
    store
        .upsert_job_definition_in_project(&job, "default", "foreign-reference-project")
        .await
        .expect("job");
    let id = WorkflowId::new(unique_id("rejected_workflow")).expect("id");
    let workflow = WorkflowDefinition::new(
        id.clone(),
        "Rejected workflow",
        "",
        WorkflowStatus::Enabled,
        vec![WorkflowStep::new(
            WorkflowStepId::new(unique_id("rejected_step")).expect("step"),
            id.clone(),
            1,
            "Foreign step",
            job.id().clone(),
            ExecutionPoolName::new("mini").expect("pool"),
        )],
    );
    let result = store
        .upsert_workflow_in_project(&workflow, "default", "default")
        .await;
    assert!(matches!(
        result,
        Err(capsulet_postgres::PostgresStoreError::ScopeMismatch)
    ));
    assert!(
        store.find_workflow(&id).await.expect("load").is_none(),
        "a refused dependency must roll back the header too"
    );
}

#[tokio::test]
async fn workflow_upsert_cannot_replace_another_projects_definition_or_steps() {
    let store = PostgresStore::connect(&database_url())
        .await
        .expect("connect");
    store.migrate().await.expect("migrate");
    let job = |name| {
        JobDefinition::new(
            capsulet_core::JobDefinitionId::new(unique_id(name)).expect("id"),
            name,
            "python:3.12-slim",
            vec!["python".to_string()],
            Vec::new(),
            "immutable.py",
            "{}",
            capsulet_core::RetryPolicy::no_retry(),
        )
        .expect("definition")
    };
    let original_job = job("workflow_original_job");
    let foreign_job = job("workflow_foreign_job");
    store
        .upsert_job_definition(&original_job)
        .await
        .expect("original job");
    store
        .upsert_job_definition_in_project(&foreign_job, "default", "other-workflow-project")
        .await
        .expect("foreign job");
    let id = WorkflowId::new(unique_id("scoped_workflow")).expect("id");
    let step_id = WorkflowStepId::new(unique_id("scoped_step")).expect("step id");
    let workflow = |name, job: &JobDefinition| {
        WorkflowDefinition::new(
            id.clone(),
            name,
            "",
            WorkflowStatus::Enabled,
            vec![WorkflowStep::new(
                step_id.clone(),
                id.clone(),
                1,
                name,
                job.id().clone(),
                ExecutionPoolName::new("mini").expect("pool"),
            )],
        )
    };
    store
        .upsert_workflow_in_project(&workflow("Original", &original_job), "default", "default")
        .await
        .expect("original");
    let result = store
        .upsert_workflow_in_project(
            &workflow("Taken over", &foreign_job),
            "default",
            "other-workflow-project",
        )
        .await;
    assert!(matches!(
        result,
        Err(capsulet_postgres::PostgresStoreError::ScopeMismatch)
    ));
    let persisted = store
        .find_workflow(&id)
        .await
        .expect("load")
        .expect("exists");
    assert_eq!(persisted.name(), "Original");
    assert_eq!(persisted.steps()[0].job_definition_id(), original_job.id());
    assert_eq!(
        store
            .resource_project("workflows", id.as_str())
            .await
            .expect("owner"),
        Some(("default".to_string(), "default".to_string()))
    );
}

#[tokio::test]
async fn saves_loads_and_cascades_workflow_dependencies_when_database_is_available() {
    let database_url = database_url();
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
