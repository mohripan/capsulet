use std::time::{SystemTime, UNIX_EPOCH};

use capsulet_core::{
    ArtifactId, ArtifactObjectKind, Automation, AutomationId, AutomationSettings, AutomationStatus,
    AutomationTriggerKind, ExecutionPoolName, JobArtifact, JobDefinition, JobRun, JobRunId,
    JobRunLog, JobRunLogRepository, JobRunRepository, JobRunTransition, WorkflowDefinition,
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

    let run = JobRun::new(
        JobRunId::new(unique_id("run_persistence_test")).expect("valid run id"),
        definition.id().clone(),
        ExecutionPoolName::new("mini").expect("valid pool"),
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
        .lease_next_queued_run("worker-test", 60)
        .await
        .expect("lease next run")
        .expect("queued run available");

    assert_eq!(leased.id(), run.id());
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

    let run = JobRun::new(
        JobRunId::new(unique_id("run_lease_test")).expect("valid run id"),
        definition.id().clone(),
        ExecutionPoolName::new("mini").expect("valid pool"),
    );
    store.save(&run).await.expect("save run");

    let first = store
        .lease_next_queued_run("worker-a", 60)
        .await
        .expect("lease first")
        .expect("run available");
    let second = store
        .lease_next_queued_run("worker-b", 60)
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
async fn saves_and_finds_interval_automation_when_database_is_available() {
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
        AutomationSettings::new(
            AutomationStatus::Enabled,
            AutomationTriggerKind::Interval,
            Some(30),
        ),
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

    assert_eq!(persisted.interval_seconds(), Some(30));
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
