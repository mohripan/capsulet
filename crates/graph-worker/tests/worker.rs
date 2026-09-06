//! The worker, against a real database.
//!
//! Every test here kills something. That is the point of the milestone: a
//! control-plane process can be stopped at any point and restarted without
//! losing committed state, duplicating a protected effect, or corrupting the
//! run's history. A worker that only works when nothing goes wrong is not a
//! durable worker, it is a script.

mod support;

use std::sync::Arc;

use capsulet_graph_worker::{EffectOutcome, GraphWorker, NodeOutcome, Progress, WorkerConfig};
use capsulet_ir::Digest;
use capsulet_ir::correctness::evidence::RecordedTime;
use capsulet_ir::effect::Idempotency;
use capsulet_ir::loop_region::FailureKind;
use capsulet_runtime::event::Wait;
use capsulet_runtime::{RunEvent, RunFailure, RunStatus, wait};

use support::{
    NOW, ScriptedExecutor, TestClock, admission, effect_definition, fixture_id, id,
    pipeline_definition, reversible_effect_definition, seeded_run, store,
};

fn graph_worker(
    store: &capsulet_postgres::PostgresStore,
    executor: &Arc<ScriptedExecutor>,
    clock: &Arc<TestClock>,
    worker_id: &str,
    tenant: &str,
) -> GraphWorker {
    GraphWorker::new(
        store.clone(),
        Arc::clone(executor) as Arc<dyn capsulet_graph_worker::Executor>,
        Arc::clone(clock) as Arc<dyn capsulet_graph_worker::Clock>,
        WorkerConfig {
            worker_id: worker_id.to_string(),
            tenant: Some(tenant.to_string()),
            lease_seconds: 60,
            steps_per_lease: 32,
            ..WorkerConfig::default()
        },
    )
}

#[tokio::test]
async fn the_worker_advances_a_run_to_completion() {
    let store = store().await;
    let definition = pipeline_definition(&fixture_id("definition"));
    let (tenant, project, run_id) = seeded_run(&store, &definition).await;

    let executor = Arc::new(ScriptedExecutor::new());
    let clock = Arc::new(TestClock::at(NOW));
    let worker = graph_worker(&store, &executor, &clock, "worker-a", &tenant);

    assert_eq!(
        worker.advance_one().await.expect("advance"),
        Progress::Ended
    );

    let state = store
        .load_ir_run_state(&tenant, &project, &run_id)
        .await
        .expect("the log folds");
    assert_eq!(state.status(), RunStatus::Completed);
    assert_eq!(
        executor.nodes_run(),
        vec!["normalize".to_string(), "summarize".to_string()],
        "dependency order comes from the decision core, not from the worker"
    );

    let record = store
        .get_ir_run(&tenant, &project, &run_id)
        .await
        .expect("read the run")
        .expect("the run exists");
    assert_eq!(record.status, "completed");
    assert_eq!(
        record.lease_owner, None,
        "a finished run does not sit under a lease until it expires"
    );
}

#[tokio::test]
async fn a_worker_killed_mid_run_is_picked_up_where_it_stopped() {
    let store = store().await;
    let definition = pipeline_definition(&fixture_id("definition"));
    let (tenant, project, run_id) = seeded_run(&store, &definition).await;

    // The first worker gets three decisions in and dies without releasing.
    let first_executor = Arc::new(ScriptedExecutor::new());
    let clock = Arc::new(TestClock::at(NOW));
    let dying = GraphWorker::new(
        store.clone(),
        Arc::clone(&first_executor) as Arc<dyn capsulet_graph_worker::Executor>,
        Arc::clone(&clock) as Arc<dyn capsulet_graph_worker::Clock>,
        WorkerConfig {
            worker_id: "worker-before-crash".to_string(),
            tenant: Some(tenant.clone()),
            lease_seconds: 60,
            steps_per_lease: 2,
            ..WorkerConfig::default()
        },
    );
    assert_eq!(
        dying.advance_one().await.expect("advance"),
        Progress::StillRunning
    );

    let midway = store
        .load_ir_run_state(&tenant, &project, &run_id)
        .await
        .expect("folds");
    assert_eq!(midway.status(), RunStatus::Running);
    assert!(!midway.has_finished(&id("summarize")));

    // Its lease expires. A second worker takes over from the log alone.
    sqlx::query("UPDATE ir_runs SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(&run_id)
        .execute(store.pool())
        .await
        .expect("expire the lease");

    let second_executor = Arc::new(ScriptedExecutor::new());
    let recovering = graph_worker(
        &store,
        &second_executor,
        &clock,
        "worker-after-crash",
        &tenant,
    );
    assert_eq!(
        recovering.advance_one().await.expect("advance"),
        Progress::Ended
    );

    let state = store
        .load_ir_run_state(&tenant, &project, &run_id)
        .await
        .expect("folds");
    assert_eq!(state.status(), RunStatus::Completed);

    let ran: Vec<String> = first_executor
        .nodes_run()
        .into_iter()
        .chain(second_executor.nodes_run())
        .collect();
    assert_eq!(
        ran,
        vec!["normalize".to_string(), "summarize".to_string()],
        "recovery re-ran nothing that had already finished"
    );
}

#[tokio::test]
async fn a_worker_that_lost_its_lease_stops_instead_of_writing() {
    let store = store().await;
    let definition = pipeline_definition(&fixture_id("definition"));
    let (tenant, project, run_id) = seeded_run(&store, &definition).await;

    let executor = Arc::new(ScriptedExecutor::new());
    // Somebody takes the run over while this worker is inside the executor:
    // after it decided, before it wrote down what it did. That is the only
    // moment a handover is interesting.
    executor.steal_lease_during_work(&store, &run_id);
    let clock = Arc::new(TestClock::at(NOW));
    let worker = graph_worker(&store, &executor, &clock, "worker-a", &tenant);

    assert_eq!(
        worker.advance_one().await.expect("advance"),
        Progress::LeaseLost,
        "a superseded worker finds out on its next append"
    );

    let kinds: Vec<&str> = store
        .load_ir_run_events(&tenant, &project, &run_id)
        .await
        .expect("read the log")
        .iter()
        .map(|recorded| recorded.event.as_str())
        .collect();
    assert_eq!(
        kinds,
        vec!["admitted", "started", "node_started"],
        "the work it did after losing the lease reached nothing"
    );

    let record = store
        .get_ir_run(&tenant, &project, &run_id)
        .await
        .expect("read the run")
        .expect("the run exists");
    assert_eq!(
        record.lease_owner.as_deref(),
        Some("thief"),
        "and it did not release a lease it no longer held"
    );
}

#[tokio::test]
async fn an_effect_is_claimed_before_it_is_performed_and_finalized_after() {
    let store = store().await;
    let definition = effect_definition(&fixture_id("definition"), Idempotency::Idempotent);
    let (tenant, project, run_id) = seeded_run(&store, &definition).await;

    let executor = Arc::new(ScriptedExecutor::new());
    executor.effects(vec![EffectOutcome::Performed {
        receipt: Digest::of(b"pull request 41"),
    }]);
    let clock = Arc::new(TestClock::at(NOW));
    let worker = graph_worker(&store, &executor, &clock, "worker-a", &tenant);

    assert_eq!(
        worker.advance_one().await.expect("advance"),
        Progress::Ended
    );

    let kinds: Vec<&str> = store
        .load_ir_run_events(&tenant, &project, &run_id)
        .await
        .expect("read the log")
        .iter()
        .map(|recorded| recorded.event.as_str())
        .collect();
    assert_eq!(
        kinds,
        vec![
            "admitted",
            "started",
            "node_started",
            "effect_claimed",
            "effect_finalized",
            "node_finished",
            "completed"
        ],
        "performing the effect is the node running, and the claim is written before the effect — \
         which is what makes a crash in between answerable"
    );

    let claims = store
        .ir_effect_claims_for_run(&tenant, &project, &run_id)
        .await
        .expect("read the ledger");
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].outcome, "finalized");
}

#[tokio::test]
async fn an_effect_whose_outcome_is_unknown_is_retried_only_when_the_ir_allows_it() {
    let store = store().await;
    let definition = effect_definition(
        &fixture_id("definition"),
        Idempotency::Keyed {
            key_source: "run_id".to_string(),
        },
    );
    let (tenant, project, run_id) = seeded_run(&store, &definition).await;

    let executor = Arc::new(ScriptedExecutor::new());
    executor.effects(vec![
        EffectOutcome::Uncertain {
            detail: "the connection dropped before the response".to_string(),
        },
        EffectOutcome::Performed {
            receipt: Digest::of(b"pull request 41"),
        },
    ]);
    let clock = Arc::new(TestClock::at(NOW));
    let worker = graph_worker(&store, &executor, &clock, "worker-a", &tenant);

    assert_eq!(
        worker.advance_one().await.expect("advance"),
        Progress::Ended
    );

    let performed = executor.effects_performed();
    assert_eq!(performed.len(), 2, "the first attempt left a question");
    assert_eq!(
        performed[0].1, performed[1].1,
        "a keyed retry keeps the attempt, so the key stays the one the far side saw"
    );
    assert_eq!(performed[0].2, performed[1].2, "and presents that same key");
    assert!(performed[0].2.is_some());

    let state = store
        .load_ir_run_state(&tenant, &project, &run_id)
        .await
        .expect("folds");
    assert_eq!(state.status(), RunStatus::Completed);
}

#[tokio::test]
async fn a_non_idempotent_effect_left_in_doubt_stops_the_run() {
    let store = store().await;
    let definition = effect_definition(&fixture_id("definition"), Idempotency::NonIdempotent);
    let (tenant, project, run_id) = seeded_run(&store, &definition).await;

    let executor = Arc::new(ScriptedExecutor::new());
    executor.effects(vec![EffectOutcome::Uncertain {
        detail: "the connection dropped before the response".to_string(),
    }]);
    let clock = Arc::new(TestClock::at(NOW));
    let worker = graph_worker(&store, &executor, &clock, "worker-a", &tenant);

    assert_eq!(
        worker.advance_one().await.expect("advance"),
        Progress::Ended
    );

    assert_eq!(
        executor.effects_performed().len(),
        1,
        "guessing that it did not happen is exactly what the declaration forbids"
    );

    let state = store
        .load_ir_run_state(&tenant, &project, &run_id)
        .await
        .expect("folds");
    assert_eq!(state.status(), RunStatus::Failed);
    assert_eq!(
        state.failure(),
        Some(&RunFailure::EffectUncertain {
            node: id("publish"),
            effect: id("open-pull-request"),
        }),
        "the run says which effect nobody can account for"
    );
}

#[tokio::test]
async fn a_node_failure_with_no_declared_route_stops_the_run() {
    let store = store().await;
    let definition = pipeline_definition(&fixture_id("definition"));
    let (tenant, project, run_id) = seeded_run(&store, &definition).await;

    let executor = Arc::new(ScriptedExecutor::new());
    executor.node(
        "normalize",
        NodeOutcome::Failed {
            failure: FailureKind::SchemaMismatch,
            detail: "the model returned prose".to_string(),
        },
    );
    let clock = Arc::new(TestClock::at(NOW));
    let worker = graph_worker(&store, &executor, &clock, "worker-a", &tenant);

    assert_eq!(
        worker.advance_one().await.expect("advance"),
        Progress::Ended
    );

    let state = store
        .load_ir_run_state(&tenant, &project, &run_id)
        .await
        .expect("folds");
    assert_eq!(state.status(), RunStatus::Failed);
    assert!(matches!(
        state.failure(),
        Some(RunFailure::Node {
            failure: FailureKind::SchemaMismatch,
            ..
        })
    ));
    assert_eq!(
        executor.nodes_run(),
        vec!["normalize".to_string()],
        "a failure nobody routed does not become a retry loop"
    );
}

#[tokio::test]
async fn a_suspended_run_is_put_down_and_picked_up_again_when_its_signal_arrives() {
    let store = store().await;
    let definition = pipeline_definition(&fixture_id("definition"));
    let (tenant, project, run_id) = seeded_run(&store, &definition).await;

    // Suspend it by hand: what the worker does with a suspended run is what is
    // under test, not what suspends it.
    let lease = store
        .lease_next_ir_run("setup", 60, Some(&tenant), NOW)
        .await
        .expect("lease")
        .expect("available");
    for event in [
        RunEvent::Started { by: id("setup") },
        wait::suspend(Wait::Event {
            name: id("review-posted"),
        }),
    ] {
        store
            .append_ir_run_event(
                &tenant,
                &project,
                &run_id,
                lease.epoch,
                &event,
                RecordedTime(NOW),
            )
            .await
            .expect("append");
    }
    store
        .release_ir_run_lease(&tenant, &project, &run_id, "setup", lease.epoch)
        .await
        .expect("release");

    let executor = Arc::new(ScriptedExecutor::new());
    let clock = Arc::new(TestClock::at(NOW));
    let worker = graph_worker(&store, &executor, &clock, "worker-a", &tenant);

    assert_eq!(
        worker.advance_one().await.expect("advance"),
        Progress::NothingToDo,
        "a run waiting on a webhook is not work"
    );

    store
        .deliver_ir_run_signal(
            &tenant,
            &project,
            &run_id,
            &wait::Signal::event(id("review-posted"), id("webhook")),
        )
        .await
        .expect("deliver");

    assert_eq!(
        worker.advance_one().await.expect("advance"),
        Progress::Ended,
        "the delivered signal made it work again, and it ran to completion"
    );
    assert_eq!(
        store
            .load_ir_run_state(&tenant, &project, &run_id)
            .await
            .expect("folds")
            .status(),
        RunStatus::Completed
    );
}

#[tokio::test]
async fn an_empty_queue_is_not_an_error() {
    let store = store().await;
    let executor = Arc::new(ScriptedExecutor::new());
    let clock = Arc::new(TestClock::at(NOW));
    let worker = graph_worker(&store, &executor, &clock, "worker-a", &fixture_id("tenant"));

    assert_eq!(
        worker.advance_one().await.expect("advance"),
        Progress::NothingToDo
    );
}

/// Appends a run of events under a fresh lease, then gives the lease back.
///
/// Used to put a run into the state a test is about without the worker having
/// been the one to put it there.
async fn seed_events(
    store: &capsulet_postgres::PostgresStore,
    tenant: &str,
    project: &str,
    run_id: &str,
    events: Vec<RunEvent>,
) {
    let lease = store
        .lease_next_ir_run("setup", 60, Some(tenant), NOW)
        .await
        .expect("lease")
        .expect("available");
    for (offset, event) in events.into_iter().enumerate() {
        store
            .append_ir_run_event(
                tenant,
                project,
                run_id,
                lease.epoch,
                &event,
                RecordedTime(NOW + i64::try_from(offset).expect("the fixture is small")),
            )
            .await
            .expect("append");
    }
    store
        .release_ir_run_lease(tenant, project, run_id, "setup", lease.epoch)
        .await
        .expect("release");
}

#[tokio::test]
async fn a_cancelled_run_undoes_what_it_published_before_it_stops() {
    let store = store().await;
    let definition = reversible_effect_definition(&fixture_id("definition"));
    let (tenant, project, run_id) = seeded_run(&store, &definition).await;

    // Published, and then asked to stop.
    seed_events(
        &store,
        &tenant,
        &project,
        &run_id,
        vec![
            RunEvent::Started { by: id("setup") },
            RunEvent::NodeStarted {
                node: id("publish"),
            },
            RunEvent::EffectClaimed {
                node: id("publish"),
                effect: id("open-pull-request"),
                attempt: 0,
                key: None,
            },
            RunEvent::EffectFinalized {
                node: id("publish"),
                effect: id("open-pull-request"),
                attempt: 0,
                receipt: Digest::of(b"pull request 41"),
            },
            RunEvent::NodeFinished {
                node: id("publish"),
                outputs: std::collections::BTreeMap::new(),
                control: std::collections::BTreeMap::new(),
            },
            capsulet_runtime::failure::request_cancellation(id("operator")),
        ],
    )
    .await;

    let executor = Arc::new(ScriptedExecutor::new());
    let clock = Arc::new(TestClock::at(NOW));
    let worker = graph_worker(&store, &executor, &clock, "worker-a", &tenant);
    assert_eq!(
        worker.advance_one().await.expect("advance"),
        Progress::Ended
    );

    assert_eq!(
        executor.compensations_performed(),
        vec![(
            "open-pull-request".to_string(),
            "close-pull-request".to_string()
        )],
        "the run took back what it published before it stopped"
    );

    let kinds: Vec<&str> = store
        .load_ir_run_events(&tenant, &project, &run_id)
        .await
        .expect("read the log")
        .iter()
        .map(|recorded| recorded.event.as_str())
        .collect();
    let compensated = kinds
        .iter()
        .position(|kind| *kind == "compensated")
        .expect("it compensated");
    let cancelled = kinds
        .iter()
        .position(|kind| *kind == "cancelled")
        .expect("it stopped");
    assert!(
        compensated < cancelled,
        "compensating after the terminal event would mean never compensating: {kinds:?}"
    );
    assert_eq!(
        store
            .load_ir_run_state(&tenant, &project, &run_id)
            .await
            .expect("folds")
            .status(),
        RunStatus::Cancelled
    );
}

#[tokio::test]
async fn a_run_nobody_cancelled_keeps_what_it_published() {
    let store = store().await;
    let definition = reversible_effect_definition(&fixture_id("definition"));
    let (tenant, project, run_id) = seeded_run(&store, &definition).await;

    let executor = Arc::new(ScriptedExecutor::new());
    let clock = Arc::new(TestClock::at(NOW));
    let worker = graph_worker(&store, &executor, &clock, "worker-a", &tenant);
    assert_eq!(
        worker.advance_one().await.expect("advance"),
        Progress::Ended
    );

    assert_eq!(executor.effects_performed().len(), 1);
    assert!(
        executor.compensations_performed().is_empty(),
        "a run that finished has nothing to take back"
    );
    assert_eq!(
        store
            .load_ir_run_state(&tenant, &project, &run_id)
            .await
            .expect("folds")
            .status(),
        RunStatus::Completed
    );
}

#[tokio::test]
async fn a_node_left_running_by_a_dead_worker_times_out_rather_than_hanging() {
    let store = store().await;
    let definition = pipeline_definition(&fixture_id("definition"));
    let (tenant, project, run_id) = seeded_run(&store, &definition).await;

    // A worker started a node and never came back.
    let lease = store
        .lease_next_ir_run("worker-that-died", 60, Some(&tenant), NOW)
        .await
        .expect("lease")
        .expect("available");
    for (offset, event) in [
        RunEvent::Started {
            by: id("worker-that-died"),
        },
        RunEvent::NodeStarted {
            node: id("normalize"),
        },
    ]
    .into_iter()
    .enumerate()
    {
        store
            .append_ir_run_event(
                &tenant,
                &project,
                &run_id,
                lease.epoch,
                &event,
                RecordedTime(NOW + i64::try_from(offset).expect("small")),
            )
            .await
            .expect("append");
    }
    sqlx::query("UPDATE ir_runs SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(&run_id)
        .execute(store.pool())
        .await
        .expect("expire the lease");

    // A long time later, somebody else picks it up.
    let executor = Arc::new(ScriptedExecutor::new());
    let clock = Arc::new(TestClock::at(NOW + 3_600_000));
    let worker = graph_worker(&store, &executor, &clock, "worker-after", &tenant);
    worker.advance_one().await.expect("advance");

    let kinds: Vec<&str> = store
        .load_ir_run_events(&tenant, &project, &run_id)
        .await
        .expect("read the log")
        .iter()
        .map(|recorded| recorded.event.as_str())
        .collect();
    assert!(
        kinds.contains(&"node_failed"),
        "the inherited node was timed out against the deadline the log records: {kinds:?}"
    );
    assert!(
        !executor.nodes_run().contains(&"normalize".to_string()),
        "and it was not started a second time on top of the first"
    );
}

#[tokio::test]
async fn a_completed_run_certifies_from_its_log_and_the_certificate_replays() {
    let store = store().await;
    let definition = pipeline_definition(&fixture_id("definition"));
    let (tenant, project, run_id) = seeded_run(&store, &definition).await;

    let executor = Arc::new(ScriptedExecutor::new());
    let clock = Arc::new(TestClock::at(NOW));
    let worker = graph_worker(&store, &executor, &clock, "worker-a", &tenant);
    assert_eq!(
        worker.advance_one().await.expect("advance"),
        Progress::Ended
    );

    // Everything the certificate says about the run comes from the log, read
    // back from storage rather than from anything the worker kept.
    let events = store
        .load_ir_run_events(&tenant, &project, &run_id)
        .await
        .expect("read the log");
    let state = capsulet_runtime::RunState::fold(&events).expect("folds");
    let evidence = capsulet_runtime::certify::evidence_of(
        &definition,
        admission(&definition),
        &id("certified-run"),
        &state,
        &events,
        vec![],
    );

    let certificate = capsulet_kernel::workflow::certify(capsulet_kernel::workflow::Assembly {
        id: id("certificate-1"),
        admission: evidence.admission.clone(),
        mode: definition.assurance,
        subject: evidence.subject.clone(),
        policy_version: "1".to_string(),
        contracts: vec![],
        verifiers: vec![],
        evidence: vec![],
        obligations: vec![],
        loops: evidence.loops.clone(),
    })
    .expect("the assembly seals");

    // Offline: no database, no network, nothing but the certificate and the
    // evidence it carries.
    let outcome =
        capsulet_kernel::replay::replay(&certificate, &capsulet_kernel::replay::EvidenceMap::new());
    assert!(
        matches!(
            outcome,
            capsulet_kernel::replay::ReplayOutcome::Reproduced { .. }
        ),
        "a certificate assembled from the log has to replay, or it is a record of nothing: \
         {outcome:?}"
    );

    assert_eq!(certificate.body().subject.run, Some(id("certified-run")));
    assert_eq!(
        certificate.body().subject.definition,
        *state.definition(),
        "the certificate points at the exact bytes the run executed"
    );
}

#[tokio::test]
async fn a_timer_signal_that_fired_early_does_not_leave_the_worker_spinning() {
    let store = store().await;
    let definition = pipeline_definition(&fixture_id("definition"));
    let (tenant, project, run_id) = seeded_run(&store, &definition).await;

    seed_events(
        &store,
        &tenant,
        &project,
        &run_id,
        vec![
            RunEvent::Started { by: id("setup") },
            wait::suspend(Wait::Timer {
                until: RecordedTime(NOW + 60_000),
            }),
        ],
    )
    .await;

    // A scheduler fires the timer a minute early.
    store
        .deliver_ir_run_signal(
            &tenant,
            &project,
            &run_id,
            &wait::Signal::timer(id("scheduler")),
        )
        .await
        .expect("deliver");

    let executor = Arc::new(ScriptedExecutor::new());
    let clock = Arc::new(TestClock::at(NOW));
    let worker = graph_worker(&store, &executor, &clock, "worker-a", &tenant);

    // The pending signal makes the run leasable, and the worker answers it
    // "not yet". If it left the signal pending the run would stay leasable and
    // the worker would take it again, and again, for the whole minute.
    assert_eq!(
        worker.advance_one().await.expect("advance"),
        Progress::Suspended
    );
    assert!(
        store
            .pending_ir_run_signals(&tenant, &project, &run_id)
            .await
            .expect("read the inbox")
            .is_empty(),
        "the early signal was answered rather than left to be re-answered forever"
    );
    assert_eq!(
        worker.advance_one().await.expect("advance"),
        Progress::NothingToDo,
        "and the run went back to waiting for its wake-up time"
    );

    // Which still arrives.
    clock.advance_to(NOW + 60_000);
    assert_eq!(
        worker.advance_one().await.expect("advance"),
        Progress::Ended,
        "the stored wake time is what resumes it, not the signal"
    );
    assert_eq!(
        store
            .load_ir_run_state(&tenant, &project, &run_id)
            .await
            .expect("folds")
            .status(),
        RunStatus::Completed
    );
}
