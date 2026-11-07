//! What a worker owes real work: a lease that outlasts it, the values it needs, and
//! attention when somebody asks it to stop.

mod support;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use capsulet_graph_worker::{
    Clock, CompensationRequest, EffectOutcome, EffectRequest, Executor, GraphWorker, NodeOutcome,
    NodeRequest, Progress, WorkerConfig,
};
use capsulet_ir::Digest;
use capsulet_ir::loop_region::FailureKind;
use capsulet_runtime::{RunStatus, wait};

use support::{
    NOW, ScriptedExecutor, TestClock, fixture_id, id, pipeline_definition, seeded_run, store,
};

fn config(worker_id: &str, tenant: &str, lease_seconds: i64) -> WorkerConfig {
    WorkerConfig {
        worker_id: worker_id.to_string(),
        tenant: Some(tenant.to_string()),
        lease_seconds,
        steps_per_lease: 32,
        ..WorkerConfig::default()
    }
}

/// A node that takes longer than the lease it runs under.
struct SlowExecutor {
    inner: Arc<ScriptedExecutor>,
    pause: Duration,
}

#[async_trait]
impl Executor for SlowExecutor {
    async fn run_node(&self, request: NodeRequest<'_>) -> NodeOutcome {
        tokio::time::sleep(self.pause).await;
        self.inner.run_node(request).await
    }

    async fn perform_effect(&self, request: EffectRequest<'_>) -> EffectOutcome {
        self.inner.perform_effect(request).await
    }

    async fn compensate(&self, request: CompensationRequest<'_>) -> EffectOutcome {
        self.inner.compensate(request).await
    }
}

#[tokio::test]
async fn a_node_slower_than_its_lease_is_not_run_twice() {
    let store = store().await;
    let definition = pipeline_definition(&fixture_id("definition"));
    let (tenant, _project, _run_id) = seeded_run(&store, &definition).await;

    let scripted = Arc::new(ScriptedExecutor::new());
    let clock = Arc::new(TestClock::at(NOW));
    let slow = GraphWorker::new(
        store.clone(),
        Arc::new(SlowExecutor {
            inner: Arc::clone(&scripted),
            pause: Duration::from_millis(2_500),
        }),
        Arc::clone(&clock) as Arc<dyn Clock>,
        config("worker-slow", &tenant, 1),
    );
    let working = tokio::spawn(async move { slow.advance_one().await });

    // Keep trying to take the run for longer than the lease lasts. Without a
    // heartbeat during the work, this succeeds and the node runs a second time.
    let rival = GraphWorker::new(
        store.clone(),
        Arc::clone(&scripted) as Arc<dyn Executor>,
        Arc::clone(&clock) as Arc<dyn Clock>,
        config("worker-rival", &tenant, 60),
    );
    let mut took_it = false;
    while !working.is_finished() {
        tokio::time::sleep(Duration::from_millis(200)).await;
        if !working.is_finished()
            && rival.advance_one().await.expect("advance") != Progress::NothingToDo
        {
            took_it = true;
        }
    }

    assert!(!took_it, "a live worker's lease was taken from under it");
    assert_eq!(
        working.await.expect("the task ran").expect("advance"),
        Progress::Ended
    );
    assert_eq!(
        scripted.nodes_run(),
        vec!["normalize".to_string(), "summarize".to_string()],
        "each node ran once"
    );
}

/// Records what each node was told feeds it.
#[derive(Default)]
struct Recording {
    seen: Mutex<Vec<(String, Vec<Option<Digest>>)>>,
}

#[async_trait]
impl Executor for Recording {
    async fn run_node(&self, request: NodeRequest<'_>) -> NodeOutcome {
        for (port, input) in request.inputs {
            self.seen.lock().expect("not poisoned").push((
                format!("{}.{port}", request.node.id),
                input.sources.iter().map(|source| source.digest).collect(),
            ));
        }
        NodeOutcome::finished(BTreeMap::from([(
            "clean".to_string(),
            Digest::of(b"the cleaned text"),
        )]))
    }

    async fn perform_effect(&self, request: EffectRequest<'_>) -> EffectOutcome {
        EffectOutcome::Failed {
            failure: FailureKind::VerifierUnavailable,
            detail: format!("{} is not expected here", request.effect.id),
        }
    }

    async fn compensate(&self, request: CompensationRequest<'_>) -> EffectOutcome {
        EffectOutcome::Failed {
            failure: FailureKind::VerifierUnavailable,
            detail: format!("{} is not expected here", request.effect.id),
        }
    }
}

#[tokio::test]
async fn a_node_is_told_what_feeds_it() {
    let store = store().await;
    let definition = pipeline_definition(&fixture_id("definition"));
    let (tenant, _project, _run_id) = seeded_run(&store, &definition).await;

    let executor = Arc::new(Recording::default());
    let worker = GraphWorker::new(
        store.clone(),
        Arc::clone(&executor) as Arc<dyn Executor>,
        Arc::new(TestClock::at(NOW)) as Arc<dyn Clock>,
        config("worker-a", &tenant, 60),
    );
    assert_eq!(
        worker.advance_one().await.expect("advance"),
        Progress::Ended
    );

    assert_eq!(
        *executor.seen.lock().expect("not poisoned"),
        vec![(
            "summarize.body".to_string(),
            vec![Some(Digest::of(b"the cleaned text"))],
        )],
        "what a node produced is what the next one is told about"
    );
}

#[tokio::test]
async fn a_cancellation_signal_stops_a_run_and_is_answered_once() {
    let store = store().await;
    let definition = pipeline_definition(&fixture_id("definition"));
    let (tenant, project, run_id) = seeded_run(&store, &definition).await;

    store
        .deliver_ir_run_signal(
            &tenant,
            &project,
            &run_id,
            &wait::Signal::cancellation(id("operator")),
        )
        .await
        .expect("delivered");

    let executor = Arc::new(ScriptedExecutor::new());
    let worker = GraphWorker::new(
        store.clone(),
        Arc::clone(&executor) as Arc<dyn Executor>,
        Arc::new(TestClock::at(NOW)) as Arc<dyn Clock>,
        config("worker-a", &tenant, 60),
    );
    assert_eq!(
        worker.advance_one().await.expect("advance"),
        Progress::Ended
    );

    let state = store
        .load_ir_run_state(&tenant, &project, &run_id)
        .await
        .expect("the log folds");
    assert_eq!(state.status(), RunStatus::Cancelled);
    assert!(
        executor.nodes_run().is_empty(),
        "a run cancelled before it started does no work"
    );
    assert!(
        store
            .pending_ir_run_signals(&tenant, &project, &run_id)
            .await
            .expect("reads")
            .is_empty(),
        "the request is answered once, not on every pass"
    );
}
