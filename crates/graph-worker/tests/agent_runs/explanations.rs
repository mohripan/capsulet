//! Run bundles and explanations: what a run did, read back without its content.

use super::*;

async fn bundle_of(stack: &Stack, id: &str, fixture: &str) -> (SealedRunBundle, RunBundle) {
    let (status, body) = stack
        .post(&format!("/v1/agent-runs/{id}/debug-bundle"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let sealed: SealedRunBundle = serde_json::from_value(body).expect("a sealed bundle");
    let bundle = sealed.open().expect("the bundle matches its digest");
    if std::env::var_os("CAPSULET_WRITE_BUNDLE_FIXTURES").is_some() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../replay/tests/fixtures")
            .join(format!("run-{fixture}.json"));
        let text = serde_json::to_string_pretty(&sealed).expect("encodes");
        std::fs::write(path, text + "\n").expect("the fixture is written");
    }
    (sealed, bundle)
}

#[tokio::test]
async fn a_bundle_explains_a_run_that_succeeded_and_carries_none_of_its_content() {
    const QUERY: &str = "bundle-question-5150";
    const ANSWER: &str = "bundle-answer-5151";
    let stack = Stack::new(Some(ScriptedModel::answering(&[ANSWER]))).await;
    let agent = answering_agent(&stack).await;
    let id = stack
        .start(&agent, json!({ "query": QUERY, "answer": "" }))
        .await;
    stack.run_to_rest().await;

    let (sealed, bundle) = bundle_of(&stack, &id, "succeeded").await;
    let explained = explain(&bundle);
    assert!(explained.agrees(), "{explained:?}");
    assert_eq!(explained.status, Ok(RunStatus::Completed));
    assert!(explained.definition_matches);
    assert_eq!(explained.spent.tokens, 10);
    assert_eq!(explained.unfinished, [] as [capsulet_ir::Identifier; 0]);
    let model = explained
        .nodes
        .iter()
        .find(|node| node.kind == "llm")
        .expect("the model node");
    assert_eq!(
        (model.attempts, model.finished, model.rework),
        (1, 1, Rework::Model)
    );
    assert!(
        !sealed.body.contains(QUERY) && !sealed.body.contains(ANSWER),
        "a bundle names what a run was given and produced by digest, never by content"
    );

    let mut tampered = sealed.clone();
    tampered.body = tampered.body.replacen("completed", "cancelled", 1);
    assert!(
        tampered.open().is_err(),
        "a changed body no longer matches its digest"
    );
}

#[tokio::test]
async fn a_bundle_explains_a_run_that_failed() {
    let stack = Stack::new(None).await;
    let agent = answering_agent(&stack).await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;
    stack.run_to_rest().await;

    let (_, bundle) = bundle_of(&stack, &id, "failed").await;
    let explained = explain(&bundle);
    assert!(explained.agrees(), "{explained:?}");
    assert_eq!(explained.status, Ok(RunStatus::Failed));
    assert!(
        explained
            .ended_because
            .as_deref()
            .is_some_and(|reason| reason.contains("verifier_unavailable")),
        "{explained:?}"
    );
}

#[tokio::test]
async fn a_bundle_explains_a_run_interrupted_mid_call() {
    let stack = Stack::new(None).await;
    let agent = answering_agent(&stack).await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;
    // The worker is dropped while it waits on the model, as a crash would leave it.
    let worker = stack.worker_using(Some(Arc::new(SilentModel)), 256);
    assert!(
        tokio::time::timeout(Duration::from_secs(3), worker.advance_one())
            .await
            .is_err(),
        "the model never answers"
    );

    let (_, bundle) = bundle_of(&stack, &id, "interrupted").await;
    let explained = explain(&bundle);
    assert!(explained.agrees(), "{explained:?}");
    assert_eq!(explained.status, Ok(RunStatus::Running));
    let doing: Vec<String> = explained
        .unfinished
        .iter()
        .map(ToString::to_string)
        .collect();
    let model = explained
        .nodes
        .iter()
        .find(|node| node.kind == "llm")
        .expect("the model node");
    assert_eq!(
        doing,
        vec![model.node.to_string()],
        "it was in the model call"
    );
    assert_eq!((model.attempts, model.finished), (1, 0));
}

#[tokio::test]
async fn a_bundle_explains_a_run_that_ran_out_of_budget() {
    let stack = Stack::new(Some(ScriptedModel::answering(&["one", "two", "three"]))).await;
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

    let (_, bundle) = bundle_of(&stack, &id, "budget-exhausted").await;
    let explained = explain(&bundle);
    assert!(explained.agrees(), "{explained:?}");
    assert_eq!(explained.status, Ok(RunStatus::Failed));
    assert!(
        explained
            .ended_because
            .as_deref()
            .is_some_and(|reason| reason.contains("iterations budget ran out")),
        "{explained:?}"
    );
    let model = explained
        .nodes
        .iter()
        .find(|node| node.kind == "llm")
        .expect("the model node");
    assert_eq!(model.finished, 2, "one call each time round, twice");
}

fn explained_node(explained: &Value, name: &str) -> Value {
    explained["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["node"] == name)
        .cloned()
        .unwrap_or_else(|| panic!("no node {name} in {explained}"))
}

/// Over HTTP, a run's explanation is its bundle's, read by the same code: a check
/// that held is `checked` with the rule it held, the model's answer `proposed`, and
/// the run, which completed in observe mode, `unverified`. An offline reader of the
/// run's own export says the same of every node.
#[tokio::test]
async fn an_explanation_labels_what_each_node_did_and_agrees_with_the_export() {
    let stack = Stack::new(Some(ScriptedModel::answering(&["an answer"]))).await;
    let agent = answering_agent(&stack).await;
    let id = stack
        .start(&agent, json!({ "query": "what?", "answer": "" }))
        .await;
    stack.run_to_rest().await;

    let (status, explained) = stack.get(&format!("/v1/agent-runs/{id}/explanation")).await;
    assert_eq!(status, StatusCode::OK, "{explained}");
    assert_eq!(explained["result"], "unverified", "{explained}");
    assert_eq!(explained["assurance_mode"], "observe", "{explained}");
    assert_eq!(explained["agrees"], true, "{explained}");
    let check = explained_node(&explained, "check");
    assert_eq!(check["result"], "checked", "{explained}");
    assert_eq!(
        check["rules"],
        json!([{ "rule": "non_empty", "value": "true" }])
    );
    assert_eq!(explained_node(&explained, "model")["result"], "proposed");
    assert_eq!(
        explained_node(&explained, "normalize")["result"],
        "unverified"
    );
    // Evidence is named by digest, never carried.
    assert!(
        explained_node(&explained, "model")["events"]
            .as_array()
            .expect("events")
            .iter()
            .flat_map(|event| event["evidence"].as_array().cloned().unwrap_or_default())
            .any(|evidence| evidence["digest"]
                .as_str()
                .is_some_and(|digest| digest.starts_with("sha256:"))),
        "{explained}"
    );
    assert!(!explained.to_string().contains("an answer"), "{explained}");

    let offline = explain(&exported(&stack, &id).await);
    assert!(offline.agrees(), "{offline:?}");
    assert_eq!(
        explained["decisions"]["replayed"],
        offline.decisions.replayed
    );
    for account in &offline.nodes {
        assert_eq!(
            explained_node(&explained, account.node.as_str())["result"],
            account.result.as_str(),
            "{}",
            account.node
        );
    }
}

/// A check whose rules did not hold is `disputed`, and a run that ended on it
/// without an answer `abstained`.
#[tokio::test]
async fn an_explanation_says_a_failed_check_disputed_the_answer_and_the_run_abstained() {
    let stack = Stack::new(Some(ScriptedModel::answering(&["no sources"]))).await;
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

    let (status, explained) = stack.get(&format!("/v1/agent-runs/{id}/explanation")).await;
    assert_eq!(status, StatusCode::OK, "{explained}");
    assert_eq!(
        explained_node(&explained, "check")["result"],
        "disputed",
        "{explained}"
    );
    assert_eq!(explained["result"], "abstained", "{explained}");
    assert!(explained["ended_because"].is_string(), "{explained}");
    assert_eq!(explained["agrees"], true, "{explained}");
}

/// Another project cannot read a run's explanation, or tell that the run exists.
#[tokio::test]
async fn another_projects_explanation_of_a_run_is_not_found() {
    let stack = Stack::new(Some(ScriptedModel::answering(&["an answer"]))).await;
    let agent = answering_agent(&stack).await;
    let id = stack
        .start(&agent, json!({ "query": "what?", "answer": "" }))
        .await;
    stack.run_to_rest().await;

    let response = stack
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/v1/agent-runs/{id}/explanation"))
                .header("authorization", format!("Bearer {}", stack.token))
                .header("x-capsulet-project-id", "another-project")
                .body(Body::empty())
                .expect("a valid request"),
        )
        .await
        .expect("a response");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("a body")
        .to_bytes();
    assert!(
        !String::from_utf8_lossy(&body).contains(&id),
        "the answer names the run"
    );
}
