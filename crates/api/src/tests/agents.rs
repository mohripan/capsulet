//! Typed graphs, agent definitions and agent runs.

use super::*;

fn rag_graph_request(id: &str) -> Value {
    json!({
        "id": id,
        "name": "RAG Graph",
        "nodes": [
            {
                "id": "normalize",
                "name": "Normalize",
                "kind": "query_normalizer",
                "ports": [
                    { "id": "normalize.input", "direction": "input", "value_type": "user_query" },
                    { "id": "normalize.output", "direction": "output", "value_type": "normalized_query" }
                ]
            },
            {
                "id": "embed",
                "name": "Embed",
                "kind": "embedding",
                "ports": [
                    { "id": "embed.input", "direction": "input", "value_type": "normalized_query" },
                    { "id": "embed.output", "direction": "output", "value_type": "embedding_vector" }
                ],
                "config": { "model": "small-embedder" }
            }
        ],
        "hyperedges": [
            {
                "id": "query_to_normalize",
                "sources": [
                    { "kind": "state_field", "field": "query", "value_type": "user_query" }
                ],
                "targets": [
                    { "kind": "port", "node_id": "normalize", "port_id": "normalize.input", "value_type": "user_query" }
                ]
            },
            {
                "id": "normalize_to_embed",
                "sources": [
                    { "kind": "port", "node_id": "normalize", "port_id": "normalize.output", "value_type": "normalized_query" }
                ],
                "targets": [
                    { "kind": "port", "node_id": "embed", "port_id": "embed.input", "value_type": "normalized_query" }
                ]
            }
        ],
        "transition_policy": {
            "mode": "static",
            "cycles_allowed": false
        }
    })
}

async fn create_graph(app: &axum::Router, id: &str) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/graphs")
                .header("content-type", "application/json")
                .body(Body::from(rag_graph_request(id).to_string()))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
}

async fn create_agent(app: &axum::Router, id: &str, graph_id: &str) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/agents")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": id,
                        "name": "RAG Agent",
                        "graph_id": graph_id,
                        "budget": {
                            "max_steps": 8,
                            "max_tokens": 4096,
                            "max_seconds": 30,
                        "max_cost_micros": 100_000
                        }
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
}

#[tokio::test]
async fn creates_and_returns_typed_agent_graph() {
    let app = test_app(FakeStore::default());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/graphs")
                .header("content-type", "application/json")
                .body(Body::from(rag_graph_request("rag_graph").to_string()))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    let body = response_json(response).await;
    assert_eq!(body["id"], "rag_graph");
    assert_eq!(body["nodes"][0]["kind"], "query_normalizer");
    assert_eq!(body["transition_policy"]["mode"], "static");

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/graphs")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        response_json(list_response).await["graphs"][0]["id"],
        "rag_graph"
    );

    let get_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/graphs/rag_graph")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        response_json(get_response).await["hyperedges"][1]["targets"][0]["node_id"],
        "embed"
    );
}

#[tokio::test]
async fn rejects_invalid_typed_graph_wiring() {
    let app = test_app(FakeStore::default());
    let mut request = rag_graph_request("bad_graph");
    request["nodes"][1]["ports"][0]["value_type"] = json!("prompt");
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/graphs")
                .header("content-type", "application/json")
                .body(Body::from(request.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(response_json(response).await["code"], "validation_error");
}

#[tokio::test]
async fn creates_and_returns_agent_definition() {
    let app = test_app(FakeStore::default());
    create_graph(&app, "rag_agent_graph").await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/agents")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "rag_agent",
                        "name": "RAG Agent",
                        "graph_id": "rag_agent_graph",
                        "budget": {
                            "max_steps": 12,
                            "max_tokens": 8192,
                            "max_seconds": 60,
                        "max_cost_micros": 250_000
                        },
                        "termination_conditions": ["validator_pass", "safety_failure"]
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    assert_eq!(response_json(response).await["graph_id"], "rag_agent_graph");

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/agents")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        response_json(list_response).await["agents"][0]["id"],
        "rag_agent"
    );

    let get_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/agents/rag_agent")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let body = response_json(get_response).await;
    assert_eq!(body["budget"]["max_steps"], 12);
    assert_eq!(body["termination_conditions"][0], "validator_pass");
}

#[tokio::test]
async fn resubmitting_an_agent_run_id_is_a_conflict_not_a_reset() {
    let app = test_app(FakeStore::default());
    create_graph(&app, "conflict_graph").await;
    create_agent(&app, "conflict_agent", "conflict_graph").await;

    let start = |query: &'static str| {
        app.clone().oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/agents/conflict_agent/runs")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "id": "conflict_run", "initial_state": { "query": query } })
                        .to_string(),
                ))
                .expect("request"),
        )
    };
    assert_eq!(
        start("first").await.expect("response").status(),
        axum::http::StatusCode::CREATED
    );

    let conflict = start("second").await.expect("response");
    assert_eq!(conflict.status(), axum::http::StatusCode::CONFLICT);
    assert_eq!(
        response_json(conflict).await["code"],
        "agent_run_already_exists"
    );

    let stored = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/agent-runs/conflict_run")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response_json(stored).await["state"]["query"], "first");
}

#[tokio::test]
async fn starts_and_returns_agent_run() {
    let app = test_app(FakeStore::default());
    create_graph(&app, "run_graph").await;
    create_agent(&app, "run_agent", "run_graph").await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/agents/run_agent/runs")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "agent_run_1",
                        "initial_state": { "query": "What is Capsulet?" }
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    let body = response_json(response).await;
    assert_eq!(body["id"], "agent_run_1");
    assert_eq!(body["agent_id"], "run_agent");
    assert_eq!(body["status"], "queued");
    assert_eq!(body["state_version"], 0);
    assert_eq!(body["state"]["query"], "What is Capsulet?");

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/agent-runs")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        response_json(list_response).await["agent_runs"][0]["id"],
        "agent_run_1"
    );

    let get_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/agent-runs/agent_run_1")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        response_json(get_response).await["state"]["query"],
        "What is Capsulet?"
    );
}
