//! Ingestion runs and the review queues they feed: entity resolutions, candidate claims and conflicts.

use super::*;

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "integration-style endpoint flow stays readable when kept in request order"
)]
async fn local_text_connector_run_creates_candidate_memory() {
    let app = authenticated_app(FakeStore::default());
    let token = "Bearer admin-token-0123456789-abcdefghijkl";
    let canonical_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/memory/canonical-entities")
                .header("authorization", token)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "canonical_project_atlas",
                        "entity_type": "Project",
                        "display_name": "Project Atlas",
                        "aliases": ["atlas"]
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(canonical_response.status(), axum::http::StatusCode::CREATED);

    let connector_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/ingestion/connectors")
                .header("authorization", token)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "connector_project_notes",
                        "name": "Project notes",
                        "kind": "local_text",
                        "config": {
                            "title": "Project Atlas Notes",
                            "content": "# Project Atlas\n\n- Project Atlas is blocked by Legal Review\n- Sarah approved Project Atlas\n",
                            "content_type": "text/markdown",
                            "uri": "local://project-atlas.md",
                            "authority": "high"
                        },
                        "enabled": true
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(connector_response.status(), axum::http::StatusCode::CREATED);

    let run_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/ingestion/connectors/connector_project_notes/runs")
                .header("authorization", token)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(run_response.status(), axum::http::StatusCode::CREATED);
    let body = response_json(run_response).await;
    assert_eq!(body["run"]["status"], "succeeded");
    assert_eq!(body["run"]["source_count"], 1);
    assert_eq!(body["run"]["claim_count"], 2);
    assert_eq!(
        body["outputs"]["sources"]
            .as_array()
            .expect("sources")
            .len(),
        1
    );
    let evidence = body["outputs"]["evidence"].as_array().expect("evidence");
    assert!(!evidence.is_empty(), "{evidence:?}");
    assert_eq!(
        body["outputs"]["claims"].as_array().expect("claims").len(),
        2
    );
    let run_id = body["run"]["id"].as_str().expect("a run id").to_string();
    for uri in [
        "/v1/ingestion/connectors/connector_project_notes".to_string(),
        format!("/v1/ingestion/runs/{run_id}"),
    ] {
        succeeds(&app, admin_request(Method::GET, &uri, None)).await;
    }

    let resolutions_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/memory/entity-resolutions?status=candidate")
                .header("authorization", token)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(resolutions_response.status(), axum::http::StatusCode::OK);
    let body = response_json(resolutions_response).await;
    let resolutions = body["entity_resolutions"]
        .as_array()
        .expect("entity resolutions");
    assert_eq!(resolutions.len(), 1);
    assert_eq!(
        resolutions[0]["canonical_entity_id"],
        "canonical_project_atlas"
    );
    assert_eq!(resolutions[0]["status"], "candidate");
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "integration-style endpoint flow stays readable when kept in request order"
)]
async fn entity_resolution_queue_confirms_and_rejects_candidates() {
    let app = authenticated_app(FakeStore::default());
    let token = "Bearer admin-token-0123456789-abcdefghijkl";
    for (uri, body) in [
        (
            "/v1/memory/sources",
            json!({
                "id": "source_resolution",
                "kind": "local_text",
                "title": "Resolution notes",
                "authority": "high"
            }),
        ),
        (
            "/v1/memory/evidence",
            json!({
                "id": "evidence_resolution",
                "source_id": "source_resolution",
                "locator": "chunk:1",
                "excerpt": "Project Atlas appears in project notes",
                "observed_at": "ingestion"
            }),
        ),
        (
            "/v1/memory/entities",
            json!({
                "id": "entity_project_atlas",
                "entity_type": "DocumentTopic",
                "name": "Project Atlas",
                "aliases": []
            }),
        ),
        (
            "/v1/memory/canonical-entities",
            json!({
                "id": "canonical_project_atlas",
                "entity_type": "Project",
                "display_name": "Project Atlas",
                "aliases": []
            }),
        ),
        (
            "/v1/memory/subgraphs",
            json!({ "id": "global", "name": "Global memory" }),
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header("authorization", token)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    }

    for id in ["resolution_candidate_one", "resolution_candidate_two"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/entity-resolutions")
                    .header("authorization", token)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "id": id,
                            "subgraph_id": "global",
                            "entity_id": "entity_project_atlas",
                            "canonical_entity_id": "canonical_project_atlas",
                            "confidence": 0.9,
                            "status": "candidate",
                            "evidence_ids": ["evidence_resolution"]
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    }

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/memory/entity-resolutions?status=candidate")
                .header("authorization", token)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(list_response.status(), axum::http::StatusCode::OK);
    let body = response_json(list_response).await;
    assert_eq!(
        body["entity_resolutions"]
            .as_array()
            .expect("entity resolutions")
            .len(),
        2
    );

    let confirm_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/memory/entity-resolutions/resolution_candidate_one/confirm")
                .header("authorization", token)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(confirm_response.status(), axum::http::StatusCode::OK);
    assert_eq!(response_json(confirm_response).await["status"], "confirmed");

    let reject_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/memory/entity-resolutions/resolution_candidate_two/reject")
                .header("authorization", token)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(reject_response.status(), axum::http::StatusCode::OK);
    assert_eq!(response_json(reject_response).await["status"], "rejected");
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "integration-style endpoint flow stays readable when kept in request order"
)]
async fn ingestion_review_queue_approves_and_rejects_candidate_claims() {
    let app = authenticated_app(FakeStore::default());
    let token = "Bearer admin-token-0123456789-abcdefghijkl";
    let connector_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/ingestion/connectors")
                .header("authorization", token)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "connector_review_notes",
                        "name": "Review notes",
                        "kind": "local_text",
                        "config": {
                            "title": "Review Notes",
                            "content": "# Project Atlas\n\n- Project Atlas is blocked by Legal Review\n- Sarah approved Project Atlas\n",
                            "content_type": "text/markdown",
                            "authority": "high"
                        }
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(connector_response.status(), axum::http::StatusCode::CREATED);

    let run_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/ingestion/connectors/connector_review_notes/runs")
                .header("authorization", token)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(run_response.status(), axum::http::StatusCode::CREATED);

    let review_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/ingestion/review/claims?status=candidate")
                .header("authorization", token)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(review_response.status(), axum::http::StatusCode::OK);
    let body = response_json(review_response).await;
    let claims = body["claims"].as_array().expect("claims");
    assert_eq!(claims.len(), 2);
    assert_eq!(claims[0]["status"], "candidate");
    assert!(
        claims[0]["evidence"][0]["excerpt"]
            .as_str()
            .expect("evidence excerpt")
            .contains("Project Atlas")
    );
    assert_eq!(claims[0]["sources"][0]["title"], "Review Notes");
    assert_eq!(claims[0]["sources"][0]["authority"], "high");

    let first_claim_id = claims[0]["id"].as_str().expect("claim id");
    let second_claim_id = claims[1]["id"].as_str().expect("claim id");
    let approve_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/v1/ingestion/review/claims/{first_claim_id}/approve"
                ))
                .header("authorization", token)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(approve_response.status(), axum::http::StatusCode::OK);
    let approved = response_json(approve_response).await;
    assert_eq!(approved["status"], "active");
    assert!(
        approved["evidence"][0]["excerpt"]
            .as_str()
            .expect("approved evidence excerpt")
            .contains("Project Atlas")
    );

    let reject_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/v1/ingestion/review/claims/{second_claim_id}/reject"
                ))
                .header("authorization", token)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(reject_response.status(), axum::http::StatusCode::OK);
    assert_eq!(response_json(reject_response).await["status"], "rejected");
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "integration-style endpoint flow stays readable when kept in request order"
)]
async fn approving_claim_detects_and_reviews_conflicting_active_claims() {
    let app = authenticated_app(FakeStore::default());
    let token = "Bearer admin-token-0123456789-abcdefghijkl";
    for (uri, body) in [
        (
            "/v1/memory/sources",
            json!({
                "id": "source_conflict",
                "kind": "local_text",
                "title": "Conflict notes",
                "authority": "high"
            }),
        ),
        (
            "/v1/memory/evidence",
            json!({
                "id": "evidence_conflict",
                "source_id": "source_conflict",
                "locator": "chunk:1",
                "excerpt": "Project Atlas launch date changed",
                "observed_at": "ingestion"
            }),
        ),
        (
            "/v1/memory/entities",
            json!({
                "id": "entity_conflict_project",
                "entity_type": "Project",
                "name": "Project Atlas",
                "aliases": []
            }),
        ),
        (
            "/v1/memory/claims",
            json!({
                "id": "claim_launch_july",
                "subject_id": "entity_conflict_project",
                "predicate": "launch_date",
                "object": "July 15",
                "evidence_ids": ["evidence_conflict"],
                "confidence": 0.81,
                "authority": "medium",
                "status": "active",
                "observed_at": "2026-07-01"
            }),
        ),
        (
            "/v1/memory/claims",
            json!({
                "id": "claim_launch_august",
                "subject_id": "entity_conflict_project",
                "predicate": "launch_date",
                "object": "August 1",
                "evidence_ids": ["evidence_conflict"],
                "confidence": 0.9,
                "authority": "high",
                "status": "candidate",
                "observed_at": "2026-07-04"
            }),
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header("authorization", token)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    }

    let approve_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/ingestion/review/claims/claim_launch_august/approve")
                .header("authorization", token)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(approve_response.status(), axum::http::StatusCode::OK);

    let conflicts_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/memory/conflicts?status=candidate")
                .header("authorization", token)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(conflicts_response.status(), axum::http::StatusCode::OK);
    let body = response_json(conflicts_response).await;
    let conflicts = body["conflicts"].as_array().expect("conflicts");
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0]["predicate"], "launch_date");
    assert_eq!(conflicts[0]["status"], "candidate");
    let claim_ids = conflicts[0]["claim_ids"].as_array().expect("claim ids");
    assert!(claim_ids.iter().any(|id| id == "claim_launch_july"));
    assert!(claim_ids.iter().any(|id| id == "claim_launch_august"));

    let conflict_id = conflicts[0]["id"].as_str().expect("conflict id");
    let resolve_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/memory/conflicts/{conflict_id}/resolve"))
                .header("authorization", token)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "preferred_claim_id": "claim_launch_august" }).to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(resolve_response.status(), axum::http::StatusCode::OK);
    assert_eq!(response_json(resolve_response).await["status"], "resolved");

    let dismiss_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/memory/conflicts/{conflict_id}/dismiss"))
                .header("authorization", token)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        dismiss_response.status(),
        axum::http::StatusCode::BAD_REQUEST
    );
}
