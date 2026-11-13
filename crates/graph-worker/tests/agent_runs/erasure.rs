//! Erasing what a run held, and retention.

use super::*;

// ---- erasure

/// What the project still holds, as text, from every place a run's payloads live.
async fn held_content(stack: &Stack) -> Vec<String> {
    let mut held: Vec<String> = sqlx::query_scalar(
        "SELECT content FROM ir_run_values \
         WHERE tenant_id = $1 AND project_id = $2 AND content IS NOT NULL",
    )
    .bind(&stack.tenant)
    .bind(&stack.project)
    .fetch_all(stack.store.pool())
    .await
    .expect("values read");
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT state_json FROM agent_runs WHERE tenant_id = $1 AND project_id = $2",
    )
    .bind(&stack.tenant)
    .bind(&stack.project)
    .fetch_all(stack.store.pool())
    .await
    .expect("runs read");
    held.extend(rows);
    let snapshots: Vec<String> = sqlx::query_scalar(
        "SELECT snapshots.state_json FROM agent_state_snapshots snapshots \
         JOIN agent_runs runs ON runs.id = snapshots.agent_run_id \
         WHERE runs.tenant_id = $1 AND runs.project_id = $2",
    )
    .bind(&stack.tenant)
    .bind(&stack.project)
    .fetch_all(stack.store.pool())
    .await
    .expect("snapshots read");
    held.extend(snapshots);
    held
}

#[tokio::test]
async fn erasing_a_finished_run_removes_what_it_held_and_keeps_its_record() {
    const QUERY: &str = "erase-me-question-3301";
    const ANSWER: &str = "erase-me-answer-3302";
    let stack = Stack::new(Some(ScriptedModel::answering(&[ANSWER]))).await;
    let agent = answering_agent(&stack).await;
    let id = stack
        .start(&agent, json!({ "query": QUERY, "answer": "" }))
        .await;
    stack.run_to_rest().await;
    assert_eq!(stack.run(&id).await["state"]["answer"], ANSWER);
    let record = stack.event_kinds(&id).await;
    assert!(
        held_content(&stack)
            .await
            .iter()
            .any(|each| each.contains(QUERY)),
        "the run held its question before it was erased"
    );

    // Not from another project, which cannot even tell the run exists.
    let run_id = AgentRunId::new(id.clone()).expect("a valid id");
    assert_eq!(
        stack
            .store
            .erase_agent_run_payloads_in_project(&run_id, &stack.tenant, "another-project", "x")
            .await
            .expect("asks"),
        None
    );

    let (status, body) = stack
        .post(&format!("/v1/agent-runs/{id}/erase"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body["values_erased"].as_u64().expect("a count") >= 4,
        "{body}"
    );
    assert_eq!(body["run"]["state"], json!({}));
    assert!(
        body["run"]["execution"]["payloads_erased_at"].is_string(),
        "{body}"
    );
    assert_eq!(
        body["run"]["status"], "succeeded",
        "erasing what a run held does not change how it ended"
    );
    assert_eq!(
        stack.event_kinds(&id).await,
        record,
        "the record that it ran stays whole"
    );
    for held in held_content(&stack).await {
        assert!(
            !held.contains(QUERY) && !held.contains(ANSWER),
            "still held: {held}"
        );
    }
    let tombstones: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ir_run_values \
         WHERE tenant_id = $1 AND project_id = $2 AND erased_by IS NOT NULL",
    )
    .bind(&stack.tenant)
    .bind(&stack.project)
    .fetch_one(stack.store.pool())
    .await
    .expect("counted");
    assert!(tombstones >= 4, "each erased value says when and by whom");

    let (status, again) = stack
        .post(&format!("/v1/agent-runs/{id}/erase"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(
        again["values_erased"], 0,
        "asking again erases nothing more"
    );

    // The same bytes produced later are held again: an erasure is about what was
    // held, not a ban on a string.
    let next = stack
        .start(&agent, json!({ "query": QUERY, "answer": "" }))
        .await;
    stack.run_to_rest().await;
    let rerun = stack.run(&next).await;
    assert_eq!(rerun["status"], "succeeded", "{rerun}");
    assert_eq!(rerun["state"]["answer"], ANSWER);
    assert_eq!(
        stack.run(&id).await["state"],
        json!({}),
        "and the erased run stays erased"
    );
}

#[tokio::test]
async fn a_run_that_has_not_ended_cannot_have_its_payloads_erased() {
    let stack = Stack::new(Some(ScriptedModel::answering(&["an answer"]))).await;
    let agent = answering_agent(&stack).await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;

    let (status, body) = stack
        .post(&format!("/v1/agent-runs/{id}/erase"), json!({}))
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "agent_run_state_conflict");

    stack.run_to_rest().await;
    let run = stack.run(&id).await;
    assert_eq!(run["status"], "succeeded", "nothing it needed went: {run}");
    assert_eq!(run["state"]["answer"], "an answer");
}

#[tokio::test]
async fn bytes_another_run_holds_are_kept_for_it_and_go_when_it_is_erased() {
    const QUERY: &str = "shared-question-4417";
    let stack = Stack::new(Some(ScriptedModel::answering(&["an answer"]))).await;
    let agent = answering_agent(&stack).await;
    let finished = stack
        .start(&agent, json!({ "query": QUERY, "answer": "" }))
        .await;
    stack.run_to_rest().await;
    // Queued with exactly the same input, so it holds the same stored value.
    let live = stack
        .start(&agent, json!({ "query": QUERY, "answer": "" }))
        .await;

    let (status, body) = stack
        .post(&format!("/v1/agent-runs/{finished}/erase"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body["values_kept"].as_u64().expect("a count") >= 1,
        "{body}"
    );
    assert_eq!(body["run"]["state"], json!({}));

    // The live run never notices: what it was given is still there for it.
    stack.run_to_rest().await;
    let run = stack.run(&live).await;
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(run["state"]["query"], QUERY);
    assert!(
        held_content(&stack)
            .await
            .iter()
            .any(|each| each.contains(QUERY)),
        "held, because a run that has not been erased holds it"
    );

    let (status, body) = stack
        .post(&format!("/v1/agent-runs/{live}/erase"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for held in held_content(&stack).await {
        assert!(
            !held.contains(QUERY),
            "nobody holds it now, and it is gone: {held}"
        );
    }
}

#[tokio::test]
async fn payloads_past_retention_are_erased_and_newer_ones_kept() {
    const SHARED: &str = "retention-shared-question-7701";
    let stack = Stack::new(Some(ScriptedModel::answering(&["an answer"]))).await;
    let agent = answering_agent(&stack).await;
    let alone = stack
        .start(
            &agent,
            json!({ "query": "retention-alone-7700", "answer": "" }),
        )
        .await;
    let sharing = stack
        .start(&agent, json!({ "query": SHARED, "answer": "" }))
        .await;
    stack.run_to_rest().await;
    // Not run yet, and holding the same input as `sharing`.
    let live = stack
        .start(&agent, json!({ "query": SHARED, "answer": "" }))
        .await;
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("after the epoch")
            .as_millis(),
    )
    .expect("fits");

    // Retention has not run out for anything that ended in the last hour.
    let early = stack
        .store
        .erase_payloads_past_retention(now - 3_600_000, Some(&stack.tenant), 100)
        .await
        .expect("sweeps");
    assert_eq!(early, capsulet_postgres::PayloadRetentionSweep::default());
    assert_eq!(stack.run(&alone).await["state"]["answer"], "an answer");

    let due = stack
        .store
        .erase_payloads_past_retention(now + 3_600_000, Some(&stack.tenant), 100)
        .await
        .expect("sweeps");
    assert_eq!(
        due.runs_erased, 2,
        "both ended runs are past retention: {due:?}"
    );
    assert!(due.values_erased >= 4, "{due:?}");
    assert!(
        due.values_kept >= 1,
        "what the live run holds stays: {due:?}"
    );

    for erased in [&alone, &sharing] {
        let run = stack.run(erased).await;
        assert_eq!(run["state"], json!({}));
        assert!(run["execution"]["payloads_erased_at"].is_string());
    }
    let by: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT erased_by FROM ir_run_values \
         WHERE tenant_id = $1 AND project_id = $2 AND erased_by IS NOT NULL",
    )
    .bind(&stack.tenant)
    .bind(&stack.project)
    .fetch_all(stack.store.pool())
    .await
    .expect("read");
    assert_eq!(by, vec![capsulet_postgres::RETENTION_ERASER.to_string()]);
    assert_eq!(
        stack.run(&live).await["status"],
        "queued",
        "a run that has not ended is never swept"
    );

    // The live run runs as if nothing happened, and is swept once it is past retention.
    stack.run_to_rest().await;
    assert_eq!(stack.run(&live).await["state"]["query"], SHARED);
    let next = stack
        .store
        .erase_payloads_past_retention(now + 3_600_000, Some(&stack.tenant), 100)
        .await
        .expect("sweeps");
    assert_eq!(next.runs_erased, 1, "{next:?}");
    for held in held_content(&stack).await {
        assert!(!held.contains(SHARED), "still held: {held}");
    }
}
