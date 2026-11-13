//! Traces, spans and metrics of a run.

use super::*;

fn leases_of<'a>(spans: &'a [CapturedSpan], run: &str) -> Vec<&'a CapturedSpan> {
    spans
        .iter()
        .filter(|span| {
            span.name == "capsulet.lease" && span.attribute("capsulet.run_id") == Some(run)
        })
        .collect()
}

fn children_of<'a>(spans: &'a [CapturedSpan], parents: &[&CapturedSpan]) -> Vec<&'a CapturedSpan> {
    let ids: BTreeSet<&str> = parents.iter().map(|span| span.span_id.as_str()).collect();
    spans
        .iter()
        .filter(|span| {
            span.parent_span_id
                .as_deref()
                .is_some_and(|parent| ids.contains(parent))
        })
        .collect()
}

fn assert_nothing_said(spans: &[CapturedSpan], content: &[&str]) {
    for span in spans {
        let rendered = span.rendered();
        for each in content {
            assert!(
                !rendered.contains(each),
                "`{each}` reached telemetry on `{}`: {rendered}",
                span.name
            );
        }
    }
}

#[tokio::test]
async fn a_run_is_one_trace_from_its_request_through_every_lease_on_every_worker() {
    const QUERY: &str = "which-moon-7731?";
    const FIRST: &str = "no sources 4410";
    const SECOND: &str = "as cited above 5521";
    let capture = TraceCapture::start();
    let model = ScriptedModel::answering(&[FIRST, SECOND]);
    let stack = Stack::new(Some(Arc::clone(&model))).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.contains": "cited" })),
            generous(),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": QUERY, "answer": "" }))
        .await;

    // Two workers taking turns a step at a time, so the run is carried by many
    // leases held by more than one worker.
    let workers = [stack.worker(1), stack.worker(1)];
    let mut turns = 0;
    while workers[turns % 2]
        .advance_one()
        .await
        .expect("the worker advances")
        != Progress::NothingToDo
    {
        turns += 1;
        assert!(turns < 256, "the run did not come to rest");
    }
    let run = stack.run(&id).await;
    assert_eq!(run["status"], "succeeded", "{run}");

    let spans = capture.spans();
    let path = format!("/v1/agents/{agent}/runs");
    let start = spans
        .iter()
        .find(|span| {
            span.name == "http.request"
                && span.attribute("http.method") == Some("POST")
                && span.attribute("http.route") == Some("/v1/agents/{id}/runs")
                && span.attribute("url.path") == Some(path.as_str())
        })
        .expect("the request that started the run was traced");

    let leases = assert_leases_continue(&spans, start, &stack, &id);
    assert_attempts_and_calls(&spans, start, &leases);
    assert_nothing_said(&spans, &[QUERY, FIRST, SECOND, "Last:"]);
}

/// Every lease of the run joins the starting request's trace, and both workers held one.
fn assert_leases_continue<'a>(
    spans: &'a [CapturedSpan],
    start: &CapturedSpan,
    stack: &Stack,
    id: &str,
) -> Vec<&'a CapturedSpan> {
    let leases = leases_of(spans, id);
    assert!(leases.len() > 2, "{} leases", leases.len());
    for lease in &leases {
        assert_eq!(
            lease.trace_id, start.trace_id,
            "every lease continues the trace of the request that started the run"
        );
        assert!(lease.parent_span_id.is_some());
        assert_eq!(
            lease.attribute("capsulet.tenant_id"),
            Some(stack.tenant.as_str())
        );
        assert_eq!(
            lease.attribute("capsulet.project_id"),
            Some(stack.project.as_str())
        );
        assert!(lease.attribute("capsulet.definition_digest").is_some());
        assert!(lease.attribute("capsulet.lease.epoch").is_some());
        assert!(lease.error.is_none(), "{lease:?}");
    }
    let holders: BTreeSet<&str> = leases
        .iter()
        .filter_map(|lease| lease.attribute("capsulet.worker_id"))
        .collect();
    assert_eq!(
        holders.len(),
        2,
        "both workers carried the run: {holders:?}"
    );
    // A lease records where the run stood at its last fold. The newest one wrote
    // the completion, so by its fold both model calls had been paid for.
    let last = leases
        .iter()
        .max_by_key(|lease| {
            lease
                .attribute("capsulet.lease.epoch")
                .and_then(|epoch| epoch.parse::<u64>().ok())
        })
        .expect("a lease");
    assert_eq!(
        last.attribute("capsulet.run.spent.tokens"),
        Some("20"),
        "what the run spent, as its log says"
    );
    assert_eq!(last.attribute("capsulet.run.spent.unmeasured"), Some("0"));
    leases
}

/// Each node attempt sits under a lease, and each model call under its attempt.
fn assert_attempts_and_calls(
    spans: &[CapturedSpan],
    start: &CapturedSpan,
    leases: &[&CapturedSpan],
) {
    let nodes = children_of(spans, leases);
    let attempts: Vec<&&CapturedSpan> = nodes
        .iter()
        .filter(|node| node.attribute("capsulet.node.kind") == Some("llm"))
        .collect();
    assert_eq!(attempts.len(), 2, "one attempt each time round the loop");
    let iterations: BTreeSet<&str> = attempts
        .iter()
        .filter_map(|node| node.attribute("capsulet.loop.iteration"))
        .collect();
    assert_eq!(
        iterations.len(),
        2,
        "each attempt says which time round it was"
    );
    for attempt in &attempts {
        assert_eq!(attempt.attribute("capsulet.usage.tokens"), Some("10"));
        assert_eq!(
            attempt.attribute("capsulet.usage.unmeasured"),
            Some("false")
        );
    }
    for node in &nodes {
        assert!(node.name.starts_with("capsulet.node "), "{}", node.name);
        assert_eq!(node.trace_id, start.trace_id);
        assert_eq!(node.attribute("capsulet.node.outcome"), Some("finished"));
        let started: u64 = node
            .attribute("capsulet.event.started_position")
            .and_then(|position| position.parse().ok())
            .expect("the position of the event that opened the attempt");
        let finished: u64 = node
            .attribute("capsulet.event.finished_position")
            .and_then(|position| position.parse().ok())
            .expect("the position of the event that closed it");
        assert!(started < finished);
    }

    let calls = children_of(
        spans,
        &attempts.iter().map(|each| **each).collect::<Vec<_>>(),
    );
    assert_eq!(calls.len(), 2, "each attempt made one model call");
    for call in &calls {
        assert_eq!(call.name, "text_completion small-model");
        assert_eq!(call.kind, "client");
        assert_eq!(
            call.attribute("gen_ai.operation.name"),
            Some("text_completion")
        );
        assert_eq!(call.attribute("gen_ai.provider.name"), Some("scripted"));
        assert_eq!(call.attribute("gen_ai.request.model"), Some("small-model"));
        assert_eq!(call.attribute("gen_ai.usage.input_tokens"), Some("10"));
        assert_eq!(
            call.attribute("gen_ai.usage.output_tokens"),
            None,
            "a count nobody reported is left off, not written down as zero"
        );
        assert!(call.error.is_none());
    }
}

#[tokio::test]
async fn a_refused_model_call_is_an_error_that_names_its_kind_and_not_its_content() {
    const QUERY: &str = "private-question-8812";
    let capture = TraceCapture::start();
    let stack = Stack::new(None).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.non_empty": "true" })),
            generous(),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": QUERY, "answer": "" }))
        .await;
    let worker = stack.worker_using(Some(Arc::new(RefusingModel)), 256);
    for _ in 0..64 {
        if worker.advance_one().await.expect("the worker advances") == Progress::NothingToDo {
            break;
        }
    }

    let spans = capture.spans();
    let call = spans
        .iter()
        .find(|span| span.attribute("gen_ai.operation.name") == Some("text_completion"))
        .expect("the model call was traced");
    assert_eq!(call.error.as_deref(), Some("refused with status 503"));
    assert_eq!(call.attribute("error.type"), Some("rejected"));
    let node = spans
        .iter()
        .find(|span| Some(span.span_id.as_str()) == call.parent_span_id.as_deref())
        .expect("the call sits under its node attempt");
    assert_eq!(node.attribute("capsulet.node.outcome"), Some("failed"));
    assert_eq!(node.attribute("error.type"), Some("verifier_unavailable"));
    assert_eq!(node.error.as_deref(), Some("verifier_unavailable"));

    assert_nothing_said(&spans, &[QUERY, "overloaded while answering"]);

    // Nor does the run's log, which an erasure cannot rewrite.
    let (_, events) = stack.get(&format!("/v1/agent-runs/{id}/events")).await;
    let log = events.to_string();
    assert!(log.contains("refused with status 503"), "{log}");
    assert!(!log.contains("overloaded while answering"), "{log}");
    assert!(!log.contains(QUERY), "{log}");
}

#[tokio::test]
async fn an_effect_is_a_tool_span_and_a_refused_one_says_why_only_by_kind() {
    const PREDICATE: &str = "renewed-unique-6630";
    let capture = TraceCapture::start();
    let stack = Stack::new(None).await;
    let agent = stack
        .agent(memory_write_graph(), generous(), &["validator_pass"])
        .await;
    let id = stack
        .start(&agent, json!({ "claim": { "predicate": PREDICATE } }))
        .await;
    stack.run_to_rest().await;
    let run = stack.run(&id).await;
    assert_eq!(run["status"], "failed", "{run}");

    let spans = capture.spans();
    let leases = leases_of(&spans, &id);
    let tool = children_of(&spans, &leases)
        .into_iter()
        .find(|span| span.attribute("gen_ai.operation.name") == Some("execute_tool"))
        .expect("the effect was traced under its lease");
    assert!(tool.name.starts_with("execute_tool "), "{}", tool.name);
    assert_eq!(tool.kind, "internal");
    assert_eq!(tool.attribute("gen_ai.tool.type"), Some("datastore"));
    assert_eq!(tool.attribute("capsulet.node.kind"), Some("memory_write"));
    assert_eq!(tool.attribute("capsulet.effect.outcome"), Some("failed"));
    assert_eq!(tool.attribute("capsulet.effect.recovered"), Some("false"));
    assert!(tool.attribute("capsulet.effect.attempt").is_some());
    assert!(tool.attribute("capsulet.event.claimed_position").is_some());
    let kind = tool.attribute("error.type").expect("a failure kind");
    assert_eq!(tool.error.as_deref(), Some(kind));
    assert!(
        kind.chars()
            .all(|each| each.is_ascii_lowercase() || each == '_'),
        "a kind from a closed set, not a message: {kind}"
    );

    assert_nothing_said(&spans, &[PREDICATE, "a claim to write needs"]);
    assert_decisions_replay(&stack, &id).await;
}

/// Runs the retrying agent to rest and says how it ended.
async fn retrying_run_outcome() -> (Value, Value, Value, Vec<String>) {
    let model = ScriptedModel::answering(&["no sources", "as cited above"]);
    let stack = Stack::new(Some(model)).await;
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
    let kinds = stack.event_kinds(&id).await;
    (
        run["status"].clone(),
        run["state"].clone(),
        run["execution"]["usage"].clone(),
        kinds,
    )
}

#[tokio::test]
async fn a_collector_that_is_down_changes_nothing_about_how_a_run_ends() {
    let untraced = retrying_run_outcome().await;

    let collector_down = FailingExport::start();
    let traced = retrying_run_outcome().await;

    assert!(
        collector_down.attempts() > 0,
        "spans were exported, and every export was refused"
    );
    assert_eq!(untraced.0, "succeeded");
    assert_eq!(
        traced, untraced,
        "telemetry failing is not something a run can notice"
    );
}

#[tokio::test]
async fn the_metrics_a_run_records_have_labels_from_closed_sets() {
    // Another test in this binary may have installed it already, which is as good.
    let _ = capsulet_observability::testing::install_metrics();
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
    assert_eq!(stack.run(&id).await["status"], "succeeded");

    let model_labels: &[&str] = &[
        "gen_ai_operation_name",
        "gen_ai_provider_name",
        "gen_ai_request_model",
        "error_type",
        "quantile",
    ];
    let token_labels: &[&str] = &[
        "gen_ai_operation_name",
        "gen_ai_provider_name",
        "gen_ai_request_model",
        "gen_ai_token_type",
        "quantile",
    ];
    let closed: BTreeMap<&str, &[&str]> = BTreeMap::from([
        (
            "capsulet_graph_worker_node_attempts_total",
            &["kind", "outcome", "failure"][..],
        ),
        (
            "capsulet_graph_worker_effect_attempts_total",
            &["kind", "outcome"][..],
        ),
        ("gen_ai_client_operation_duration_seconds", model_labels),
        ("gen_ai_client_token_usage", token_labels),
        (
            "capsulet_model_usage_unmeasured_total",
            &[
                "gen_ai_operation_name",
                "gen_ai_provider_name",
                "gen_ai_token_type",
            ][..],
        ),
    ]);

    let rendered = capsulet_observability::render_metrics();
    let mut seen = BTreeSet::new();
    for line in rendered.lines().filter(|line| !line.starts_with('#')) {
        let Some((name, rest)) = line.split_once('{') else {
            continue;
        };
        let base = name.trim_end_matches("_sum").trim_end_matches("_count");
        let Some(allowed) = closed.get(base) else {
            continue;
        };
        seen.insert(base);
        let (labels, _) = rest.split_once('}').expect("a closed label set");
        for pair in labels.split("\",") {
            let (key, value) = pair.split_once("=\"").expect("a key and a value");
            assert!(
                allowed.contains(&key),
                "`{base}` has a label `{key}` outside its closed set: {line}"
            );
            for identifier in [&stack.tenant, &stack.project, &id] {
                assert!(
                    !value.contains(identifier.as_str()),
                    "an unbounded identifier became a label value: {line}"
                );
            }
        }
    }
    for expected in [
        "capsulet_graph_worker_node_attempts_total",
        "gen_ai_client_operation_duration_seconds",
        "gen_ai_client_token_usage",
        "capsulet_model_usage_unmeasured_total",
    ] {
        assert!(
            seen.contains(expected),
            "`{expected}` was not recorded:\n{rendered}"
        );
    }
}
