//! Gates a person decides at, and cancelling a run.

use super::*;

async fn escalating_run(stack: &Stack) -> String {
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.contains": "cited", "on_fail": "escalate" })),
            generous(),
            &["validator_pass", "human_escalation"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;
    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "waiting", "{run}");
    assert!(
        run["execution"]["waiting_on"]
            .as_str()
            .expect("what it waits on")
            .contains("human-reviewer"),
        "{run}"
    );
    id
}

#[tokio::test]
async fn approving_at_a_gate_waives_the_failure_and_the_run_finishes() {
    let model = ScriptedModel::answering(&["no sources but fine"]);
    let stack = Stack::new(Some(model)).await;
    let id = escalating_run(&stack).await;

    let (status, body) = stack
        .post(
            &format!("/v1/agent-runs/{id}/decisions"),
            json!({ "approved": true }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "succeeded", "{run}");
    assert!(
        stack
            .event_kinds(&id)
            .await
            .contains(&"resumed".to_string())
    );
    assert_decisions_replay(&stack, &id).await;
}

#[tokio::test]
async fn refusing_at_a_gate_stops_the_run() {
    let model = ScriptedModel::answering(&["no sources and wrong"]);
    let stack = Stack::new(Some(model)).await;
    let id = escalating_run(&stack).await;

    let (status, body) = stack
        .post(
            &format!("/v1/agent-runs/{id}/decisions"),
            json!({ "approved": false }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "stopped", "{run}");
    assert!(
        run["execution"]["detail"]
            .as_str()
            .expect("a reason")
            .contains("refused"),
        "{run}"
    );
    assert_decisions_replay(&stack, &id).await;
}

#[tokio::test]
async fn deciding_on_a_run_that_is_not_waiting_is_a_conflict() {
    let model = ScriptedModel::answering(&["fine"]);
    let stack = Stack::new(Some(model)).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.non_empty": "true" })),
            generous(),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;

    let (status, body) = stack
        .post(
            &format!("/v1/agent-runs/{id}/decisions"),
            json!({ "approved": true }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "agent_run_state_conflict");
}

#[tokio::test]
async fn cancelling_stops_a_run_and_a_finished_run_cannot_be_cancelled() {
    let model = ScriptedModel::answering(&["never asked"]);
    let stack = Stack::new(Some(Arc::clone(&model))).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.non_empty": "true" })),
            generous(),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;

    let (status, body) = stack
        .post(&format!("/v1/agent-runs/{id}/cancel"), json!({}))
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "cancelled", "{run}");
    assert!(
        model.prompts().is_empty(),
        "a run cancelled before it started must not spend anything"
    );

    let (status, body) = stack
        .post(&format!("/v1/agent-runs/{id}/cancel"), json!({}))
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_decisions_replay(&stack, &id).await;
}
