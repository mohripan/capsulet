//! The run queue: leases, pool limits, adopting expired leases, and its metrics.

use super::*;

#[tokio::test]
async fn prometheus_metrics_include_queue_slo_gauges_when_database_is_available() {
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
    let database_url = database_url();
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
async fn stale_worker_cannot_finalize_attempt_adopted_by_another_worker_when_database_is_available()
{
    let database_url = database_url();
    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect");
    store.migrate().await.expect("migrate");
    let definition = JobDefinition::hello_python();
    store
        .upsert_job_definition(&definition)
        .await
        .expect("definition");
    let pool = unique_id("finalize_owner_pool");
    let run = JobRun::new(
        JobRunId::new(unique_id("finalize_owner_run")).expect("run id"),
        definition.id().clone(),
        ExecutionPoolName::new(pool.clone()).expect("pool"),
    );
    store.save(&run).await.expect("save");

    let mut running = store
        .lease_next_queued_run_with_pool_limits("worker-a", 60, &[(pool.clone(), 1)])
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
        .lease_next_queued_run_with_pool_limits_and_reattach("worker-b", 60, &[(pool, 1)], true)
        .await
        .expect("adopt")
        .expect("running run");

    let stale_finish = store
        .finish_running_attempt(
            running.id(),
            running.attempt_count(),
            "worker-a",
            JobRunStatus::Succeeded,
            None,
        )
        .await
        .expect("reject stale finalization");
    assert!(stale_finish.is_none());

    let current_finish = store
        .finish_running_attempt(
            adopted.id(),
            adopted.attempt_count(),
            "worker-b",
            JobRunStatus::Succeeded,
            None,
        )
        .await
        .expect("finalize adopted attempt")
        .expect("worker-b still owns the attempt");
    assert_eq!(current_finish.status(), JobRunStatus::Succeeded);
}
