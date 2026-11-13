//! The ask loop's contracts and queries, and the decisions people record on readings.

use super::*;

#[tokio::test]
async fn reasoning_under_another_project_s_contract_finds_no_contract_and_stores_nothing() {
    // The contract is resolved before the source is stored, and one in another
    // project reports as missing, the way every memory route reports it.
    let store = FakeStore::default();
    let app = authenticated_app(store.clone());
    let dsl = "entity Organization:\n  fields:\n    name: string\n\nrelation acquired:\n  from: Organization\n  to: Organization\n  stated_by:\n    - \"{from} acquired {to}\"\n\nclaim_policy:\n  require_source: true\n";

    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/memory/contracts")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .header("x-capsulet-project-id", "finance")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "id": "contract_deals", "name": "Deals", "source": dsl }).to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(created.status(), axum::http::StatusCode::CREATED);

    let asked = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/reasoning/ask")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .header("x-capsulet-project-id", "default")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "text": "Fabrikam acquired Contoso.",
                        "question": "Who acquired Contoso?",
                        "contract_id": "contract_deals"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(asked.status(), axum::http::StatusCode::NOT_FOUND);
    assert_eq!(response_json(asked).await["code"], "memory_not_found");
    assert!(
        store
            .memory_sources
            .lock()
            .expect("memory sources mutex")
            .is_empty(),
        "nothing should be stored for a request refused at its contract"
    );
}

/// A reasoning certificate in the default project that leaves `readings` readings
/// open, or that the kernel rejected.
fn reasoning_certificate(
    id: &str,
    verdict: capsulet_kernel::Verdict,
    readings: usize,
) -> CertificateRecord {
    let goal = capsulet_kernel::Proposition::new("either party", "may_terminate", "this agreement");
    CertificateRecord {
        id: id.to_string(),
        tenant_id: "default".to_string(),
        project_id: "default".to_string(),
        question: "May either party terminate the agreement?".to_string(),
        certificate: capsulet_kernel::Certificate {
            verdict,
            goal: goal.clone(),
            discharged: Vec::new(),
            residuals: (0..readings)
                .map(|index| capsulet_kernel::Residual {
                    kind: capsulet_kernel::ResidualKind::Source,
                    from: "mentions(contract,either party|may_terminate|this agreement)"
                        .to_string(),
                    to: goal.clone(),
                    rationale: format!("the clause is qualified by provision 12 ({index})"),
                    evidence_ids: vec!["ev_termination".to_string()],
                })
                .collect(),
            errors: Vec::new(),
            replay_digest: format!("sha256:{id}"),
            derivation_depth_limit: None,
            reading_authority: None,
        },
        alphabet_digest: "sha256:alphabet".to_string(),
        model: "mini".to_string(),
    }
}

async fn reasoning_request(
    app: &axum::Router,
    method: Method,
    uri: &str,
    project: &str,
    body: Option<Value>,
) -> (axum::http::StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .header("x-capsulet-project-id", project)
                .header("content-type", "application/json")
                .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    (status, response_json(response).await)
}

fn reading_of(record: &CertificateRecord, index: usize) -> String {
    capsulet_kernel::reading_digest(&record.certificate, &record.certificate.residuals[index])
}

#[tokio::test]
async fn reviewing_every_reading_accepts_an_answer_by_review_and_a_decision_is_not_rewritten() {
    let store = FakeStore::default();
    let record = reasoning_certificate(
        "certificate_terms",
        capsulet_kernel::Verdict::Conditional,
        2,
    );
    store.insert_certificate(&record).await.expect("store");
    let app = authenticated_app(store.clone());

    let (status, open) =
        reasoning_request(&app, Method::GET, "/v1/reasoning/readings", "default", None).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{open}");
    assert_eq!(open["readings"].as_array().map(Vec::len), Some(2), "{open}");
    assert_eq!(open["readings"][0]["certificate_id"], "certificate_terms");

    let review = |index: usize| {
        format!(
            "/v1/reasoning/certificates/certificate_terms/readings/{}/review",
            reading_of(&record, index)
        )
    };
    let holds =
        json!({ "decision": "holds", "rationale": "read with provision 12, it still holds" });
    let (status, first) = reasoning_request(
        &app,
        Method::POST,
        &review(0),
        "default",
        Some(holds.clone()),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CREATED, "{first}");
    assert_eq!(first["verdict"], "conditional");
    assert_eq!(first["readings"][0]["state"], "holds");
    assert_eq!(first["readings"][0]["authority"], "review:write");
    assert_eq!(first["readings"][1]["state"], "open");

    let (status, second) = reasoning_request(
        &app,
        Method::POST,
        &review(1),
        "default",
        Some(holds.clone()),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CREATED, "{second}");
    assert_eq!(second["kernel_verdict"], "conditional");
    assert_eq!(second["verdict"], "accepted");
    assert_eq!(second["basis"], "human_review");

    let (status, again) =
        reasoning_request(&app, Method::POST, &review(1), "default", Some(holds)).await;
    assert_eq!(status, axum::http::StatusCode::CONFLICT, "{again}");
    assert_eq!(again["code"], "reading_already_reviewed");

    let (status, read) = reasoning_request(
        &app,
        Method::GET,
        "/v1/reasoning/certificates/certificate_terms/review",
        "default",
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{read}");
    assert_eq!(read["verdict"], "accepted");
    let (_, open) =
        reasoning_request(&app, Method::GET, "/v1/reasoning/readings", "default", None).await;
    assert_eq!(open["readings"].as_array().map(Vec::len), Some(0), "{open}");
}

#[tokio::test]
async fn a_reading_whose_contract_names_its_authority_is_decided_only_by_a_holder_of_it() {
    let store = FakeStore::default();
    store.ensure_default_projects();
    for name in ["reader", "counsel"] {
        store
            .project_memberships
            .lock()
            .expect("project memberships mutex")
            .push(ProjectMembershipRecord {
                id: format!("finance_{name}_admin"),
                tenant_id: "default".to_string(),
                project_id: "finance".to_string(),
                principal_kind: "user".to_string(),
                principal_name: name.to_string(),
                role: "project_admin".to_string(),
                created_by: "system".to_string(),
                created_at: "2026-11-13 00:00:00+00".to_string(),
                updated_at: "2026-11-13 00:00:00+00".to_string(),
            });
    }
    let mut record = reasoning_certificate(
        "certificate_clause",
        capsulet_kernel::Verdict::Conditional,
        1,
    );
    record.project_id = "finance".to_string();
    record.certificate.reading_authority = Some("review:legal".to_string());
    store.insert_certificate(&record).await.expect("store");
    let auth = AuthConfig::from_json(
        r#"[
            {"name":"reader","role":"viewer","token":"viewer-token-0123456789-abcdefgh"},
            {"name":"counsel","role":"viewer","token":"counsel-token-0123456789-abcdefgh",
             "scopes":["review:read","review:write","review:legal","certificates:read"]}
        ]"#,
    )
    .expect("auth config");
    let app = router(
        AppState::new(store, FakeObjectStore::default(), ["mini".to_string()]).with_auth(auth),
    );
    let send = |token: &'static str, method: Method, uri: String, body: Option<Value>| {
        let app = app.clone();
        async move {
            let response = app
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("authorization", format!("Bearer {token}"))
                        .header("x-capsulet-project-id", "finance")
                        .header("content-type", "application/json")
                        .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))
                        .expect("request"),
                )
                .await
                .expect("response");
            let status = response.status();
            (status, response_json(response).await)
        }
    };
    let general = "viewer-token-0123456789-abcdefgh";
    let legal = "counsel-token-0123456789-abcdefgh";

    let (status, open) = send(
        general,
        Method::GET,
        "/v1/reasoning/readings".to_string(),
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{open}");
    assert_eq!(
        open["readings"][0]["reading"]["review_authority"],
        "review:legal"
    );

    let review = format!(
        "/v1/reasoning/certificates/certificate_clause/readings/{}/review",
        reading_of(&record, 0)
    );
    let holds = json!({ "decision": "holds", "rationale": "read with Section 9.3 it still holds" });
    let (status, refused) = send(general, Method::POST, review.clone(), Some(holds.clone())).await;
    assert_eq!(
        status,
        axum::http::StatusCode::FORBIDDEN,
        "a project admin without review:legal decides nothing here: {refused}"
    );
    assert_eq!(refused["code"], "permission_denied");
    assert!(
        refused["message"]
            .as_str()
            .is_some_and(|message| message.ends_with("required scope: review:legal")),
        "refused by the contract's authority, not the endpoint's: {refused}"
    );

    let (status, decided) = send(legal, Method::POST, review, Some(holds)).await;
    assert_eq!(status, axum::http::StatusCode::CREATED, "{decided}");
    assert_eq!(decided["verdict"], "accepted");
    assert_eq!(decided["basis"], "human_review");
    assert_eq!(decided["readings"][0]["authority"], "review:legal");
    assert_eq!(decided["readings"][0]["reviewer"], "counsel");
}

#[tokio::test]
async fn a_reading_a_reviewer_refutes_rejects_the_answer() {
    let store = FakeStore::default();
    let record = reasoning_certificate(
        "certificate_refuted",
        capsulet_kernel::Verdict::Conditional,
        1,
    );
    store.insert_certificate(&record).await.expect("store");
    let app = authenticated_app(store);
    let (status, body) = reasoning_request(
        &app,
        Method::POST,
        &format!(
            "/v1/reasoning/certificates/certificate_refuted/readings/{}/review",
            reading_of(&record, 0)
        ),
        "default",
        Some(json!({ "decision": "does_not_hold", "rationale": "section 12 bars termination in year one" })),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CREATED, "{body}");
    assert_eq!(body["verdict"], "rejected");
    assert!(body["basis"].is_null(), "{body}");
}

#[tokio::test]
async fn a_review_has_to_decide_something_say_why_and_name_a_reading_of_this_projects_certificate()
{
    let store = FakeStore::default();
    let record = reasoning_certificate(
        "certificate_scoped",
        capsulet_kernel::Verdict::Conditional,
        1,
    );
    store.insert_certificate(&record).await.expect("store");
    let rejected =
        reasoning_certificate("certificate_failed", capsulet_kernel::Verdict::Rejected, 1);
    store.insert_certificate(&rejected).await.expect("store");
    let app = authenticated_app(store.clone());
    let uri = format!(
        "/v1/reasoning/certificates/certificate_scoped/readings/{}/review",
        reading_of(&record, 0)
    );

    for body in [
        json!({ "decision": "probably", "rationale": "it reads fine" }),
        json!({ "decision": "holds", "rationale": "   " }),
    ] {
        let (status, refused) =
            reasoning_request(&app, Method::POST, &uri, "default", Some(body)).await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST, "{refused}");
    }

    let holds = json!({ "decision": "holds", "rationale": "it reads fine" });
    let (status, missing) = reasoning_request(
        &app,
        Method::POST,
        &uri,
        "another-project",
        Some(holds.clone()),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::NOT_FOUND, "{missing}");
    assert_eq!(missing["code"], "reasoning_certificate_not_found");

    let unknown = "/v1/reasoning/certificates/certificate_scoped/readings/sha256:nothing/review";
    let (status, _) =
        reasoning_request(&app, Method::POST, unknown, "default", Some(holds.clone())).await;
    assert_eq!(status, axum::http::StatusCode::NOT_FOUND);

    let failed = format!(
        "/v1/reasoning/certificates/certificate_failed/readings/{}/review",
        reading_of(&rejected, 0)
    );
    let (status, refused) =
        reasoning_request(&app, Method::POST, &failed, "default", Some(holds)).await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST, "{refused}");
    assert!(
        store
            .reading_reviews
            .lock()
            .expect("reviews mutex")
            .is_empty(),
        "nothing refused should be recorded"
    );
}

/// Asks the reasoning endpoint about Fabrikam and Contoso, under a contract declaring
/// `acquired`, with `query` as the typed question.
async fn ask_with_query(
    query: serde_json::Value,
) -> (axum::http::StatusCode, serde_json::Value, FakeStore) {
    let store = FakeStore::default();
    let app = authenticated_app(store.clone());
    let dsl = "entity Organization:\n  fields:\n    name: string\n\nrelation acquired:\n  from: Organization\n  to: Organization\n  stated_by:\n    - \"{from} acquired {to}\"\n\nclaim_policy:\n  require_source: true\n";
    let created = app
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
                    json!({ "id": "contract_deals", "name": "Deals", "source": dsl }).to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(created.status(), axum::http::StatusCode::CREATED);

    let asked = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/reasoning/ask")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "text": "Fabrikam acquired Contoso.",
                        "question": "Who acquired Contoso?",
                        "contract_id": "contract_deals",
                        "query": query
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    let status = asked.status();
    (status, response_json(asked).await, store)
}

#[tokio::test]
async fn reasoning_refuses_a_query_about_a_relation_its_contract_does_not_declare() {
    // An answer can only state a declared relation, so a query about any other one
    // could never be answered. It is refused before anything is stored.
    let (status, body, store) =
        ask_with_query(json!({ "relation": "owns_account", "object": "Contoso" })).await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .is_some_and(|message| message.contains("owns_account")),
        "{body}"
    );
    assert!(
        store
            .memory_sources
            .lock()
            .expect("memory sources mutex")
            .is_empty(),
        "nothing should be stored for a request refused at its query"
    );
}

#[tokio::test]
async fn reasoning_refuses_a_query_that_asks_for_nothing_in_particular() {
    // Naming neither end leaves nothing for an answer to be held to.
    let (status, body, store) = ask_with_query(json!({ "relation": "acquired" })).await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .is_some_and(|message| message.contains("acquired(?, ?)")),
        "{body}"
    );
    assert!(
        store
            .memory_sources
            .lock()
            .expect("memory sources mutex")
            .is_empty()
    );
}

#[tokio::test]
async fn reasoning_loads_typed_endpoints_and_policies_from_its_scoped_contract() {
    let store = FakeStore::default();
    let dsl = "entity Organization:\n  fields:\n    name: string\nrelation revenue:\n  from: Organization\n  to: money\n  stated_by:\n    - \"{from} is {to}\"\nstatement_policy:\n  default_currency: CAD\n  date_order: day_first\n";
    let contract = MemoryContract::parse(MemoryContractId::new("typed").expect("id"), "typed", dsl)
        .expect("contract");
    store
        .upsert_memory_contract(&contract)
        .await
        .expect("store");
    let state = AppState::new(store, FakeObjectStore::default(), ["mini".into()]);
    let context = crate::http::ProjectContext {
        tenant_id: "default".into(),
        project_id: "default".into(),
        role: crate::auth::ProjectRole::Admin,
    };
    let (book, authority) = crate::reasoning::contract_policy(&state, &context, "typed")
        .await
        .expect("scoped contract");
    assert_eq!(
        authority, None,
        "a contract that names none leaves it to review:write"
    );
    let cad = capsulet_kernel::Proposition::new("Acme", "revenue", "CAD 1200000");
    assert!(
        book.statement_in("Acme is $1.2 million.", &cad)
            .is_some_and(|s| s.residuals.is_empty())
    );
    let usd = capsulet_kernel::Proposition::new("Acme", "revenue", "USD 1200000");
    assert!(book.statement_in("Acme is $1.2 million.", &usd).is_none());
}

#[tokio::test]
async fn reasoning_refuses_a_stored_contract_that_does_not_compile() {
    let store = FakeStore::default();
    let contract = MemoryContract::parse(MemoryContractId::new("invalid").expect("id"), "invalid", "entity Thing:\n  fields:\n    name: string\nrelation value:\n  from: Thing\n  to: missing\n").expect("legacy stored contract parses");
    store
        .upsert_memory_contract(&contract)
        .await
        .expect("store");
    let state = AppState::new(store, FakeObjectStore::default(), ["mini".into()]);
    let context = crate::http::ProjectContext {
        tenant_id: "default".into(),
        project_id: "default".into(),
        role: crate::auth::ProjectRole::Admin,
    };
    assert!(
        crate::reasoning::contract_policy(&state, &context, "invalid")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn the_names_a_decision_reads_are_only_those_a_reviewer_confirmed() {
    use capsulet_core::{
        CanonicalEntityId, Confidence, Entity, EntityId, EntityResolution, EntityResolutionId,
        EntityResolutionStatus, EvidenceId, MemoryScope, MemorySubgraphId,
    };

    let store = FakeStore::default();
    let scope = MemoryScope::new("default", "default").expect("scope");
    store
        .upsert_memory_subgraph(
            &MemorySubgraph::draft(
                MemorySubgraphId::new("subgraph").expect("id"),
                scope.clone(),
                None,
                "Organizations",
                None,
            )
            .expect("subgraph"),
        )
        .await
        .expect("stored");
    for (id, name) in [
        ("e_ibm", "IBM"),
        ("e_long", "International Business Machines"),
        ("e_guess", "Big Blue"),
    ] {
        store
            .upsert_memory_entity(
                &Entity::new(
                    EntityId::new(id).expect("id"),
                    scope.clone(),
                    "organization",
                    name,
                    Vec::new(),
                )
                .expect("entity"),
            )
            .await
            .expect("stored");
    }
    for (id, entity, status) in [
        ("r1", "e_ibm", EntityResolutionStatus::Confirmed),
        ("r2", "e_long", EntityResolutionStatus::Confirmed),
        ("r3", "e_guess", EntityResolutionStatus::Candidate),
    ] {
        store
            .upsert_memory_entity_resolution(
                &EntityResolution::new(
                    EntityResolutionId::new(id).expect("id"),
                    scope.clone(),
                    MemorySubgraphId::new("subgraph").expect("id"),
                    EntityId::new(entity).expect("id"),
                    CanonicalEntityId::new("c_ibm").expect("id"),
                    Confidence::new(1.0).expect("confidence"),
                    status,
                    vec![EvidenceId::new("ev").expect("id")],
                )
                .expect("resolution"),
            )
            .await
            .expect("stored");
    }

    let state = AppState::new(store, FakeObjectStore::default(), ["mini".to_string()]);
    let context = crate::http::ProjectContext {
        tenant_id: "default".to_string(),
        project_id: "default".to_string(),
        role: crate::ProjectRole::Viewer,
    };
    let groups = crate::reasoning::project_name_groups(&state, &context)
        .await
        .expect("the groups load");

    assert_eq!(groups.len(), 1);
    assert!(groups[0].contains("IBM"));
    assert!(groups[0].contains("International Business Machines"));
    assert!(
        !groups[0].contains("Big Blue"),
        "a candidate resolution is not a review"
    );
    assert_eq!(groups[0].resolutions(), ["r1", "r2"]);
}
