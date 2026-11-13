//! How a run executes: checks and retries, budgets, models, and the limits it is refused at.

use super::*;

#[tokio::test]
async fn a_run_started_over_http_is_executed_by_the_worker() {
    let model = ScriptedModel::answering(&["Capsulet checks things."]);
    let stack = Stack::new(Some(Arc::clone(&model))).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.non_empty": "true" })),
            generous(),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(
            &agent,
            json!({ "query": "  what is\n capsulet? ", "answer": "" }),
        )
        .await;

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(run["state"]["answer"], "Capsulet checks things.");
    assert_eq!(
        run["state"]["query"], "  what is\n capsulet? ",
        "what the run was given stays what it was given"
    );
    assert_eq!(
        run["assurance"], "unverified",
        "finishing is not the same as being right"
    );
    assert_eq!(run["execution"]["usage"]["tokens"], 10);
    assert_eq!(run["execution"]["ir_run_id"], id);
    assert!(run["state_version"].as_u64().expect("a count") >= 4);

    assert_eq!(
        model.prompts(),
        vec!["Q: what is capsulet? Last: ".to_string()],
        "the normalized query reached the model through the template"
    );

    let kinds = stack.event_kinds(&id).await;
    assert_eq!(kinds[0], "admitted");
    assert_eq!(kinds[1], "inputs_bound");
    assert!(kinds.contains(&"resources_consumed".to_string()));
    assert_eq!(kinds.last().map(String::as_str), Some("completed"));
}

#[tokio::test]
async fn a_failed_check_goes_round_again_carrying_what_the_last_attempt_left() {
    let model = ScriptedModel::answering(&["no sources", "as cited above"]);
    let stack = Stack::new(Some(Arc::clone(&model))).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.contains": "cited" })),
            generous(),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(run["state"]["answer"], "as cited above");
    assert_eq!(run["execution"]["usage"]["tokens"], 20);
    assert_eq!(
        model.prompts(),
        vec![
            "Q: why? Last: ".to_string(),
            "Q: why? Last: no sources".to_string(),
        ],
        "a retry has to be able to see its own last attempt"
    );
}

#[tokio::test]
async fn a_failed_check_with_no_retry_policy_fails_the_run_and_says_why() {
    let model = ScriptedModel::answering(&["no sources"]);
    let stack = Stack::new(Some(model)).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.contains": "cited" })),
            generous(),
            &["safety_failure"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "failed", "{run}");
    let detail = run["execution"]["detail"].as_str().expect("a reason");
    assert!(detail.contains("cited"), "{detail}");
}

#[tokio::test]
async fn running_out_of_iterations_stops_a_run_rather_than_succeeding_it() {
    let model = ScriptedModel::answering(&["one", "two", "three", "four"]);
    let stack = Stack::new(Some(Arc::clone(&model))).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.contains": "never" })),
            json!({ "max_steps": 2, "max_tokens": 10_000, "max_seconds": 300, "max_cost_micros": 1_000 }),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "stopped", "{run}");
    assert!(
        run["execution"]["detail"]
            .as_str()
            .expect("a reason")
            .contains("iterations"),
        "{run}"
    );
    assert_eq!(model.prompts().len(), 2);
}

#[tokio::test]
async fn running_out_of_tokens_stops_a_run_part_way_through() {
    let model = ScriptedModel::answering(&["no sources"]);
    let stack = Stack::new(Some(Arc::clone(&model))).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.contains": "never" })),
            json!({ "max_steps": 20, "max_tokens": 10, "max_seconds": 300, "max_cost_micros": 1_000 }),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "stopped", "{run}");
    assert!(
        run["execution"]["detail"]
            .as_str()
            .expect("a reason")
            .contains("tokens"),
        "{run}"
    );
    assert_eq!(model.prompts().len(), 1);
}

#[tokio::test]
async fn a_model_that_reports_no_usage_is_counted_as_unknown() {
    let model = ScriptedModel::without_token_counts(&["an answer"]);
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

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(run["execution"]["usage"]["tokens"], 0);
    assert_eq!(
        run["execution"]["usage"]["unmeasured"], 1,
        "zero tokens and nobody-counted-them must not read the same"
    );
}

#[tokio::test]
async fn a_model_repeating_itself_is_stopped_for_going_nowhere() {
    let model = ScriptedModel::answering(&["the same wrong answer"]);
    let stack = Stack::new(Some(Arc::clone(&model))).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.contains": "never" })),
            json!({ "max_steps": 20, "max_tokens": 10_000, "max_seconds": 300, "max_cost_micros": 1_000 }),
            &["validator_pass", "no_progress"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "stopped", "{run}");
    assert!(
        run["execution"]["detail"]
            .as_str()
            .expect("a reason")
            .contains("non_progress"),
        "{run}"
    );
    assert!(
        model.prompts().len() < 20,
        "non-progress has to stop it before the budget does"
    );
}

#[tokio::test]
async fn with_no_model_configured_a_run_fails_and_names_the_fix() {
    let stack = Stack::new(None).await;
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

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "failed", "{run}");
    assert!(
        run["execution"]["detail"]
            .as_str()
            .expect("a reason")
            .contains("CAPSULET_OLLAMA_URL"),
        "{run}"
    );
}

#[tokio::test]
async fn a_safety_check_stops_the_run() {
    let model = ScriptedModel::answering(&["rm -rf /"]);
    let stack = Stack::new(Some(model)).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.not_contains": "rm -rf", "on_fail": "safety" })),
            generous(),
            &["validator_pass", "safety_failure"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "stopped", "{run}");
    assert!(
        run["execution"]["detail"]
            .as_str()
            .expect("a reason")
            .contains("unsafe_effect"),
        "{run}"
    );
}

#[tokio::test]
async fn a_worker_stopping_after_every_step_changes_nothing() {
    let model = ScriptedModel::answering(&["no sources", "as cited above"]);
    let stack = Stack::new(Some(Arc::clone(&model))).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.contains": "cited" })),
            generous(),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;

    // Every step on a worker that has never seen this run, so nothing survives
    // between steps except what the log and the value store hold.
    for _ in 0..512 {
        match stack
            .worker(1)
            .advance_one()
            .await
            .expect("the worker advances")
        {
            // One step per lease means the step that finishes the run is not the pass
            // that notices, so the next worker finds nothing left to lease.
            Progress::Ended | Progress::NothingToDo => break,
            Progress::Suspended | Progress::StillRunning | Progress::LeaseLost => {}
        }
    }

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(run["state"]["answer"], "as cited above");
    assert_eq!(
        model.prompts().len(),
        2,
        "two iterations, each asking once, however many workers it took"
    );
}

#[tokio::test]
async fn a_bad_start_gets_an_answer_now_rather_than_a_run_that_fails_later() {
    let stack = Stack::new(None).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.non_empty": "true" })),
            generous(),
            &["validator_pass"],
        )
        .await;

    let (status, body) = stack
        .post(
            &format!("/v1/agents/{agent}/runs"),
            json!({ "initial_state": { "answer": "" } }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .expect("a message")
            .contains("`query`"),
        "{body}"
    );

    let (status, body) = stack
        .post(
            &format!("/v1/agents/{agent}/runs"),
            json!({ "initial_state": { "query": 42, "answer": "" } }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    let (status, body) = stack
        .post(
            &format!("/v1/agents/{agent}/runs"),
            json!({ "initial_state": [] }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn a_planner_agent_is_refused_at_the_start_by_name() {
    let stack = Stack::new(None).await;
    let mut graph = answering_graph(&json!({ "rule.non_empty": "true" }));
    graph["transition_policy"] = json!({ "mode": "planner", "actions": ["model"] });
    let agent = stack.agent(graph, generous(), &["validator_pass"]).await;

    let (status, body) = stack
        .post(
            &format!("/v1/agents/{agent}/runs"),
            json!({ "initial_state": { "query": "why?", "answer": "" } }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "agent_not_runnable");
    assert!(
        body["message"]
            .as_str()
            .expect("a message")
            .contains("planner"),
        "{body}"
    );
}

#[tokio::test]
async fn another_project_can_see_none_of_it() {
    let model = ScriptedModel::answering(&["a private answer"]);
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
    stack.run_to_rest().await;

    for path in [
        format!("/v1/agent-runs/{id}"),
        format!("/v1/agent-runs/{id}/events"),
    ] {
        let (status, _) = stack
            .request_as(&stack.foreign_token, Method::GET, &path, None)
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }
    for path in [
        format!("/v1/agent-runs/{id}/cancel"),
        format!("/v1/agents/{agent}/runs"),
    ] {
        let (status, _) = stack
            .request_as(
                &stack.foreign_token,
                Method::POST,
                &path,
                Some(json!({ "initial_state": { "query": "x", "answer": "" } })),
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }

    let (_, listed) = stack
        .request_as(&stack.foreign_token, Method::GET, "/v1/agent-runs", None)
        .await;
    assert_eq!(listed["agent_runs"], json!([]));

    // Holding a digest is not a way in either.
    let answer = serde_json::to_string("a private answer").expect("encodes");
    let digest = Digest::of(answer.as_bytes());
    assert!(
        stack
            .store
            .get_ir_run_value(&stack.tenant, &stack.project, &digest)
            .await
            .expect("reads")
            .is_some()
    );
    assert!(
        stack
            .store
            .get_ir_run_value(&stack.tenant, "some-other-project", &digest)
            .await
            .expect("reads")
            .is_none()
    );
}

#[tokio::test]
async fn the_definition_a_run_executed_stays_readable_after_the_agent_changes() {
    let model = ScriptedModel::answering(&["an answer"]);
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
    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    let digest = run["execution"]["definition_digest"]
        .as_str()
        .expect("a digest");
    let (status, body) = stack.get(&format!("/v1/ir/definitions/{digest}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["digest"], digest);
    let bytes = body["canonical_bytes"].as_str().expect("the bytes");
    assert!(
        bytes.contains("Q: {{query}} Last: {{last}}"),
        "the prompt is part of the version"
    );
}
