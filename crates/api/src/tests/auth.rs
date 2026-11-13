//! Authentication, rate limits, project selection and membership, and scoped tokens.

use super::*;

#[tokio::test]
async fn protected_routes_require_a_valid_bearer_token() {
    let app = authenticated_app(FakeStore::default());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/jobs/runs")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), 401);
    assert_eq!(
        response_json(response).await["code"],
        "authentication_required"
    );
}

#[tokio::test]
async fn activity_stream_requires_auth_and_uses_sse() {
    let app = authenticated_app(FakeStore::default());
    let unauthenticated_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events/stream")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(unauthenticated_response.status(), 401);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/stream")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), 200);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
}

#[tokio::test]
async fn rate_limit_is_partitioned_by_client_ip() {
    let app = test_app(FakeStore::default());
    for _ in 0..105 {
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/livez")
                    .header("x-forwarded-for", "10.10.10.1")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
    }

    let response = app
        .oneshot(
            Request::builder()
                .uri("/livez")
                .header("x-forwarded-for", "10.10.10.2")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
}

#[tokio::test]
async fn rate_limit_is_not_reset_by_presenting_a_bearer_token() {
    let app = authenticated_app(FakeStore::default());
    // Drain the bucket by observation rather than by a fixed count. The quota
    // replenishes on a timer, so any hard-coded number races the clock on a slow
    // machine and this test was flaky when it assumed 105 requests was enough.
    let mut exhausted = false;
    for _ in 0..2_000 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/livez")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        if response.status() == axum::http::StatusCode::TOO_MANY_REQUESTS {
            exhausted = true;
            break;
        }
    }
    assert!(
        exhausted,
        "rate limit never engaged for the unauthenticated path"
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/auth/me")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    // Keying on the credential would put this request in a fresh bucket, so an attacker
    // rotating tokens would never be throttled on the authentication path.
    assert_eq!(response.status(), axum::http::StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn job_definitions_are_scoped_to_selected_project() {
    let app = authenticated_app(FakeStore::default());
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/job-definitions")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .header("x-capsulet-project-id", "finance")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "job_finance_report",
                        "name": "Finance report",
                        "python_script": "print('finance')"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(create_response.status(), 201);

    let default_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/job-definitions")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        response_json(default_response).await["job_definitions"],
        json!([])
    );

    let finance_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/job-definitions")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .header("x-capsulet-project-id", "finance")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        response_json(finance_response).await["job_definitions"][0]["id"],
        "job_finance_report"
    );
}

#[tokio::test]
async fn principal_cannot_select_project_without_membership() {
    let response = authenticated_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .uri("/v1/job-definitions")
                .header(
                    "authorization",
                    "Bearer finance-token-0123456789-abcdefghijkl",
                )
                .header("x-capsulet-project-id", "default")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn project_admin_can_manage_project_memberships() {
    let app = authenticated_app(FakeStore::default());
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/projects/finance/memberships")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "principal_kind": "user",
                        "principal_name": "developer",
                        "role": "project_operator"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(create_response.status(), 200);

    let list_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/projects/finance/memberships")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let body = response_json(list_response).await;
    assert!(
        body["memberships"]
            .as_array()
            .expect("memberships")
            .iter()
            .any(|membership| membership["principal_name"] == "developer"
                && membership["role"] == "project_operator")
    );
}

#[tokio::test]
async fn stored_project_admin_membership_can_manage_project_memberships() {
    let store = FakeStore::default();
    store.ensure_default_projects();
    store
        .project_memberships
        .lock()
        .expect("project memberships mutex")
        .push(ProjectMembershipRecord {
            id: "finance_reader_admin".to_string(),
            tenant_id: "default".to_string(),
            project_id: "finance".to_string(),
            principal_kind: "user".to_string(),
            principal_name: "reader".to_string(),
            role: "project_admin".to_string(),
            created_by: "system".to_string(),
            created_at: "2026-06-24 00:00:00+00".to_string(),
            updated_at: "2026-06-24 00:00:00+00".to_string(),
        });
    let app = authenticated_app(store);
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/projects/finance/memberships")
                .header("authorization", "Bearer viewer-token-0123456789-abcdefgh")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "principal_kind": "user",
                        "principal_name": "developer",
                        "role": "project_operator"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn viewer_cannot_submit_work_but_operator_can() {
    let app = authenticated_app(FakeStore::with_definition("auth_job"));
    let request_body = json!({
        "job_definition_id": "auth_job",
        "execution_pool": "mini"
    })
    .to_string();
    let viewer_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/jobs/runs")
                .header("content-type", "application/json")
                .header("authorization", "Bearer viewer-token-0123456789-abcdefgh")
                .body(Body::from(request_body.clone()))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(viewer_response.status(), 403);

    let operator_response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/jobs/runs")
                .header("content-type", "application/json")
                .header("authorization", "Bearer operator-token-0123456789-abcdef")
                .body(Body::from(request_body))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(operator_response.status(), 201);
}

#[tokio::test]
async fn current_principal_reports_authenticated_identity() {
    let response = authenticated_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .uri("/v1/auth/me")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), 200);
    assert_eq!(
        response_json(response).await,
        json!({
            "name":"owner",
            "role":"admin",
            "platform_admin":true,
            "tenant_id":"default",
            "project_id":"default",
            "project_memberships":[
                {"tenant_id":"default","project_id":"default","role":"project_admin"}
            ],
            "scopes":["*"]
        })
    );
}

#[tokio::test]
async fn current_principal_reports_project_memberships() {
    let response = authenticated_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .uri("/v1/auth/me")
                .header(
                    "authorization",
                    "Bearer finance-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), 200);
    assert_eq!(
        response_json(response).await["project_memberships"],
        json!([
            {"tenant_id":"default","project_id":"finance","role":"project_operator"}
        ])
    );
}

#[tokio::test]
async fn lists_only_projects_visible_to_the_principal() {
    let response = authenticated_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .uri("/v1/projects")
                .header(
                    "authorization",
                    "Bearer finance-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), 200);
    assert_eq!(
        response_json(response).await,
        json!({
            "projects": [
                {"id":"finance","tenant_id":"default","name":"Finance"}
            ]
        })
    );
}

#[tokio::test]
async fn scoped_token_can_only_use_declared_permissions() {
    let app = authenticated_app(FakeStore::with_definition("scoped_job"));
    let run_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/jobs/runs")
                .header("content-type", "application/json")
                .header("authorization", "Bearer ci-runner-token-0123456789-abcdef")
                .body(Body::from(
                    json!({
                        "job_definition_id": "scoped_job",
                        "execution_pool": "mini"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(run_response.status(), 201);

    let workflow_response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/workflows")
                .header("content-type", "application/json")
                .header(
                    "authorization",
                    "Bearer ci-runner-token-0123456789-abcdef",
                )
                .body(Body::from(
                    json!({
                        "id": "scoped_workflow",
                        "name": "Scoped Workflow",
                        "steps": [
                            { "id": "only", "name": "Only", "job_definition_id": "scoped_job", "execution_pool": "mini" }
                        ]
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(workflow_response.status(), 403);
}
