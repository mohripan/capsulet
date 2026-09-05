//! Kill the worker at every step boundary in turn, and check what survives.
//!
//! What this simulates, precisely: the worker is discarded and a new one, with
//! no shared memory, recovers the run from its log and the database alone. The
//! lease is expired first, which is what a real crash leaves behind. It is not
//! an operating-system kill — the shipped binary has no executor that can run a
//! workflow until M4 brings providers, so there is no process to kill that
//! would do anything. What it does exercise is every line of state that a real
//! crash would take with it, because the new worker shares none of it.
//!
//! The executor is deliberately *not* discarded between restarts. It stands in
//! for the world outside the run, and the world does not restart when a worker
//! does. That is what makes "the effect happened twice" observable here.

mod support;

use std::sync::Arc;

use capsulet_graph_worker::{EffectOutcome, GraphWorker, Progress, WorkerConfig};
use capsulet_ir::correctness::evidence::RecordedTime;
use capsulet_ir::effect::Idempotency;
use capsulet_postgres::PostgresStore;
use capsulet_runtime::{RunEvent, RunState, RunStatus};

use support::{
    NOW, ScriptedExecutor, TestClock, admission, chaos_definition, fixture_id, id, iteration,
    seeded_run, store,
};

/// How many decisions in the worker is killed, across the scenarios.
///
/// Twelve is past the end of the longest run this fixture produces, so the last
/// few scenarios are the uninterrupted case — which is worth running too.
const KILL_POINTS: u32 = 12;

/// Simulates the crash: the worker is dropped, and the lease it never released
/// ages out so somebody else can take the run.
async fn crash(store: &PostgresStore, run_id: &str) {
    sqlx::query("UPDATE ir_runs SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(run_id)
        .execute(store.pool())
        .await
        .expect("expire the lease");
}

#[tokio::test]
async fn a_run_survives_the_worker_dying_at_every_step_boundary() {
    let store = store().await;

    for kill_after in 1..=KILL_POINTS {
        // A keyed effect: retrying is safe precisely because the same key goes
        // back to the far side, which is the property under test.
        let definition = chaos_definition(
            &fixture_id("definition"),
            Idempotency::Keyed {
                key_source: "run_id".to_string(),
            },
        );
        let (tenant, project, run_id) = seeded_run(&store, &definition).await;
        seed_loop_history(&store, &tenant, &project, &run_id).await;

        // One executor for the whole scenario. It is the far side, and the far
        // side does not forget what it was asked to do when a worker dies.
        let executor = Arc::new(ScriptedExecutor::new());
        executor.effects(vec![
            // The first attempt is the awkward one: the request went out and
            // the answer never came back.
            EffectOutcome::Uncertain {
                detail: "the connection dropped before the response".to_string(),
            },
            EffectOutcome::Performed {
                receipt: capsulet_ir::Digest::of(b"pull request 41"),
            },
        ]);
        let clock = Arc::new(TestClock::at(NOW));

        let mut restarts = 0;
        loop {
            let worker = GraphWorker::new(
                store.clone(),
                Arc::clone(&executor) as Arc<dyn capsulet_graph_worker::Executor>,
                Arc::clone(&clock) as Arc<dyn capsulet_graph_worker::Clock>,
                WorkerConfig {
                    worker_id: format!("worker-{restarts}"),
                    tenant: Some(tenant.clone()),
                    lease_seconds: 60,
                    steps_per_lease: kill_after,
                    ..WorkerConfig::default()
                },
            );

            match worker.advance_one().await.expect("advance") {
                Progress::Ended => break,
                // A worker killed on the very step that ended the run does not
                // get to observe the ending; the next one finds nothing to
                // lease. That is a finished run, not a stalled one, and the
                // difference is worth checking rather than assuming.
                Progress::NothingToDo => {
                    let state = store
                        .load_ir_run_state(&tenant, &project, &run_id)
                        .await
                        .expect("folds");
                    assert!(
                        state.status().is_terminal(),
                        "the run stalled with work outstanding at kill point {kill_after}"
                    );
                    break;
                }
                _ => {}
            }

            crash(&store, &run_id).await;
            restarts += 1;
            assert!(
                restarts < 64,
                "the run did not converge at kill point {kill_after}"
            );
        }

        assert_run_is_intact(&store, &tenant, &project, &run_id, &definition, &executor).await;
    }
}

/// Puts two finished loop iterations into the log before the worker touches it.
///
/// The worker does not drive loop iterations yet — deciding when one begins is
/// region-execution semantics that arrives with node providers — so the loop
/// history is seeded rather than produced. What is under test here is that the
/// history survives every restart, which is a property of the log and the fold
/// and is exactly as real either way.
async fn seed_loop_history(store: &PostgresStore, tenant: &str, project: &str, run_id: &str) {
    let lease = store
        .lease_next_ir_run("setup", 60, Some(tenant), NOW)
        .await
        .expect("lease")
        .expect("available");
    let events = vec![
        RunEvent::Started { by: id("setup") },
        RunEvent::IterationStarted {
            region: id("repair-loop"),
            index: 0,
        },
        RunEvent::IterationFinished {
            region: id("repair-loop"),
            record: Box::new(iteration(0, Some(4))),
        },
        RunEvent::IterationStarted {
            region: id("repair-loop"),
            index: 1,
        },
        RunEvent::IterationFinished {
            region: id("repair-loop"),
            record: Box::new(iteration(1, Some(2))),
        },
    ];
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

/// Everything the milestone promises about a run that survived a crash.
async fn assert_run_is_intact(
    store: &PostgresStore,
    tenant: &str,
    project: &str,
    run_id: &str,
    definition: &capsulet_ir::Definition,
    executor: &ScriptedExecutor,
) {
    let events = store
        .load_ir_run_events(tenant, project, run_id)
        .await
        .expect("read the log");
    let state = RunState::fold(&events).expect("the log folds after every restart");

    assert_eq!(state.status(), RunStatus::Completed);

    // No committed state was lost: what the loop spent before the crash is
    // still spent afterwards.
    let progress = state.loop_progress(&id("repair-loop"));
    assert_eq!(progress.started, 2, "iterations a restart handed back");
    assert_eq!(progress.finished, 2);
    assert_eq!(progress.last_progress, Some(2));

    // The log is gapless, which is the other half of losing nothing.
    let positions: Vec<u64> = events.iter().map(|recorded| recorded.position).collect();
    assert_eq!(
        positions,
        (0..u64::try_from(events.len()).expect("small")).collect::<Vec<_>>(),
        "a gap would mean an event nobody can account for"
    );

    // No duplicated effect. It was performed more than once — the first attempt
    // left a question — but every attempt presented the same key, so the far
    // side saw one effect, and the run finalized it once.
    let performed = executor.effects_performed();
    assert!(!performed.is_empty());
    let keys: Vec<Option<String>> = performed.iter().map(|(_, _, key)| key.clone()).collect();
    assert!(
        keys.windows(2).all(|pair| pair[0] == pair[1]),
        "a retry that changed the key would be a second effect wearing the first one's name: \
         {keys:?}"
    );
    assert_eq!(
        events
            .iter()
            .filter(|recorded| recorded.event.as_str() == "effect_finalized")
            .count(),
        1,
        "the run recorded the effect as having happened exactly once"
    );

    // The ledger agrees with the log about what is outstanding.
    assert!(state.outstanding_effects().is_empty());
    let claims = store
        .ir_effect_claims_for_run(tenant, project, run_id)
        .await
        .expect("read the ledger");
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].outcome, "finalized");

    // And the certificate assembled from that log replays offline.
    let evidence = capsulet_runtime::certify::evidence_of(
        definition,
        admission(definition),
        &id("chaos-run"),
        &state,
        &events,
        vec![],
    );
    let certificate = capsulet_kernel::workflow::certify(capsulet_kernel::workflow::Assembly {
        id: id("chaos-certificate"),
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

    let outcome =
        capsulet_kernel::replay::replay(&certificate, &capsulet_kernel::replay::EvidenceMap::new());
    assert!(
        matches!(
            outcome,
            capsulet_kernel::replay::ReplayOutcome::Reproduced { .. }
        ),
        "a certificate that does not replay is a record of nothing: {outcome:?}"
    );
}
