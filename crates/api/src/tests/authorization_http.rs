use super::*;

const ACTORS: [(&str, Option<&str>, u8); 8] = [
    ("anonymous", None, 0),
    (
        "tenant member",
        Some("tenant-member-token-0123456789-abcdef"),
        1,
    ),
    (
        "project viewer",
        Some("matrix-viewer-token-0123456789-abcdef"),
        2,
    ),
    (
        "project operator",
        Some("matrix-operator-token-0123456789-abcdef"),
        3,
    ),
    (
        "project admin",
        Some("matrix-project-admin-token-0123456789-abcdef"),
        4,
    ),
    (
        "service account",
        Some("matrix-service-token-0123456789-abcdef"),
        3,
    ),
    (
        "wrong project",
        Some("matrix-wrong-project-token-0123456789-abcdef"),
        1,
    ),
    (
        "global admin",
        Some("matrix-global-admin-token-0123456789-abcdef"),
        5,
    ),
];

#[tokio::test]
async fn authorization_user_does_not_inherit_a_same_named_service_accounts_membership() {
    let store = FakeStore::default();
    store
        .project_memberships
        .lock()
        .expect("memberships")
        .push(ProjectMembershipRecord {
            id: "service-membership".to_string(),
            tenant_id: "default".to_string(),
            project_id: "finance".to_string(),
            principal_kind: "service_account".to_string(),
            principal_name: "viewer".to_string(),
            role: "project_admin".to_string(),
            created_by: "system".to_string(),
            created_at: "2026-10-11".to_string(),
            updated_at: "2026-10-11".to_string(),
        });
    let auth = AuthConfig::from_json(
        r#"[{"name":"viewer","role":"viewer","token":"matrix-viewer-token-0123456789-abcdef"}]"#,
    )
    .expect("auth");
    let app = router(
        AppState::new(store, FakeObjectStore::default(), ["mini".to_string()]).with_auth(auth),
    );
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/auth/me")
                .header(
                    "authorization",
                    "Bearer matrix-viewer-token-0123456789-abcdef",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let body = response_json(response).await;
    assert!(
        body["project_memberships"]
            .as_array()
            .expect("memberships")
            .iter()
            .all(|membership| membership["project_id"] != "finance"),
        "a different principal kind must not grant project access"
    );
}

fn authorization_router() -> axum::Router {
    authorization_router_with_store(FakeStore::default())
}

fn authorization_router_with_store(store: FakeStore) -> axum::Router {
    store
        .service_accounts
        .lock()
        .expect("accounts")
        .push(FakeServiceAccount {
            token_hash: crate::auth::token_digest("matrix-service-token-0123456789-abcdef"),
            record: ServiceAccountRecord {
                id: "matrix-service".to_string(),
                name: "service-account".to_string(),
                tenant_id: "default".to_string(),
                project_id: "default".to_string(),
                role: "operator".to_string(),
                scopes: vec!["jobs:read".to_string()],
                expires_at: None,
                revoked_at: None,
                last_used_at: None,
                created_at: "2026-10-11".to_string(),
            },
        });
    store
        .project_memberships
        .lock()
        .expect("memberships")
        .push(ProjectMembershipRecord {
            id: "matrix-admin".to_string(),
            tenant_id: "default".to_string(),
            project_id: "default".to_string(),
            principal_kind: "user".to_string(),
            principal_name: "project-admin".to_string(),
            role: "project_admin".to_string(),
            created_by: "system".to_string(),
            created_at: "2026-10-11".to_string(),
            updated_at: "2026-10-11".to_string(),
        });
    let auth = AuthConfig::from_json(r#"[
        {"name":"tenant-member","role":"viewer","project_id":"unassigned","token":"tenant-member-token-0123456789-abcdef","scopes":["*"]},
        {"name":"viewer","role":"viewer","token":"matrix-viewer-token-0123456789-abcdef","scopes":["*"]},
        {"name":"operator","role":"operator","token":"matrix-operator-token-0123456789-abcdef","scopes":["*"]},
        {"name":"project-admin","role":"viewer","project_id":"bootstrap","token":"matrix-project-admin-token-0123456789-abcdef","scopes":["*"]},
        {"name":"wrong-project","role":"operator","project_id":"finance","token":"matrix-wrong-project-token-0123456789-abcdef","scopes":["*"]},
        {"name":"global-admin","role":"admin","token":"matrix-global-admin-token-0123456789-abcdef"}
    ]"#).expect("matrix auth");
    router(AppState::new(store, FakeObjectStore::default(), ["mini".to_string()]).with_auth(auth))
}

#[tokio::test]
async fn authorization_service_account_does_not_inherit_a_same_named_users_membership() {
    let store = FakeStore::default();
    store
        .project_memberships
        .lock()
        .expect("memberships")
        .push(ProjectMembershipRecord {
            id: "user-membership".to_string(),
            tenant_id: "default".to_string(),
            project_id: "finance".to_string(),
            principal_kind: "user".to_string(),
            principal_name: "service-account".to_string(),
            role: "project_admin".to_string(),
            created_by: "system".to_string(),
            created_at: "2026-10-11".to_string(),
            updated_at: "2026-10-11".to_string(),
        });
    let response = authorization_router_with_store(store)
        .oneshot(
            Request::builder()
                .uri("/v1/jobs/runs")
                .header(
                    "authorization",
                    "Bearer matrix-service-token-0123456789-abcdef",
                )
                .header("x-capsulet-project-id", "finance")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        response.status(),
        403,
        "a user's membership must not expand a same-named service account's projects"
    );
}

#[tokio::test]
async fn authorization_wrong_project_resource_is_not_disclosed() {
    let store = FakeStore::default().with_workflow("private-existing");
    store
        .ownership
        .lock()
        .expect("ownership")
        .push(FakeOwnership {
            resource: "workflows".to_string(),
            id: "private-existing".to_string(),
            tenant_id: "default".to_string(),
            project_id: "finance".to_string(),
        });
    let response = authenticated_app(store)
        .oneshot(
            Request::builder()
                .uri("/v1/workflows/private-existing")
                .header("authorization", "Bearer viewer-token-0123456789-abcdefgh")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), 404);
    assert_eq!(response_json(response).await["code"], "resource_not_found");
}
macro_rules! authorization_operation {
    ($test:ident, $operation:literal) => {
        #[tokio::test]
        async fn $test() {
            check_operation($operation).await;
        }
    };
}
authorization_operation!(authorization_get_health, "getHealth");
authorization_operation!(authorization_get_liveness, "getLiveness");
authorization_operation!(authorization_get_readiness, "getReadiness");
authorization_operation!(authorization_get_metrics, "getMetrics");
authorization_operation!(authorization_get_open_api, "getOpenApi");
authorization_operation!(authorization_ingest_webhook, "ingestWebhook");
authorization_operation!(authorization_get_current_principal, "getCurrentPrincipal");
authorization_operation!(authorization_list_projects, "listProjects");
authorization_operation!(
    authorization_list_project_memberships,
    "listProjectMemberships"
);
authorization_operation!(
    authorization_upsert_project_membership,
    "upsertProjectMembership"
);
authorization_operation!(
    authorization_delete_project_membership,
    "deleteProjectMembership"
);
authorization_operation!(authorization_list_service_accounts, "listServiceAccounts");
authorization_operation!(authorization_create_service_account, "createServiceAccount");
authorization_operation!(authorization_revoke_service_account, "revokeServiceAccount");
authorization_operation!(authorization_list_job_definitions, "listJobDefinitions");
authorization_operation!(authorization_create_job_definition, "createJobDefinition");
authorization_operation!(authorization_get_job_definition, "getJobDefinition");
authorization_operation!(authorization_update_job_definition, "updateJobDefinition");
authorization_operation!(authorization_delete_job_definition, "deleteJobDefinition");
authorization_operation!(
    authorization_get_job_definition_source,
    "getJobDefinitionSource"
);
authorization_operation!(authorization_list_execution_pools, "listExecutionPools");
authorization_operation!(authorization_list_audit_events, "listAuditEvents");
authorization_operation!(authorization_list_host_groups, "listHostGroups");
authorization_operation!(authorization_get_topology, "getTopology");
authorization_operation!(authorization_list_workflows, "listWorkflows");
authorization_operation!(authorization_create_workflow, "createWorkflow");
authorization_operation!(authorization_get_workflow, "getWorkflow");
authorization_operation!(authorization_update_workflow, "updateWorkflow");
authorization_operation!(authorization_delete_workflow, "deleteWorkflow");
authorization_operation!(
    authorization_get_workflow_editability,
    "getWorkflowEditability"
);
authorization_operation!(authorization_list_graphs, "listGraphs");
authorization_operation!(authorization_create_graph, "createGraph");
authorization_operation!(authorization_get_graph, "getGraph");
authorization_operation!(authorization_list_agents, "listAgents");
authorization_operation!(authorization_create_agent, "createAgent");
authorization_operation!(authorization_get_agent, "getAgent");
authorization_operation!(authorization_start_agent_run, "startAgentRun");
authorization_operation!(authorization_list_agent_runs, "listAgentRuns");
authorization_operation!(authorization_get_agent_run, "getAgentRun");
authorization_operation!(authorization_ask_reasoning, "askReasoning");
authorization_operation!(authorization_register_ir_definition, "registerIrDefinition");
authorization_operation!(authorization_list_ir_definitions, "listIrDefinitions");
authorization_operation!(
    authorization_get_ir_definition_version,
    "getIrDefinitionVersion"
);
authorization_operation!(authorization_start_ir_run, "startIrRun");
authorization_operation!(authorization_list_ir_runs, "listIrRuns");
authorization_operation!(authorization_get_ir_run, "getIrRun");
authorization_operation!(authorization_get_ir_run_events, "getIrRunEvents");
authorization_operation!(
    authorization_list_assurance_certificates,
    "listAssuranceCertificates"
);
authorization_operation!(
    authorization_get_assurance_certificate,
    "getAssuranceCertificate"
);
authorization_operation!(
    authorization_get_assurance_certificate_bundle,
    "getAssuranceCertificateBundle"
);
authorization_operation!(authorization_list_certificates, "listCertificates");
authorization_operation!(authorization_list_memory_sources, "listMemorySources");
authorization_operation!(authorization_create_memory_source, "createMemorySource");
authorization_operation!(authorization_get_memory_source, "getMemorySource");
authorization_operation!(authorization_list_memory_evidence, "listMemoryEvidence");
authorization_operation!(authorization_create_memory_evidence, "createMemoryEvidence");
authorization_operation!(authorization_get_memory_evidence, "getMemoryEvidence");
authorization_operation!(authorization_list_memory_entities, "listMemoryEntities");
authorization_operation!(authorization_create_memory_entity, "createMemoryEntity");
authorization_operation!(authorization_get_memory_entity, "getMemoryEntity");
authorization_operation!(authorization_list_memory_claims, "listMemoryClaims");
authorization_operation!(authorization_create_memory_claim, "createMemoryClaim");
authorization_operation!(authorization_get_memory_claim, "getMemoryClaim");
authorization_operation!(authorization_list_memory_events, "listMemoryEvents");
authorization_operation!(authorization_create_memory_event, "createMemoryEvent");
authorization_operation!(authorization_get_memory_event, "getMemoryEvent");
authorization_operation!(
    authorization_list_memory_relationships,
    "listMemoryRelationships"
);
authorization_operation!(
    authorization_create_memory_relationship,
    "createMemoryRelationship"
);
authorization_operation!(
    authorization_get_memory_relationship,
    "getMemoryRelationship"
);
authorization_operation!(authorization_list_memory_contracts, "listMemoryContracts");
authorization_operation!(authorization_create_memory_contract, "createMemoryContract");
authorization_operation!(authorization_get_memory_contract, "getMemoryContract");
authorization_operation!(authorization_list_memory_subgraphs, "listMemorySubgraphs");
authorization_operation!(authorization_create_memory_subgraph, "createMemorySubgraph");
authorization_operation!(authorization_get_memory_subgraph, "getMemorySubgraph");
authorization_operation!(
    authorization_activate_memory_subgraph,
    "activateMemorySubgraph"
);
authorization_operation!(
    authorization_create_memory_subgraph_member,
    "createMemorySubgraphMember"
);
authorization_operation!(
    authorization_list_canonical_entities,
    "listCanonicalEntities"
);
authorization_operation!(
    authorization_create_canonical_entity,
    "createCanonicalEntity"
);
authorization_operation!(
    authorization_list_entity_resolutions,
    "listEntityResolutions"
);
authorization_operation!(
    authorization_create_entity_resolution,
    "createEntityResolution"
);
authorization_operation!(
    authorization_confirm_entity_resolution,
    "confirmEntityResolution"
);
authorization_operation!(
    authorization_reject_entity_resolution,
    "rejectEntityResolution"
);
authorization_operation!(authorization_list_claim_conflicts, "listClaimConflicts");
authorization_operation!(authorization_resolve_claim_conflict, "resolveClaimConflict");
authorization_operation!(authorization_dismiss_claim_conflict, "dismissClaimConflict");
authorization_operation!(authorization_create_summary_trace, "createSummaryTrace");
authorization_operation!(
    authorization_create_entity_graph_attachment,
    "createEntityGraphAttachment"
);
authorization_operation!(authorization_create_subgraph_edge, "createSubgraphEdge");
authorization_operation!(
    authorization_list_ingestion_connectors,
    "listIngestionConnectors"
);
authorization_operation!(
    authorization_create_ingestion_connector,
    "createIngestionConnector"
);
authorization_operation!(
    authorization_get_ingestion_connector,
    "getIngestionConnector"
);
authorization_operation!(
    authorization_run_ingestion_connector,
    "runIngestionConnector"
);
authorization_operation!(authorization_list_ingestion_runs, "listIngestionRuns");
authorization_operation!(authorization_get_ingestion_run, "getIngestionRun");
authorization_operation!(
    authorization_list_ingestion_review_claims,
    "listIngestionReviewClaims"
);
authorization_operation!(
    authorization_approve_ingestion_review_claim,
    "approveIngestionReviewClaim"
);
authorization_operation!(
    authorization_reject_ingestion_review_claim,
    "rejectIngestionReviewClaim"
);
authorization_operation!(authorization_list_automations, "listAutomations");
authorization_operation!(authorization_create_automation, "createAutomation");
authorization_operation!(authorization_get_automation, "getAutomation");
authorization_operation!(authorization_update_automation, "updateAutomation");
authorization_operation!(authorization_delete_automation, "deleteAutomation");
authorization_operation!(authorization_enable_automation, "enableAutomation");
authorization_operation!(authorization_disable_automation, "disableAutomation");
authorization_operation!(
    authorization_list_automation_triggers,
    "listAutomationTriggers"
);
authorization_operation!(authorization_trigger_automation, "triggerAutomation");
authorization_operation!(authorization_list_trigger_plugins, "listTriggerPlugins");
authorization_operation!(authorization_create_trigger_plugin, "createTriggerPlugin");
authorization_operation!(authorization_get_trigger_plugin, "getTriggerPlugin");
authorization_operation!(authorization_list_workflow_runs, "listWorkflowRuns");
authorization_operation!(authorization_stream_activity_events, "streamActivityEvents");
authorization_operation!(authorization_get_workflow_run, "getWorkflowRun");
authorization_operation!(authorization_get_workflow_run_logs, "getWorkflowRunLogs");
authorization_operation!(
    authorization_stream_workflow_run_logs,
    "streamWorkflowRunLogs"
);
authorization_operation!(authorization_remove_workflow_run, "removeWorkflowRun");
authorization_operation!(authorization_cancel_workflow_run, "cancelWorkflowRun");
authorization_operation!(authorization_resume_workflow_run, "resumeWorkflowRun");
authorization_operation!(authorization_list_job_runs, "listJobRuns");
authorization_operation!(authorization_create_job_run, "createJobRun");
authorization_operation!(authorization_get_job_run, "getJobRun");
authorization_operation!(authorization_cancel_job_run, "cancelJobRun");
authorization_operation!(authorization_get_job_run_logs, "getJobRunLogs");
authorization_operation!(authorization_stream_job_run_logs, "streamJobRunLogs");
authorization_operation!(authorization_list_job_artifacts, "listJobArtifacts");
authorization_operation!(authorization_download_job_artifact, "downloadJobArtifact");

#[derive(serde::Deserialize)]
struct AuthorizationCase {
    operation_id: String,
    minimum_role: String,
    ownership: String,
}

async fn check_operation(operation_id: &str) {
    let cases: Vec<AuthorizationCase> = serde_json::from_str(include_str!(
        "../../tests/fixtures/authorization-cases.json"
    ))
    .expect("independently reviewed role matrix");
    let case = cases
        .iter()
        .find(|case| case.operation_id == operation_id)
        .expect("operation case");
    let endpoint = crate::find_operation(operation_id).expect("registered operation");
    let app = authorization_router();
    let required = match case.minimum_role.as_str() {
        "anonymous" => 0,
        "authenticated" => 1,
        "viewer" => 2,
        "operator" => 3,
        "admin" => 4,
        other => panic!("unknown expected role: {other}"),
    };
    let path = endpoint
        .path
        .split('/')
        .map(|segment| match segment {
            "{project_id}" => "default",
            "{digest}" => "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            value if value.starts_with('{') => "missing",
            value => value,
        })
        .collect::<Vec<_>>()
        .join("/");
    for (actor, token, level) in ACTORS {
        let mut request = Request::builder()
            .method(endpoint.method.as_str())
            .uri(&path)
            .header("x-capsulet-project-id", "default")
            .header("content-type", "application/json");
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        // Deliberately invalid JSON makes allowed writes reach an extractor without
        // creating data. A denied request must fail before any body extraction.
        let body = if endpoint.method == crate::HttpMethod::Get {
            Body::empty()
        } else {
            Body::from("{")
        };
        let response = app
            .clone()
            .oneshot(request.body(body).expect("request"))
            .await
            .expect("response");
        if operation_id == "ingestWebhook" {
            assert_eq!(
                response.status(),
                401,
                "webhooks require their signature for every actor"
            );
            continue;
        }
        let service_allowed = actor != "service account"
            || required == 0
            || matches!(
                operation_id,
                "listJobRuns"
                    | "getJobRun"
                    | "getJobRunLogs"
                    | "streamJobRunLogs"
                    | "listJobArtifacts"
                    | "downloadJobArtifact"
            );
        let allowed = level >= required && service_allowed;
        if required > 0 && level == 0 {
            assert_eq!(response.status(), 401, "{operation_id}: {actor}");
        } else if !allowed {
            assert_eq!(response.status(), 403, "{operation_id}: {actor}");
        } else {
            assert!(
                !matches!(response.status().as_u16(), 401 | 403 | 429 | 500),
                "{operation_id}: {actor} unexpectedly denied with {}",
                response.status()
            );
        }
    }
    assert!(matches!(
        case.ownership.as_str(),
        "unscoped" | "selected_project" | "path_project"
    ));
}

// This loop guarantees new fixture entries execute even before their individually
// named test is added. The external matrix test requires exact route coverage.
#[tokio::test]
async fn authorization_executes_every_reviewed_public_operation() {
    let cases: Vec<AuthorizationCase> = serde_json::from_str(include_str!(
        "../../tests/fixtures/authorization-cases.json"
    ))
    .expect("cases");
    for case in cases {
        check_operation(&case.operation_id).await;
    }
}

#[tokio::test]
async fn authorization_existing_resource_without_ownership_is_not_disclosed() {
    let store = FakeStore {
        omit_ownership: true,
        ..FakeStore::default().with_workflow("unowned")
    };
    let response = authenticated_app(store)
        .oneshot(
            Request::builder()
                .uri("/v1/workflows/unowned")
                .header("authorization", "Bearer viewer-token-0123456789-abcdefgh")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn authorization_wildcard_scope_cannot_elevate_a_project_viewer() {
    let auth = AuthConfig::from_json(
        r#"[{"name":"reader","role":"viewer","token":"wildcard-viewer-0123456789-abcdefgh","scopes":["*"]}]"#,
    )
    .expect("auth config");
    let app = router(
        AppState::new(
            FakeStore::default(),
            FakeObjectStore::default(),
            ["mini".to_string()],
        )
        .with_auth(auth),
    );
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/graphs")
                .header(
                    "authorization",
                    "Bearer wildcard-viewer-0123456789-abcdefgh",
                )
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), 403);
}
