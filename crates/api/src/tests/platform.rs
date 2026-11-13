//! Health, the published `OpenAPI` document, execution pools, and request telemetry.

use super::*;

#[tokio::test]
async fn healthz_returns_ok() {
    let response = test_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({ "status": "ok", "version": release_version() })
    );
}

/// `product.version` in the repository's `release.toml`, which the API must
/// serve: read from the file, not from this crate, so a crate that drifted
/// from the release fails here.
fn release_version() -> String {
    let release = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../release.toml"),
    )
    .expect("release.toml");
    let mut table = "";
    for line in release.lines().map(str::trim) {
        if line.starts_with('[') {
            table = line;
        } else if table == "[product]"
            && let Some(value) = line.strip_prefix("version = \"")
        {
            return value.trim_end_matches('"').to_string();
        }
    }
    panic!("release.toml has no product.version");
}

#[tokio::test]
async fn openapi_endpoint_returns_the_canonical_generated_contract() {
    let response = test_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .uri("/openapi.json")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response.headers()[axum::http::header::CONTENT_TYPE],
        "application/vnd.oai.openapi+json; charset=utf-8"
    );
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    assert_eq!(
        String::from_utf8(bytes.to_vec()).expect("UTF-8 OpenAPI"),
        crate::canonical_openapi_json().expect("OpenAPI should serialize")
    );
}

#[tokio::test]
async fn lists_configured_execution_pools() {
    let response = test_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .uri("/v1/execution-pools")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "execution_pools": [
                { "name": "mini", "description": "Default execution pool", "is_default": true, "host_group": "mini" },
                { "name": "large", "description": "Configured execution pool", "is_default": false, "host_group": "large" }
            ]
        })
    );

    let response = test_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .uri("/v1/host-groups")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "host_groups": [
                { "name": "mini", "description": "Default host group", "is_default": true, "execution_pool": "mini", "host_count": null },
                { "name": "large", "description": "Configured host group", "is_default": false, "execution_pool": "large", "host_count": null }
            ]
        })
    );
}

#[tokio::test]
async fn response_body_helper_handles_empty_body() {
    let bytes = to_bytes(Body::empty(), usize::MAX)
        .await
        .expect("empty body");
    assert!(bytes.is_empty());
}

#[tokio::test]
async fn request_metrics_and_spans_name_the_route_template_not_the_path() {
    let _ = capsulet_observability::testing::install_metrics();
    let capture = capsulet_observability::testing::TraceCapture::start();
    let app = test_app(FakeStore::default());
    for path in [
        "/v1/agent-runs/run_route_label_one",
        "/v1/agent-runs/run_route_label_two",
        "/no/such/route_label_three",
    ] {
        app.clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
    }

    let rendered = capsulet_observability::render_metrics();
    assert!(
        rendered.contains("route=\"/v1/agent-runs/{id}\""),
        "{rendered}"
    );
    assert!(rendered.contains("route=\"unmatched\""), "{rendered}");
    for id in [
        "run_route_label_one",
        "run_route_label_two",
        "route_label_three",
    ] {
        assert!(
            !rendered.contains(id),
            "a request path became a metric label: {id}"
        );
    }

    let spans = capture.spans();
    assert!(spans.iter().any(|span| {
        span.name == "http.request"
            && span.attribute("http.route") == Some("/v1/agent-runs/{id}")
            && span.attribute("url.path") == Some("/v1/agent-runs/run_route_label_one")
    }));
    assert!(spans.iter().any(|span| {
        span.name == "http.request" && span.attribute("http.route") == Some("unmatched")
    }));
}

#[tokio::test]
async fn a_request_carrying_a_traceparent_joins_the_callers_trace() {
    const CALLER_TRACE: &str = "4bf92f3577b34da6a3ce929d0e0e4736";
    let capture = capsulet_observability::testing::TraceCapture::start();
    let app = test_app(FakeStore::default());
    for traceparent in [
        format!("00-{CALLER_TRACE}-00f067aa0ba902b7-01"),
        "not a traceparent".to_string(),
    ] {
        app.clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/agent-runs/run_traceparent")
                    .header("traceparent", traceparent)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
    }

    let requests: Vec<_> = capture
        .spans()
        .into_iter()
        .filter(|span| span.name == "http.request")
        .collect();
    assert_eq!(requests.len(), 2);
    assert!(
        requests.iter().any(|span| span.trace_id == CALLER_TRACE
            && span.parent_span_id.as_deref() == Some("00f067aa0ba902b7")),
        "the request continues the caller's trace: {requests:?}"
    );
    assert!(
        requests
            .iter()
            .any(|span| span.trace_id != CALLER_TRACE && span.parent_span_id.is_none()),
        "a malformed header starts a trace of its own: {requests:?}"
    );
}
