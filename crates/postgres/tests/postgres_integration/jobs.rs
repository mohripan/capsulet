//! Job definitions, job runs, their logs and artifacts.

use super::*;

#[tokio::test]
async fn concurrent_job_definition_creates_have_exactly_one_project_owner() {
    let store = PostgresStore::connect(&database_url())
        .await
        .expect("connect");
    store.migrate().await.expect("migrate");
    sqlx::query("INSERT INTO projects (id, tenant_id, name) VALUES ('racing-project-a', 'default', 'Race A'), ('racing-project-b', 'default', 'Race B') ON CONFLICT (id) DO NOTHING")
        .execute(store.pool()).await.expect("projects");
    let id = capsulet_core::JobDefinitionId::new(unique_id("racing_job")).expect("id");
    let definition = |name| {
        JobDefinition::new(
            id.clone(),
            name,
            "python:3.12-slim",
            vec!["python".to_string()],
            Vec::new(),
            "immutable/script.py",
            "{}",
            capsulet_core::RetryPolicy::no_retry(),
        )
        .expect("definition")
    };
    let first = definition("Project A job");
    let second = definition("Project B job");
    let (a, b) = tokio::join!(
        store.upsert_job_definition_in_project(&first, "default", "racing-project-a"),
        store.upsert_job_definition_in_project(&second, "default", "racing-project-b"),
    );
    let owner = match (a, b) {
        (Ok(()), Err(capsulet_postgres::PostgresStoreError::ScopeMismatch)) => "racing-project-a",
        (Err(capsulet_postgres::PostgresStoreError::ScopeMismatch), Ok(())) => "racing-project-b",
        outcomes => panic!("one create must succeed and one fail closed: {outcomes:?}"),
    };
    let persisted = store
        .find_job_definition_in_project(&id, "default", owner)
        .await
        .expect("load")
        .expect("winner");
    assert_eq!(
        persisted.name(),
        if owner == "racing-project-a" {
            "Project A job"
        } else {
            "Project B job"
        }
    );
}

#[tokio::test]
async fn scoped_job_definition_delete_cannot_remove_a_foreign_projects_record() {
    let store = PostgresStore::connect(&database_url())
        .await
        .expect("connect");
    store.migrate().await.expect("migrate");
    let definition = JobDefinition::new(
        capsulet_core::JobDefinitionId::new(unique_id("delete_job")).expect("id"),
        "Private job",
        "python:3.12-slim",
        vec!["python".to_string()],
        Vec::new(),
        "private.py",
        "{}",
        capsulet_core::RetryPolicy::no_retry(),
    )
    .expect("definition");
    store
        .upsert_job_definition(&definition)
        .await
        .expect("save");
    assert!(
        !store
            .delete_job_definition_in_project(definition.id(), "default", "foreign-project")
            .await
            .expect("delete")
    );
    assert!(
        !store
            .delete_job_definition_in_project(definition.id(), "foreign-tenant", "default")
            .await
            .expect("delete")
    );
    assert!(
        store
            .find_job_definition(definition.id())
            .await
            .expect("load")
            .is_some()
    );
    assert!(
        store
            .delete_job_definition_in_project(definition.id(), "default", "default")
            .await
            .expect("delete")
    );
}

#[tokio::test]
async fn scoped_job_definition_lookup_hides_foreign_tenants_and_projects() {
    let store = PostgresStore::connect(&database_url())
        .await
        .expect("connect");
    store.migrate().await.expect("migrate");
    let definition = JobDefinition::new(
        capsulet_core::JobDefinitionId::new(unique_id("lookup_job")).expect("id"),
        "Private job",
        "python:3.12-slim",
        vec!["python".to_string()],
        Vec::new(),
        "private.py",
        "{}",
        capsulet_core::RetryPolicy::no_retry(),
    )
    .expect("definition");
    store
        .upsert_job_definition(&definition)
        .await
        .expect("save");
    assert!(
        store
            .find_job_definition_in_project(definition.id(), "default", "foreign-project")
            .await
            .expect("lookup")
            .is_none()
    );
    assert!(
        store
            .find_job_definition_in_project(definition.id(), "foreign-tenant", "default")
            .await
            .expect("lookup")
            .is_none()
    );
    assert!(
        store
            .find_job_definition_in_project(definition.id(), "default", "default")
            .await
            .expect("lookup")
            .is_some()
    );
}

#[tokio::test]
async fn job_definition_listing_applies_project_scope_before_limit() {
    let store = PostgresStore::connect(&database_url())
        .await
        .expect("connect");
    store.migrate().await.expect("migrate");
    let project = unique_id("job_listing");
    sqlx::query(
        "INSERT INTO projects (id, tenant_id, name) VALUES ($1, 'default', 'Listing owner')",
    )
    .bind(&project)
    .execute(store.pool())
    .await
    .expect("project");
    let job = JobDefinition::new(
        capsulet_core::JobDefinitionId::new(unique_id("listed_job")).expect("id"),
        "Owned job",
        "python:3.12-slim",
        vec!["python".to_string()],
        Vec::new(),
        "immutable.py",
        "{}",
        capsulet_core::RetryPolicy::no_retry(),
    )
    .expect("definition");
    store
        .upsert_job_definition_in_project(&job, "default", &project)
        .await
        .expect("save");
    store
        .upsert_job_definition(&JobDefinition::hello_python())
        .await
        .expect("newer foreign row");
    let listed = store
        .list_job_definitions_in_project("default", &project, 1)
        .await
        .expect("list");
    assert_eq!(
        listed.len(),
        1,
        "foreign rows must not consume the selected project's limit"
    );
    assert_eq!(listed[0].id(), job.id());
}

#[tokio::test]
async fn job_definition_upsert_rejects_cross_project_collision_without_mutating_original() {
    let store = PostgresStore::connect(&database_url())
        .await
        .expect("connect");
    store.migrate().await.expect("migrate");
    let project = unique_id("job_owner");
    sqlx::query("INSERT INTO projects (id, tenant_id, name) VALUES ($1, 'default', 'Job owner')")
        .bind(&project)
        .execute(store.pool())
        .await
        .expect("project");
    let id = capsulet_core::JobDefinitionId::new(unique_id("scoped_job")).expect("id");
    let definition = |name| {
        JobDefinition::new(
            id.clone(),
            name,
            "python:3.12-slim",
            vec!["python".to_string()],
            Vec::new(),
            "immutable/script.py",
            "{}",
            capsulet_core::RetryPolicy::no_retry(),
        )
        .expect("definition")
    };
    store
        .upsert_job_definition_in_project(&definition("Original"), "default", &project)
        .await
        .expect("save original");
    let result = store
        .upsert_job_definition_in_project(&definition("Taken over"), "default", "default")
        .await;
    assert!(
        matches!(
            result,
            Err(capsulet_postgres::PostgresStoreError::ScopeMismatch)
        ),
        "cross-project upsert must fail closed"
    );
    assert!(matches!(
        store
            .upsert_job_definition_in_project(
                &definition("Wrong tenant"),
                "foreign-tenant",
                &project
            )
            .await,
        Err(capsulet_postgres::PostgresStoreError::ScopeMismatch)
    ));
    let persisted = store
        .find_job_definition(&id)
        .await
        .expect("load")
        .expect("exists");
    assert_eq!(persisted.name(), "Original");
    assert_eq!(
        store
            .resource_project("job_definitions", id.as_str())
            .await
            .expect("owner"),
        Some(("default".to_string(), project.clone()))
    );
    store
        .upsert_job_definition_in_project(&definition("Owner update"), "default", &project)
        .await
        .expect("same-owner update");
    assert_eq!(
        store
            .find_job_definition_in_project(&id, "default", &project)
            .await
            .expect("load update")
            .expect("exists")
            .name(),
        "Owner update"
    );
}

#[test]
fn parses_known_status() {
    assert!("queued".parse::<JobRunStatus>().is_ok());
    assert!("leased".parse::<JobRunStatus>().is_ok());
    assert!("not-real".parse::<JobRunStatus>().is_err());
}

#[tokio::test]
async fn migrates_and_persists_job_runs_when_database_is_available() {
    let database_url = database_url();

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

#[tokio::test]
async fn finds_job_definition_when_database_is_available() {
    let database_url = database_url();

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
async fn saves_and_finds_job_run_logs_when_database_is_available() {
    let database_url = database_url();

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
    let database_url = database_url();

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
