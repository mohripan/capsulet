//! A loop the worker actually drives.
//!
//! The rest of the runtime tests build a loop's history by hand. These do not:
//! the worker opens each iteration, runs the body, reads the continuation the
//! executor reported, and decides whether to go round again. What is under test
//! is that the loop stops for the right reason and that a restart in the middle
//! of it neither repeats an iteration nor loses one.

mod support;

use std::collections::BTreeMap;
use std::sync::Arc;

use capsulet_graph_worker::{GraphWorker, NodeOutcome, Progress, WorkerConfig};
use capsulet_ir::loop_region::StopReason;
use capsulet_runtime::event::ControlValue;
use capsulet_runtime::{RunEvent, RunStatus};

use support::{
    NOW, ScriptedExecutor, TestClock, fixture_id, id, loop_definition,
    loop_definition_with_successor, seeded_run, store,
};

fn worker_over(
    store: &capsulet_postgres::PostgresStore,
    executor: &Arc<ScriptedExecutor>,
    clock: &Arc<TestClock>,
    worker_id: &str,
    tenant: &str,
    steps: u32,
) -> GraphWorker {
    GraphWorker::new(
        store.clone(),
        Arc::clone(executor) as Arc<dyn capsulet_graph_worker::Executor>,
        Arc::clone(clock) as Arc<dyn capsulet_graph_worker::Clock>,
        WorkerConfig {
            worker_id: worker_id.to_string(),
            tenant: Some(tenant.to_string()),
            lease_seconds: 60,
            steps_per_lease: steps,
            ..WorkerConfig::default()
        },
    )
}

/// `check` says "keep going" the given number of times, then stops.
fn checker_that_stops_after(iterations: usize) -> Arc<ScriptedExecutor> {
    let executor = Arc::new(ScriptedExecutor::new());
    let mut answers = Vec::with_capacity(iterations + 1);
    for round in 0..=iterations {
        answers.push(NodeOutcome::Finished {
            outputs: BTreeMap::new(),
            control: BTreeMap::from([(
                "keep-going".to_string(),
                ControlValue::Bool {
                    value: round < iterations,
                },
            )]),
        });
    }
    executor.node_sequence("check", answers);
    executor
}

#[tokio::test]
async fn the_worker_runs_a_loop_until_its_condition_goes_false() {
    let store = store().await;
    let definition = loop_definition(&fixture_id("definition"), 5);
    let (tenant, project, run_id) = seeded_run(&store, &definition).await;

    let executor = checker_that_stops_after(2);
    let clock = Arc::new(TestClock::at(NOW));
    let worker = worker_over(&store, &executor, &clock, "worker-a", &tenant, 64);

    assert_eq!(
        worker.advance_one().await.expect("advance"),
        Progress::Ended
    );

    let state = store
        .load_ir_run_state(&tenant, &project, &run_id)
        .await
        .expect("folds");
    assert_eq!(state.status(), RunStatus::Completed);

    let progress = state.loop_progress(&id("repair-loop"));
    assert_eq!(
        progress.started, 3,
        "two rounds saying keep going, then one saying stop"
    );
    assert_eq!(progress.finished, 3);
    assert_eq!(
        progress.stopped,
        Some(StopReason::ConditionFalse),
        "the only stop reason that means the loop did what it set out to do"
    );

    // The body ran once per iteration, not once for the whole run.
    let ran: Vec<String> = executor.nodes_run();
    assert_eq!(
        ran.iter().filter(|node| *node == "check").count(),
        3,
        "the body runs once per iteration: {ran:?}"
    );

    let events = store
        .load_ir_run_events(&tenant, &project, &run_id)
        .await
        .expect("read the log");
    let kinds: Vec<&str> = events
        .iter()
        .map(|recorded| recorded.event.as_str())
        .collect();
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| **kind == "iteration_started")
            .count(),
        3
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| **kind == "iteration_finished")
            .count(),
        3
    );
    assert_eq!(
        kinds.iter().filter(|kind| **kind == "loop_stopped").count(),
        1
    );
}

#[tokio::test]
async fn a_loop_that_runs_out_of_iterations_stops_and_the_run_fails() {
    let store = store().await;
    // Two iterations allowed, and a checker that would go round forever.
    let definition = loop_definition(&fixture_id("definition"), 2);
    let (tenant, project, run_id) = seeded_run(&store, &definition).await;

    let executor = checker_that_stops_after(100);
    let clock = Arc::new(TestClock::at(NOW));
    let worker = worker_over(&store, &executor, &clock, "worker-a", &tenant, 64);

    assert_eq!(
        worker.advance_one().await.expect("advance"),
        Progress::Ended
    );

    let state = store
        .load_ir_run_state(&tenant, &project, &run_id)
        .await
        .expect("folds");
    assert_eq!(
        state.loop_progress(&id("repair-loop")).started,
        2,
        "the declared bound is the bound"
    );
    assert_eq!(
        state.status(),
        RunStatus::Failed,
        "a loop that ran out of iterations did not finish its work, and a run \
         that carried on past it would be claiming otherwise"
    );
}

#[tokio::test]
async fn a_restart_mid_loop_neither_repeats_an_iteration_nor_loses_one() {
    let store = store().await;
    let definition = loop_definition(&fixture_id("definition"), 5);
    let (tenant, project, run_id) = seeded_run(&store, &definition).await;

    // One executor across every restart: it is the world outside the run.
    let executor = checker_that_stops_after(2);
    let clock = Arc::new(TestClock::at(NOW));

    let mut restarts = 0;
    loop {
        // Two decisions per lease, so the worker dies part-way through an
        // iteration every time.
        let worker = worker_over(
            &store,
            &executor,
            &clock,
            &format!("worker-{restarts}"),
            &tenant,
            2,
        );
        match worker.advance_one().await.expect("advance") {
            Progress::Ended | Progress::NothingToDo => break,
            _ => {}
        }
        sqlx::query(
            "UPDATE ir_runs SET lease_expires_at = now() - interval '1 second' WHERE id = $1",
        )
        .bind(&run_id)
        .execute(store.pool())
        .await
        .expect("expire the lease");
        restarts += 1;
        assert!(restarts < 64, "the run did not converge");
    }

    assert!(restarts > 0, "the point of this test is the restarts");

    let state = store
        .load_ir_run_state(&tenant, &project, &run_id)
        .await
        .expect("folds");
    assert_eq!(state.status(), RunStatus::Completed);
    let progress = state.loop_progress(&id("repair-loop"));
    assert_eq!(
        progress.started, 3,
        "a restart neither repeated an iteration nor skipped one"
    );
    assert_eq!(progress.finished, 3);
    assert_eq!(
        executor
            .nodes_run()
            .iter()
            .filter(|node| *node == "check")
            .count(),
        3,
        "and the body ran exactly three times across every worker that touched it"
    );
}

#[tokio::test]
async fn a_loop_whose_continuation_nobody_reported_stops_the_run() {
    let store = store().await;
    let definition = loop_definition(&fixture_id("definition"), 5);
    let (tenant, project, run_id) = seeded_run(&store, &definition).await;

    // The executor finishes the checker without answering the question the
    // loop declared it would answer.
    let executor = Arc::new(ScriptedExecutor::new());
    executor.node("check", NodeOutcome::finished(BTreeMap::new()));
    let clock = Arc::new(TestClock::at(NOW));
    let worker = worker_over(&store, &executor, &clock, "worker-a", &tenant, 64);

    let outcome = worker.advance_one().await;

    // Either the worker refuses to write an iteration record it cannot fill in,
    // or the run fails on the missing continuation. Both are refusals; what
    // must not happen is the loop carrying on with a value nobody produced.
    match outcome {
        Ok(_) => {
            let state = store
                .load_ir_run_state(&tenant, &project, &run_id)
                .await
                .expect("folds");
            assert_eq!(state.status(), RunStatus::Failed);
            assert!(matches!(
                state.failure(),
                Some(capsulet_runtime::RunFailure::ControlMissing { .. })
            ));
        }
        Err(error) => {
            assert!(
                error.to_string().contains("reported no"),
                "the refusal should name what was missing: {error}"
            );
        }
    }

    let events = store
        .load_ir_run_events(&tenant, &project, &run_id)
        .await
        .expect("read the log");
    assert!(
        !events
            .iter()
            .any(|recorded| matches!(recorded.event, RunEvent::LoopStopped { .. })),
        "a loop nobody could evaluate must not be recorded as having stopped for a reason"
    );
}

#[tokio::test]
async fn a_node_after_a_loop_waits_for_the_loop_rather_than_for_one_iteration() {
    let store = store().await;
    let definition = loop_definition_with_successor(&fixture_id("definition"), 5);
    let (tenant, project, run_id) = seeded_run(&store, &definition).await;

    let executor = checker_that_stops_after(2);
    let clock = Arc::new(TestClock::at(NOW));
    let worker = worker_over(&store, &executor, &clock, "worker-a", &tenant, 64);

    assert_eq!(
        worker.advance_one().await.expect("advance"),
        Progress::Ended
    );
    assert_eq!(
        store
            .load_ir_run_state(&tenant, &project, &run_id)
            .await
            .expect("folds")
            .status(),
        RunStatus::Completed
    );

    let ran = executor.nodes_run();
    let summarise = ran
        .iter()
        .position(|node| node == "summarise")
        .expect("the node after the loop ran");
    assert_eq!(
        ran.iter().filter(|node| *node == "summarise").count(),
        1,
        "it runs once, not once per iteration: {ran:?}"
    );
    assert_eq!(
        ran.iter().filter(|node| *node == "check").count(),
        3,
        "the loop still went round as many times as its condition called for: {ran:?}"
    );
    assert!(
        ran.iter()
            .take(summarise)
            .filter(|node| *node == "leave")
            .count()
            == 3,
        "every iteration finished before it started: acting on one iteration's output would be \
         acting on a value the loop was still working on: {ran:?}"
    );

    // And the log agrees: the loop stopped before the node after it started.
    let kinds: Vec<&str> = store
        .load_ir_run_events(&tenant, &project, &run_id)
        .await
        .expect("read the log")
        .iter()
        .map(|recorded| recorded.event.as_str())
        .collect();
    let stopped = kinds
        .iter()
        .position(|kind| *kind == "loop_stopped")
        .expect("the loop stopped");
    let started_after: Vec<usize> = kinds
        .iter()
        .enumerate()
        .filter(|(_, kind)| **kind == "node_started")
        .map(|(at, _)| at)
        .collect();
    assert!(
        started_after.last().copied().expect("nodes started") > stopped,
        "the last node to start is the one after the loop: {kinds:?}"
    );
}
