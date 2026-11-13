//! Claim memory, memory contracts and the memory graph over the API.

use super::*;

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end memory API contract covers source, evidence, entity, and claim wiring"
)]
async fn creates_lists_and_reads_claim_memory() {
    let app = authenticated_app(FakeStore::default());

    let source_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/memory/sources")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "source_exec_update",
                        "kind": "executive_update",
                        "uri": "file:///updates/atlas.md",
                        "title": "Atlas executive update",
                        "authority": "high"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(source_response.status(), axum::http::StatusCode::CREATED);

    let evidence_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/memory/evidence")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "evidence_exec_update",
                        "source_id": "source_exec_update",
                        "locator": "updates/atlas.md#L12",
                        "excerpt": "Project Atlas launch date moved to August 1.",
                        "observed_at": "2026-07-05T10:00:00Z"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(evidence_response.status(), axum::http::StatusCode::CREATED);

    let entity_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/memory/entities")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "project_atlas",
                        "entity_type": "Project",
                        "name": "Project Atlas",
                        "aliases": ["Atlas"]
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(entity_response.status(), axum::http::StatusCode::CREATED);

    let claim_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/memory/claims")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "claim_launch_august",
                        "subject_id": "project_atlas",
                        "predicate": "launch_date",
                        "object": "2026-08-01",
                        "evidence_ids": ["evidence_exec_update"],
                        "confidence": 0.93,
                        "authority": "high",
                        "status": "active",
                        "observed_at": "2026-07-05T10:00:00Z",
                        "valid_from": "2026-07-05T00:00:00Z"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(claim_response.status(), axum::http::StatusCode::CREATED);

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/memory/claims")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(list_response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(list_response).await["claims"][0]["evidence_ids"],
        json!(["evidence_exec_update"])
    );

    let get_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/memory/claims/claim_launch_august")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(get_response.status(), axum::http::StatusCode::OK);
    assert_eq!(response_json(get_response).await["status"], "active");
}

#[tokio::test]
async fn creates_lists_and_reads_memory_contract() {
    let app = authenticated_app(FakeStore::default());
    let dsl = r"
entity Project:
  fields:
    name: string
    owner: Person

entity Person:
  fields:
    name: string

relation owns:
  from: Person
  to: Project

claim_policy:
  require_source: true
  store_confidence: true
  allow_contradictions: true
  min_confidence: 0.8

trust_policy:
  source_priority:
    - legal_contracts
    - board_minutes
";

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/memory/contracts")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "contract_project_memory",
                        "name": "Project memory",
                        "source": dsl
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(create_response.status(), axum::http::StatusCode::CREATED);
    let body = response_json(create_response).await;
    assert_eq!(
        body["compiled"]["entity_types"],
        json!(["Project", "Person"])
    );
    assert_eq!(
        body["compiled"]["claim_policy"]["min_confidence"],
        json!(0.8)
    );

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/memory/contracts")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(list_response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(list_response).await["contracts"][0]["id"],
        "contract_project_memory"
    );

    let get_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/memory/contracts/contract_project_memory")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(get_response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(get_response).await["compiled"]["relations"][0]["name"],
        "owns"
    );
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "nested memory graph API test covers the complete boundary-edge activation flow"
)]
async fn creates_and_activates_nested_memory_graph() {
    let app = authenticated_app(FakeStore::default());
    let auth = (
        "authorization",
        "Bearer admin-token-0123456789-abcdefghijkl",
    );

    for (uri, body) in [
        (
            "/v1/memory/sources",
            json!({
                "id": "source_nested",
                "kind": "roadmap",
                "title": "Nested roadmap",
                "authority": "high"
            }),
        ),
        (
            "/v1/memory/evidence",
            json!({
                "id": "evidence_nested",
                "source_id": "source_nested",
                "locator": "roadmap.md#L1",
                "excerpt": "Sales says August and engineering says September.",
                "observed_at": "2026-07-02T00:00:00Z"
            }),
        ),
        (
            "/v1/memory/entities",
            json!({
                "id": "entity_capsulet",
                "entity_type": "Project",
                "name": "Capsulet",
                "aliases": []
            }),
        ),
        (
            "/v1/memory/claims",
            json!({
                "id": "claim_sales_summary",
                "subject_id": "entity_capsulet",
                "predicate": "summary",
                "object": "Sales and engineering disagree on target month.",
                "evidence_ids": ["evidence_nested"],
                "confidence": 0.9,
                "authority": "high",
                "status": "active",
                "observed_at": "2026-07-02T00:00:00Z"
            }),
        ),
        (
            "/v1/memory/contracts",
            json!({
                "id": "contract_nested",
                "name": "Nested contract",
                "source": "entity Project:\n  fields:\n    name: string\n\nclaim_policy:\n  require_source: true\n  store_confidence: true\n"
            }),
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(uri)
                    .header(auth.0, auth.1)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    }

    for body in [
        json!({
            "id": "subgraph_sales",
            "name": "Sales memory",
            "description": "Sales commitments"
        }),
        json!({
            "id": "subgraph_engineering",
            "name": "Engineering memory"
        }),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/memory/subgraphs")
                    .header(auth.0, auth.1)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    }

    for (uri, body) in [
        (
            "/v1/memory/subgraphs/subgraph_sales/members",
            json!({
                "id": "member_sales_summary",
                "member_kind": "claim",
                "member_id": "claim_sales_summary",
                "role": "summary"
            }),
        ),
        (
            "/v1/memory/canonical-entities",
            json!({
                "id": "canonical_capsulet",
                "entity_type": "Project",
                "display_name": "Capsulet",
                "aliases": []
            }),
        ),
        (
            "/v1/memory/entity-resolutions",
            json!({
                "id": "resolution_capsulet_sales",
                "subgraph_id": "subgraph_sales",
                "entity_id": "entity_capsulet",
                "canonical_entity_id": "canonical_capsulet",
                "confidence": 0.95,
                "status": "confirmed",
                "evidence_ids": ["evidence_nested"]
            }),
        ),
        (
            "/v1/memory/summary-traces",
            json!({
                "id": "trace_sales_summary",
                "subgraph_id": "subgraph_sales",
                "summary_claim_id": "claim_sales_summary",
                "inner_claim_ids": [],
                "evidence_ids": ["evidence_nested"]
            }),
        ),
        (
            "/v1/memory/entity-graph-attachments",
            json!({
                "id": "attachment_capsulet",
                "canonical_entity_id": "canonical_capsulet",
                "subgraph_id": "subgraph_sales",
                "attachment_type": "primary"
            }),
        ),
        (
            "/v1/memory/subgraph-edges",
            json!({
                "id": "edge_sales_engineering",
                "edge_type": "contradicts",
                "from_subgraph_id": "subgraph_sales",
                "to_subgraph_id": "subgraph_engineering",
                "from_member_kind": "claim",
                "from_member_id": "claim_sales_summary",
                "to_member_kind": "claim",
                "to_member_id": "claim_engineering_summary",
                "claim_ids": ["claim_sales_summary"],
                "evidence_ids": ["evidence_nested"]
            }),
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(uri)
                    .header(auth.0, auth.1)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    }

    let activate_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/memory/subgraphs/subgraph_sales/activate")
                .header(auth.0, auth.1)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "owner_kind": "team",
                        "owner_id": "sales",
                        "contract_id": "contract_nested",
                        "permissions": { "read": ["sales"] },
                        "summary_claim_id": "claim_sales_summary"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(activate_response.status(), axum::http::StatusCode::OK);
    assert_eq!(response_json(activate_response).await["status"], "active");
    let subgraph = succeeds(
        &app,
        admin_request(Method::GET, "/v1/memory/subgraphs/subgraph_sales", None),
    )
    .await;
    assert_eq!(subgraph["status"], "active");
}

fn project_request(method: Method, uri: &str, project: &str, body: Option<Value>) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(
            "authorization",
            "Bearer admin-token-0123456789-abcdefghijkl",
        )
        .header("x-capsulet-project-id", project)
        .header("content-type", "application/json")
        .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
        .expect("request")
}

#[tokio::test]
async fn memory_contracts_are_listed_within_the_project_before_the_limit() {
    let dsl = "entity Project:\n  fields:\n    name: string\n";
    let store = FakeStore::default();
    {
        // A hundred of another project's contracts come first, filling the listing limit.
        let mut contracts = store.memory_contracts.lock().expect("contracts mutex");
        for index in 0..100 {
            contracts.push(
                MemoryContract::parse_scoped(
                    MemoryContractId::new(format!("contract_finance_{index}")).expect("id"),
                    capsulet_core::MemoryScope::new("default", "finance").expect("scope"),
                    "Finance",
                    dsl,
                )
                .expect("contract"),
            );
        }
    }
    let app = authenticated_app(store);

    let created = app
        .clone()
        .oneshot(project_request(
            Method::POST,
            "/v1/memory/contracts",
            "default",
            Some(json!({ "id": "contract_default", "name": "Default", "source": dsl })),
        ))
        .await
        .expect("response");
    assert_eq!(created.status(), axum::http::StatusCode::CREATED);
    let listed = app
        .oneshot(project_request(
            Method::GET,
            "/v1/memory/contracts",
            "default",
            None,
        ))
        .await
        .expect("response");

    assert_eq!(listed.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(listed).await["contracts"]
            .as_array()
            .expect("contracts")
            .iter()
            .map(|contract| contract["id"].clone())
            .collect::<Vec<_>>(),
        vec![json!("contract_default")],
        "another project's contracts must not crowd this project's out of the listing"
    );
}

#[tokio::test]
async fn nothing_attaches_to_another_projects_subgraph_over_the_api() {
    let store = FakeStore::default();
    let app = authenticated_app(store.clone());
    for (project, id) in [
        ("finance", "subgraph_finance"),
        ("default", "subgraph_default"),
    ] {
        let created = app
            .clone()
            .oneshot(project_request(
                Method::POST,
                "/v1/memory/subgraphs",
                project,
                Some(json!({ "id": id, "name": "Memory" })),
            ))
            .await
            .expect("response");
        assert_eq!(created.status(), axum::http::StatusCode::CREATED);
    }

    for (uri, body) in writes_naming_subgraph_finance() {
        let refused = app
            .clone()
            .oneshot(project_request(Method::POST, uri, "default", Some(body)))
            .await
            .expect("response");
        assert_eq!(
            refused.status(),
            axum::http::StatusCode::NOT_FOUND,
            "{uri} must not reach another project's subgraph"
        );
        assert_eq!(response_json(refused).await["code"], "memory_not_found");
    }

    assert!(
        store
            .memory_subgraph_members
            .lock()
            .expect("members")
            .is_empty()
    );
    assert!(
        store
            .memory_subgraph_edges
            .lock()
            .expect("edges")
            .is_empty()
    );
    assert!(
        store
            .memory_summary_traces
            .lock()
            .expect("traces")
            .is_empty()
    );
    assert!(
        store
            .memory_entity_resolutions
            .lock()
            .expect("resolutions")
            .is_empty()
    );
    assert!(
        store
            .memory_entity_graph_attachments
            .lock()
            .expect("attachments")
            .is_empty()
    );
    assert_eq!(store.memory_subgraphs.lock().expect("subgraphs").len(), 2);
}

/// Every memory write that names a subgraph, each naming `subgraph_finance`.
fn writes_naming_subgraph_finance() -> Vec<(&'static str, Value)> {
    let edge = |from: &str, to: &str| {
        json!({
            "edge_type": "contradicts",
            "from_subgraph_id": from,
            "to_subgraph_id": to,
            "from_member_kind": "claim",
            "from_member_id": "claim_a",
            "to_member_kind": "claim",
            "to_member_id": "claim_b",
            "claim_ids": ["claim_a"],
            "evidence_ids": []
        })
    };
    vec![
        (
            "/v1/memory/subgraphs/subgraph_finance/members",
            json!({ "member_kind": "claim", "member_id": "claim_a", "role": "summary" }),
        ),
        (
            "/v1/memory/subgraph-edges",
            edge("subgraph_default", "subgraph_finance"),
        ),
        (
            "/v1/memory/subgraph-edges",
            edge("subgraph_finance", "subgraph_default"),
        ),
        (
            "/v1/memory/summary-traces",
            json!({
                "subgraph_id": "subgraph_finance",
                "summary_claim_id": "claim_a",
                "inner_claim_ids": [],
                "evidence_ids": ["evidence_a"]
            }),
        ),
        (
            "/v1/memory/entity-resolutions",
            json!({
                "subgraph_id": "subgraph_finance",
                "entity_id": "entity_a",
                "canonical_entity_id": "canonical_a",
                "confidence": 0.9,
                "status": "candidate",
                "evidence_ids": []
            }),
        ),
        (
            "/v1/memory/entity-graph-attachments",
            json!({
                "canonical_entity_id": "canonical_a",
                "subgraph_id": "subgraph_finance",
                "attachment_type": "supporting"
            }),
        ),
        (
            "/v1/memory/subgraphs",
            json!({
                "id": "subgraph_child",
                "parent_subgraph_id": "subgraph_finance",
                "name": "Child"
            }),
        ),
    ]
}

#[tokio::test]
async fn another_projects_summary_trace_cannot_activate_a_subgraph() {
    let store = FakeStore::default();
    let app = authenticated_app(store.clone());
    let created = app
        .clone()
        .oneshot(project_request(
            Method::POST,
            "/v1/memory/subgraphs",
            "default",
            Some(json!({ "id": "subgraph_sales", "name": "Sales memory" })),
        ))
        .await
        .expect("response");
    assert_eq!(created.status(), axum::http::StatusCode::CREATED);
    // A trace another project wrote against this subgraph before writes were checked.
    store.memory_summary_traces.lock().expect("traces").push(
        SummaryTrace::new(
            capsulet_core::SummaryTraceId::new("trace_planted").expect("id"),
            capsulet_core::MemoryScope::new("default", "finance").expect("scope"),
            capsulet_core::MemorySubgraphId::new("subgraph_sales").expect("id"),
            ClaimId::new("claim_summary").expect("id"),
            Vec::new(),
            vec![EvidenceId::new("evidence_planted").expect("id")],
        )
        .expect("trace"),
    );

    let activated = app
        .oneshot(project_request(
            Method::POST,
            "/v1/memory/subgraphs/subgraph_sales/activate",
            "default",
            Some(json!({
                "owner_kind": "team",
                "owner_id": "sales",
                "contract_id": "contract_sales",
                "permissions": { "read": ["sales"] },
                "summary_claim_id": "claim_summary"
            })),
        ))
        .await
        .expect("response");

    assert_eq!(
        activated.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "the summary has no trace in its own project"
    );
    assert_eq!(
        store.memory_subgraphs.lock().expect("subgraphs")[0].status(),
        capsulet_core::MemorySubgraphStatus::Draft
    );
}
