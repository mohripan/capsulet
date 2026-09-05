//! The run event log is append-only, gapless, and fenced — enforced by the
//! database rather than by whichever worker happens to be writing.
//!
//! These are the properties recovery depends on. If a position can be taken
//! twice, two workers disagree about what happened. If an event can be edited,
//! the fold that reconstructs a run is reconstructing a fiction. If a
//! superseded worker can still write, a run has two owners and neither knows.

use std::collections::BTreeMap;

use capsulet_ir::admission::AdmissionRecord;
use capsulet_ir::capability::CapabilitySet;
use capsulet_ir::definition::{AssuranceMode, Definition};
use capsulet_ir::value::LengthBounds;
use capsulet_ir::{
    Digest, Graph, GraphBuilder, Identifier, Node, NodeKind, OutputPort, RecordedTime,
    ResourceBudget, ValueSchema, admit,
};
use capsulet_postgres::{PostgresStore, PostgresStoreError};
use capsulet_runtime::event::Wait;
use capsulet_runtime::{Epoch, RunEvent, RunState, RunStatus};

mod support;
use support::{fixture_id as unique_id, required_database_url as database_url};

fn id(value: &str) -> Identifier {
    Identifier::parse(value).expect("the fixture identifier is valid")
}

fn at(millis: i64) -> RecordedTime {
    RecordedTime(millis)
}

fn definition(name: &str) -> Definition {
    let graph = Graph::new(GraphBuilder {
        nodes: vec![Node {
            id: id("prepare"),
            name: "Prepare".to_string(),
            kind: NodeKind::PureComputation,
            inputs: vec![],
            outputs: vec![OutputPort::new(
                id("patch"),
                ValueSchema::Text {
                    length: LengthBounds::new(0, 1_024),
                },
            )],
            capabilities: vec![],
            effects: vec![],
            budget: ResourceBudget::deterministic(1_000),
            provider: None,
            sub_workflow: None,
        }],
        ..GraphBuilder::default()
    })
    .expect("the fixture graph is valid");

    Definition {
        schema_version: Definition::current_schema_version(),
        id: id(name),
        version: "1".to_string(),
        name: name.to_string(),
        assurance: AssuranceMode::Enforce,
        capabilities: CapabilitySet::empty(),
        budget: ResourceBudget::deterministic(600_000),
        graph,
        boundaries: vec![],
        contracts: vec![],
    }
}

fn admission(definition: &Definition) -> AdmissionRecord {
    admit(definition).expect("the fixture definition is admitted")
}

async fn store() -> PostgresStore {
    let store = PostgresStore::connect(&database_url())
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");
    store
}

/// A registered definition and a run of it, ready to append to.
async fn run(store: &PostgresStore) -> (String, String, String, Digest) {
    let tenant = unique_id("tenant");
    let project = unique_id("project");
    let definition = definition(&unique_id("definition"));
    let digest = store
        .insert_ir_definition_version(&tenant, &project, &definition, &admission(&definition))
        .await
        .expect("register the definition");
    let run_id = unique_id("run");

    store
        .create_ir_run(
            &tenant,
            &project,
            &run_id,
            &digest,
            AssuranceMode::Enforce,
            at(1_772_000_000_000),
        )
        .await
        .expect("create the run");

    (tenant, project, run_id, digest)
}

#[tokio::test]
async fn a_new_run_carries_the_admission_that_created_it() {
    let store = store().await;
    let (tenant, project, run_id, digest) = run(&store).await;

    let record = store
        .get_ir_run(&tenant, &project, &run_id)
        .await
        .expect("read the run")
        .expect("the run exists");
    assert_eq!(record.definition_digest, digest.to_string());
    assert_eq!(record.status, "queued");
    assert_eq!(record.epoch, Epoch(0));
    assert_eq!(record.next_position, 1, "admission took position zero");

    // The row and its first event are written together, so a run whose log does
    // not fold is not a state this store can be left in.
    let state = store
        .load_ir_run_state(&tenant, &project, &run_id)
        .await
        .expect("the log folds");
    assert_eq!(state.status(), RunStatus::Queued);
    assert_eq!(state.definition(), &digest);
}

#[tokio::test]
async fn concurrent_appends_take_distinct_positions_and_leave_no_gaps() {
    let store = store().await;
    let (tenant, project, run_id, _) = run(&store).await;

    // Sixteen writers on the same run at once. Without the row lock the append
    // takes under, two of them would compute the same next position and one
    // event would be lost — the failure this test exists to make impossible.
    let mut writers = Vec::new();
    for index in 0..16_u64 {
        let store = store.clone();
        let tenant = tenant.clone();
        let project = project.clone();
        let run_id = run_id.clone();
        writers.push(tokio::spawn(async move {
            store
                .append_ir_run_event(
                    &tenant,
                    &project,
                    &run_id,
                    Epoch(0),
                    &RunEvent::Started {
                        by: id("contending-worker"),
                    },
                    at(1_772_000_000_001 + i64::try_from(index).unwrap_or(0)),
                )
                .await
                .expect("append")
                .position
        }));
    }

    let mut positions = Vec::new();
    for writer in writers {
        positions.push(writer.await.expect("the writer finished"));
    }
    positions.sort_unstable();
    assert_eq!(
        positions,
        (1..=16).collect::<Vec<_>>(),
        "every writer took a distinct position and none was skipped"
    );

    let events = store
        .load_ir_run_events(&tenant, &project, &run_id)
        .await
        .expect("read the log");
    let stored: Vec<u64> = events.iter().map(|event| event.position).collect();
    assert_eq!(stored, (0..=16).collect::<Vec<_>>());
}

#[tokio::test]
async fn an_event_at_a_position_already_taken_is_refused() {
    let store = store().await;
    let (tenant, project, run_id, _) = run(&store).await;

    // Position zero belongs to the admission event. Writing there again would
    // rewrite history under a different name.
    let error = sqlx::query(
        r#"
        INSERT INTO ir_run_events (
            tenant_id, project_id, run_id, position, kind, payload, epoch, recorded_at
        )
        VALUES ($1, $2, $3, 0, 'started', '{"event":"started","by":"impostor"}'::jsonb, 0, 1)
        "#,
    )
    .bind(&tenant)
    .bind(&project)
    .bind(&run_id)
    .execute(store.pool())
    .await
    .expect_err("a taken position is refused");

    assert!(
        error.to_string().contains("duplicate key"),
        "the refusal should name the collision: {error}"
    );
}

#[tokio::test]
async fn a_recorded_event_cannot_be_edited_or_removed() {
    let store = store().await;
    let (tenant, project, run_id, _) = run(&store).await;

    let update = sqlx::query("UPDATE ir_run_events SET kind = 'completed' WHERE run_id = $1")
        .bind(&run_id)
        .execute(store.pool())
        .await
        .expect_err("an append-only table refuses an update");
    assert!(
        update.to_string().contains("append-only"),
        "the refusal should say why: {update}"
    );

    let delete = sqlx::query("DELETE FROM ir_run_events WHERE run_id = $1")
        .bind(&run_id)
        .execute(store.pool())
        .await
        .expect_err("an append-only table refuses a delete");
    assert!(
        delete.to_string().contains("append-only"),
        "the refusal should say why: {delete}"
    );

    let events = store
        .load_ir_run_events(&tenant, &project, &run_id)
        .await
        .expect("read the log");
    assert_eq!(events.len(), 1, "the log survived both attempts");
}

#[tokio::test]
async fn a_run_cannot_be_repointed_at_another_definition_or_deleted() {
    let store = store().await;
    let (tenant, project, run_id, digest) = run(&store).await;

    // Repointing a run would make every certificate it produces meaningless:
    // the bytes it claims to have executed would not be the bytes it executed.
    let repoint = sqlx::query("UPDATE ir_runs SET definition_digest = $2 WHERE id = $1")
        .bind(&run_id)
        .bind(Digest::of(b"some other definition").to_string())
        .execute(store.pool())
        .await
        .expect_err("identity is frozen after insert");
    assert!(
        repoint.to_string().contains("identity is frozen"),
        "the refusal should say why: {repoint}"
    );

    let rewind = sqlx::query("UPDATE ir_runs SET next_position = 0 WHERE id = $1")
        .bind(&run_id)
        .execute(store.pool())
        .await
        .expect_err("the log cannot be rewound");
    assert!(
        rewind.to_string().contains("rewound"),
        "the refusal should say why: {rewind}"
    );

    let delete = sqlx::query("DELETE FROM ir_runs WHERE id = $1")
        .bind(&run_id)
        .execute(store.pool())
        .await
        .expect_err("a run is not deletable");
    assert!(
        delete.to_string().contains("append-only"),
        "the refusal should say why: {delete}"
    );

    // A lease, by contrast, has to move. Freezing identity is not freezing the
    // row.
    sqlx::query("UPDATE ir_runs SET lease_owner = 'worker-1', epoch = epoch + 1 WHERE id = $1")
        .bind(&run_id)
        .execute(store.pool())
        .await
        .expect("the lease columns are writable");

    let record = store
        .get_ir_run(&tenant, &project, &run_id)
        .await
        .expect("read the run")
        .expect("the run still exists");
    assert_eq!(record.definition_digest, digest.to_string());
    assert_eq!(record.lease_owner.as_deref(), Some("worker-1"));
    assert_eq!(record.epoch, Epoch(1));
}

#[tokio::test]
async fn an_append_from_a_superseded_epoch_is_refused() {
    let store = store().await;
    let (tenant, project, run_id, _) = run(&store).await;

    store
        .append_ir_run_event(
            &tenant,
            &project,
            &run_id,
            Epoch(0),
            &RunEvent::Started { by: id("worker-1") },
            at(1_772_000_000_001),
        )
        .await
        .expect("the current owner may append");

    // Somebody else took the run over. Task 3 does this through a lease; the
    // fencing property is the same either way.
    sqlx::query("UPDATE ir_runs SET lease_owner = 'worker-2', epoch = 1 WHERE id = $1")
        .bind(&run_id)
        .execute(store.pool())
        .await
        .expect("reclaim the run");

    let error = store
        .append_ir_run_event(
            &tenant,
            &project,
            &run_id,
            Epoch(0),
            &RunEvent::Completed {
                outputs: BTreeMap::new(),
            },
            at(1_772_000_000_002),
        )
        .await
        .expect_err("a superseded worker cannot append");

    match error {
        PostgresStoreError::EpochSuperseded {
            attempted, current, ..
        } => {
            assert_eq!(attempted, 0);
            assert_eq!(current, 1);
        }
        other => panic!("expected a fencing refusal, got {other}"),
    }

    let events = store
        .load_ir_run_events(&tenant, &project, &run_id)
        .await
        .expect("read the log");
    assert_eq!(
        events.len(),
        2,
        "the refused append left no trace, so the log the new owner folds is the one it was handed"
    );
}

#[tokio::test]
async fn appending_to_a_run_that_does_not_exist_says_so() {
    let store = store().await;
    let error = store
        .append_ir_run_event(
            &unique_id("tenant"),
            &unique_id("project"),
            "no-such-run",
            Epoch(0),
            &RunEvent::Started { by: id("worker-1") },
            at(1_772_000_000_000),
        )
        .await
        .expect_err("there is nothing to append to");

    assert!(
        matches!(error, PostgresStoreError::RunNotFound(run) if run == "no-such-run"),
        "a missing run and a superseded epoch are different problems"
    );
}

#[tokio::test]
async fn the_stored_status_is_what_folding_the_log_says_it_is() {
    let store = store().await;
    let (tenant, project, run_id, _) = run(&store).await;

    // The status column is a cache maintained by a database trigger; the fold
    // in `capsulet-runtime` is the authority. They are two implementations of
    // one rule, so this walks a run through every transition and asserts they
    // agree at each step. If either drifts, this fails rather than a run
    // quietly reporting the wrong place.
    let journey = vec![
        RunEvent::Started { by: id("worker-1") },
        RunEvent::NodeStarted {
            node: id("prepare"),
        },
        RunEvent::NodeFinished {
            node: id("prepare"),
            outputs: BTreeMap::new(),
        },
        RunEvent::Suspended {
            wait: Wait::Event {
                name: id("review-posted"),
            },
        },
        RunEvent::Resumed {
            wait: Wait::Event {
                name: id("review-posted"),
            },
            by: id("webhook"),
        },
        RunEvent::Completed {
            outputs: BTreeMap::new(),
        },
    ];

    for (offset, event) in journey.into_iter().enumerate() {
        let millis = 1_772_000_000_001 + i64::try_from(offset).expect("the fixture is small");
        store
            .append_ir_run_event(&tenant, &project, &run_id, Epoch(0), &event, at(millis))
            .await
            .expect("append");

        let events = store
            .load_ir_run_events(&tenant, &project, &run_id)
            .await
            .expect("read the log");
        let folded = RunState::fold(&events).expect("the log folds");
        let stored = store
            .get_ir_run(&tenant, &project, &run_id)
            .await
            .expect("read the run")
            .expect("the run exists");

        assert_eq!(
            stored.status,
            folded.status().as_str(),
            "the projection disagreed with the fold after {}",
            event.as_str()
        );
        assert_eq!(stored.next_position, folded.next_position());
    }
}

#[tokio::test]
async fn stored_events_round_trip_to_the_events_that_were_written() {
    let store = store().await;
    let (tenant, project, run_id, _) = run(&store).await;

    let claimed = RunEvent::EffectClaimed {
        node: id("publish"),
        effect: id("open-pull-request"),
        attempt: 2,
        key: Some("run-7".to_string()),
    };
    let written = store
        .append_ir_run_event(
            &tenant,
            &project,
            &run_id,
            Epoch(0),
            &claimed,
            at(1_772_000_000_005),
        )
        .await
        .expect("append");

    let events = store
        .load_ir_run_events(&tenant, &project, &run_id)
        .await
        .expect("read the log");
    assert_eq!(
        events.last().expect("the log is not empty"),
        &written,
        "what comes back is what went in, down to the idempotency key"
    );
}
