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
