//! IR definitions and the runs enqueued from them.

use super::*;

#[tokio::test]
async fn a_registered_definition_can_be_enqueued_as_a_run_and_read_back() {
    let app = authenticated_app(FakeStore::default());

    let registration = app
        .clone()
        .oneshot(admin_request(
            Method::POST,
            "/v1/ir/definitions",
            Some(json!({
                "definition": ir_definition_body(
                    capsulet_ir::definition::AssuranceMode::Enforce
                )
            })),
        ))
        .await
        .expect("response");
    assert_eq!(
        registration.status(),
        axum::http::StatusCode::CREATED,
        "the fixture definition has to pass structural admission"
    );
    let digest = response_json(registration).await["digest"]
        .as_str()
        .expect("a digest")
        .to_string();

    let started = app
        .clone()
        .oneshot(admin_request(
            Method::POST,
            "/v1/ir/runs",
            Some(json!({ "definition_digest": digest, "id": "ir_run_release" })),
        ))
        .await
        .expect("response");
    assert_eq!(started.status(), axum::http::StatusCode::CREATED);

    let run: Value = response_json(started).await;
    assert_eq!(run["id"], "ir_run_release");
    assert_eq!(run["definition_digest"], digest.as_str());
    assert_eq!(
        run["status"], "queued",
        "the API enqueues a run; the graph worker is what advances it"
    );
    assert!(
        run["lease_owner"].is_null(),
        "nothing has taken the run yet, and the API is not entitled to"
    );

    let events: Value = response_json(
        app.clone()
            .oneshot(admin_request(
                Method::GET,
                "/v1/ir/runs/ir_run_release/events",
                None,
            ))
            .await
            .expect("response"),
    )
    .await;
    let log = events["events"].as_array().expect("an event array");
    assert_eq!(log.len(), 1, "a new run has exactly its admission event");
    assert_eq!(log[0]["position"], 0);
    assert_eq!(log[0]["kind"], "admitted");
    assert_eq!(
        log[0]["event"]["admitted"]["mode"], "enforce",
        "the mode comes from the definition, so nobody can downgrade it per run"
    );

    let listed: Value = response_json(
        app.oneshot(admin_request(Method::GET, "/v1/ir/runs", None))
            .await
            .expect("response"),
    )
    .await;
    assert_eq!(listed["runs"].as_array().expect("runs").len(), 1);
}

#[tokio::test]
async fn a_run_of_a_definition_nobody_registered_is_refused() {
    let app = authenticated_app(FakeStore::default());

    let response = app
        .oneshot(admin_request(
            Method::POST,
            "/v1/ir/runs",
            Some(json!({
                "definition_digest":
                    "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            })),
        ))
        .await
        .expect("response");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::NOT_FOUND,
        "a run has to name bytes somebody stored, or it could not say what it executed"
    );
}

#[tokio::test]
async fn an_ir_run_remembers_the_trace_of_the_request_that_started_it() {
    const CALLER_TRACE: &str = "0af7651916cd43dd8448eb211c80319c";
    let capture = capsulet_observability::testing::TraceCapture::start();
    let store = FakeStore::default();
    let app = authenticated_app(store.clone());
    let registration = app
        .clone()
        .oneshot(admin_request(
            Method::POST,
            "/v1/ir/definitions",
            Some(json!({
                "definition": ir_definition_body(capsulet_ir::definition::AssuranceMode::Enforce)
            })),
        ))
        .await
        .expect("response");
    let digest = response_json(registration).await["digest"]
        .as_str()
        .expect("a digest")
        .to_string();
    let mut request = admin_request(
        Method::POST,
        "/v1/ir/runs",
        Some(json!({ "definition_digest": digest, "id": "ir_run_traced" })),
    );
    request.headers_mut().insert(
        "traceparent",
        axum::http::HeaderValue::from_str(&format!("00-{CALLER_TRACE}-b7ad6b7169203331-01"))
            .expect("a header value"),
    );
    let started = app.oneshot(request).await.expect("response");
    assert_eq!(started.status(), axum::http::StatusCode::CREATED);
    drop(capture);

    let runs = store.ir_runs.lock().expect("not poisoned");
    let run = runs
        .iter()
        .find(|run| run.id == "ir_run_traced")
        .expect("the run was stored");
    let parent = run
        .trace_parent
        .as_deref()
        .expect("the run carries a traceparent");
    assert_eq!(
        parent.split('-').nth(1),
        Some(CALLER_TRACE),
        "the workers' spans will continue the caller's trace: {parent}"
    );
}

/// The fixture definition, taking one graph input, `change`, into its node.
fn definition_taking_a_change() -> Value {
    let mut definition = ir_definition_body(capsulet_ir::definition::AssuranceMode::Enforce);
    let graph: capsulet_ir::Graph =
        serde_json::from_value(definition["graph"].clone()).expect("the fixture graph reads");
    let identifier = |value: &str| capsulet_ir::Identifier::parse(value).expect("identifier");
    let opaque = capsulet_ir::ValueSchema::Json {
        reason: "a change, as files".to_string(),
    };
    let mut node = graph
        .node(&identifier("prepare"))
        .expect("the fixture node")
        .clone();
    node.inputs = vec![capsulet_ir::InputPort::new(
        identifier("change"),
        opaque.clone(),
    )];
    let taking = capsulet_ir::Graph::new(capsulet_ir::GraphBuilder {
        nodes: vec![node],
        edges: vec![capsulet_ir::graph::Hyperedge {
            id: identifier("change-in"),
            sources: vec![capsulet_ir::graph::Endpoint::GraphInput {
                name: identifier("change"),
            }],
            targets: vec![capsulet_ir::graph::Endpoint::Port {
                node: identifier("prepare"),
                port: identifier("change"),
            }],
            combine: capsulet_ir::graph::Combine::Forward,
            trust: capsulet_ir::graph::TrustDerivation::Weakest,
        }],
        inputs: vec![capsulet_ir::OutputPort::new(identifier("change"), opaque)],
        ..capsulet_ir::GraphBuilder::default()
    })
    .expect("the graph is valid");
    definition["graph"] = serde_json::to_value(taking).expect("encodes");
    definition
}

async fn registered(app: &axum::Router, definition: Value) -> String {
    let (status, body) = send(
        app,
        admin_request(
            Method::POST,
            "/v1/ir/definitions",
            Some(json!({ "definition": definition })),
        ),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CREATED, "{body}");
    body["digest"].as_str().expect("a digest").to_string()
}

#[tokio::test]
async fn a_run_starts_with_its_inputs_bound_and_refuses_one_the_definition_does_not_declare() {
    let app = authenticated_app(FakeStore::default());
    let digest = registered(&app, definition_taking_a_change()).await;

    let (status, body) = send(
        &app,
        admin_request(
            Method::POST,
            "/v1/ir/runs",
            Some(json!({
                "definition_digest": digest,
                "id": "ir_run_stray",
                "inputs": { "patch": "an input nobody declared" }
            })),
        ),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "validation_error");

    let change = json!({ "change/plan.json": "{\"format_version\": \"1.2\"}" });
    let (status, body) = send(
        &app,
        admin_request(
            Method::POST,
            "/v1/ir/runs",
            Some(json!({
                "definition_digest": digest,
                "id": "ir_run_change",
                "inputs": { "change": change }
            })),
        ),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CREATED, "{body}");
    let (_, events) = send(
        &app,
        admin_request(Method::GET, "/v1/ir/runs/ir_run_change/events", None),
    )
    .await;
    let log = events["events"].as_array().expect("events");
    assert_eq!(
        log[1]["kind"], "inputs_bound",
        "the inputs are bound before anything runs"
    );
    let bound = serde_json::to_string(&change).expect("encodes");
    assert_eq!(
        log[1]["event"]["inputs_bound"]["inputs"]["change"],
        capsulet_ir::Digest::of(bound.as_bytes()).to_string()
    );
}

#[tokio::test]
async fn a_run_that_has_not_finished_has_no_certificate_to_seal_yet() {
    let app = authenticated_app(FakeStore::default());
    let digest = registered(
        &app,
        ir_definition_body(capsulet_ir::definition::AssuranceMode::Enforce),
    )
    .await;
    let (status, _) = send(
        &app,
        admin_request(
            Method::POST,
            "/v1/ir/runs",
            Some(json!({ "definition_digest": digest, "id": "ir_run_queued" })),
        ),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CREATED);

    let (status, body) = send(
        &app,
        admin_request(Method::POST, "/v1/ir/runs/ir_run_queued/certificate", None),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "run_not_finished");

    let (status, _) = send(
        &app,
        admin_request(Method::POST, "/v1/ir/runs/ir_run_nobody/certificate", None),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
}
