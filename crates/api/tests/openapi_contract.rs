use std::collections::BTreeSet;

use capsulet_api::{
    AuthenticationMode, CreateServiceAccountResponse, ErrorContract, HttpMethod, MediaType,
    ProjectContextRule, RequiredPermission, ResponseStatus, SchemaId, Stability,
    canonical_openapi_json, endpoint_contracts, generated_openapi, validated_openapi,
};

fn documented_operations(document: &serde_json::Value) -> BTreeSet<(String, String)> {
    let mut operations = BTreeSet::new();
    for (path, item) in document["paths"].as_object().expect("paths object") {
        for method in ["get", "post", "put", "delete", "patch"] {
            if item.get(method).is_some() {
                operations.insert((method.to_ascii_uppercase(), path.clone()));
            }
        }
    }
    operations
}

#[test]
fn generated_openapi_should_match_every_runtime_endpoint() {
    let document = generated_openapi();
    let runtime = endpoint_contracts()
        .iter()
        .map(|endpoint| (endpoint.method.to_string(), endpoint.path.to_string()))
        .collect::<BTreeSet<_>>();

    assert_eq!(documented_operations(&document), runtime);
    assert_eq!(
        runtime
            .iter()
            .map(|(_, path)| path)
            .collect::<BTreeSet<_>>()
            .len(),
        95
    );
}

#[test]
fn every_operation_should_have_stable_metadata_and_complete_shapes() {
    let document = generated_openapi();
    let mut operation_ids = BTreeSet::new();

    for endpoint in endpoint_contracts() {
        let operation = &document["paths"][endpoint.path][endpoint.method.as_lowercase()];
        let operation_id = operation["operationId"].as_str().expect("operationId");
        assert!(!operation_id.is_empty());
        assert!(operation_ids.insert(operation_id));
        assert_eq!(
            operation["x-capsulet-stability"],
            endpoint.stability.as_str()
        );
        assert_eq!(
            operation["x-capsulet-required-scope"],
            endpoint.required_permission.as_str()
        );
        assert_eq!(
            operation["x-capsulet-project-context"],
            endpoint.project_context.is_required()
        );

        for parameter in endpoint.path_parameters() {
            assert!(
                operation["parameters"]
                    .as_array()
                    .expect("parameters")
                    .iter()
                    .any(|item| {
                        item["name"] == parameter
                            && item["in"] == "path"
                            && item["required"] == true
                    })
            );
        }
        if endpoint.request_schema.is_some() {
            assert!(
                operation["requestBody"]["content"]["application/json"]["schema"]["$ref"]
                    .is_string()
            );
        }
        if endpoint.success_status == ResponseStatus::NoContent {
            assert!(
                operation["responses"][endpoint.success_status.as_str()]
                    .get("content")
                    .is_none()
            );
        } else {
            assert!(
                operation["responses"][endpoint.success_status.as_str()]["content"]
                    [endpoint.response_content_type.as_str()]["schema"]
                    .is_object()
            );
        }
        assert!(
            operation["responses"]["default"]["content"]["application/json"]["schema"].is_object()
        );

        if endpoint.authentication.requires_authentication() {
            assert_eq!(
                operation["security"][0]["bearerAuth"],
                serde_json::json!([])
            );
        } else {
            assert_eq!(operation["security"], serde_json::json!([]));
        }
    }
}

#[test]
fn checked_openapi_artifact_should_equal_canonical_generation() {
    assert_eq!(
        include_str!("../openapi.json"),
        canonical_openapi_json().expect("OpenAPI should serialize")
    );
}

#[test]
fn generated_document_should_round_trip_through_utoipa() {
    let document = generated_openapi();
    for (name, schema) in document["components"]["schemas"]
        .as_object()
        .expect("component schemas")
    {
        serde_json::from_value::<utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>>(
            schema.clone(),
        )
        .unwrap_or_else(|error| panic!("component {name} should be valid for utoipa: {error}"));
    }
    validated_openapi().expect("generated OpenAPI should be valid for utoipa");
}

#[test]
fn control_plane_schemas_should_describe_real_wire_fields() {
    let document = generated_openapi();
    let schemas = document["components"]["schemas"]
        .as_object()
        .expect("component schemas");
    for name in [
        "PrincipalResponse",
        "ProjectMembershipResponse",
        "ServiceAccountResponse",
        "JobDefinitionResponse",
        "JobRunResponse",
        "WorkflowResponse",
        "WorkflowRunResponse",
        "AutomationResponse",
        "TriggerPluginResponse",
        "ExecutionPoolResponse",
        "TopologyResponse",
        "JobRunLogsResponse",
        "ArtifactResponse",
        "WebhookResponse",
    ] {
        assert_ne!(schemas[name]["additionalProperties"], true, "{name}");
        assert!(schemas[name]["properties"].is_object(), "{name}");
        assert!(schemas[name]["required"].is_array(), "{name}");
    }
    assert!(
        schemas["CreateRunRequest"]["required"]
            .as_array()
            .expect("required")
            .contains(&serde_json::json!("job_definition_id"))
    );
    assert_eq!(
        schemas["JobRunResponse"]["properties"]["status"]["type"], "string",
        "the real wire field is currently an unconstrained String"
    );
    assert_eq!(
        schemas["WorkflowRunResponse"]["properties"]["automation_id"]["type"],
        serde_json::json!(["string", "null"])
    );
}

#[test]
fn control_plane_operations_should_document_observable_parameters() {
    let document = generated_openapi();
    let job_runs = &document["paths"]["/v1/jobs/runs"]["get"]["parameters"];
    for name in [
        "limit",
        "start_at",
        "end_at",
        "q",
        "state",
        "sort",
        "direction",
    ] {
        assert!(
            job_runs
                .as_array()
                .expect("parameters")
                .iter()
                .any(|parameter| { parameter["name"] == name && parameter["in"] == "query" })
        );
    }
    let webhook = &document["paths"]["/v1/webhooks/{automation_id}/{trigger_name}"]["post"];
    for name in [
        "x-capsulet-timestamp",
        "x-capsulet-signature",
        "x-capsulet-delivery",
        "x-capsulet-correlation",
    ] {
        assert!(
            webhook["parameters"]
                .as_array()
                .expect("parameters")
                .iter()
                .any(|parameter| parameter["name"] == name && parameter["in"] == "header")
        );
    }
    assert_eq!(
        document["components"]["schemas"]["EventStreamResponse"]["format"],
        "event-stream"
    );
}

#[test]
fn every_control_plane_operation_should_use_a_concrete_component() {
    let document = generated_openapi();
    let schemas = &document["components"]["schemas"];
    for endpoint in endpoint_contracts().iter().filter(|endpoint| {
        !endpoint.path.starts_with("/v1/graphs")
            && !endpoint.path.starts_with("/v1/agents")
            && !endpoint.path.starts_with("/v1/agent-runs")
            && !endpoint.path.starts_with("/v1/reasoning")
            && !endpoint.path.starts_with("/v1/memory")
            && !endpoint.path.starts_with("/v1/ingestion")
    }) {
        for name in endpoint
            .request_schema
            .into_iter()
            .chain(std::iter::once(endpoint.response_schema))
        {
            let name = name.as_str();
            let schema = &schemas[name];
            assert!(!schema.is_null(), "missing component {name}");
            if !matches!(
                name,
                "WebhookRequest"
                    | "OpenApiDocument"
                    | "StringResponse"
                    | "EventStreamResponse"
                    | "BinaryResponse"
                    | "EmptyResponse"
            ) {
                assert_ne!(
                    schema["additionalProperties"], true,
                    "generic component {name}"
                );
            }
        }
    }
}

#[test]
fn agent_correctness_memory_and_ingestion_schemas_should_be_concrete() {
    let document = generated_openapi();
    let schemas = &document["components"]["schemas"];
    for endpoint in endpoint_contracts().iter().filter(|endpoint| {
        endpoint.path.starts_with("/v1/graphs")
            || endpoint.path.starts_with("/v1/agents")
            || endpoint.path.starts_with("/v1/agent-runs")
            || endpoint.path.starts_with("/v1/reasoning")
            || endpoint.path.starts_with("/v1/memory")
            || endpoint.path.starts_with("/v1/ingestion")
    }) {
        assert_eq!(endpoint.stability, Stability::Experimental);
        for name in endpoint
            .request_schema
            .into_iter()
            .chain(std::iter::once(endpoint.response_schema))
        {
            let name = name.as_str();
            let schema = &schemas[name];
            assert!(!schema.is_null(), "missing component {name}");
            assert_ne!(
                schema["additionalProperties"], true,
                "generic component {name}"
            );
            assert!(schema["properties"].is_object(), "{name}");
        }
    }
}

#[test]
fn certificate_schema_should_expose_only_current_kernel_assurance() {
    let document = generated_openapi();
    let schemas = &document["components"]["schemas"];
    assert_eq!(
        schemas["KernelVerdict"]["enum"],
        serde_json::json!(["accepted", "conditional", "rejected"])
    );
    assert!(
        !schemas["KernelVerdict"]["enum"]
            .as_array()
            .expect("verdicts")
            .contains(&serde_json::json!("unverified"))
    );
    assert_eq!(
        schemas["Certificate"]["examples"].as_array().map(Vec::len),
        Some(3)
    );
    for field in [
        "verdict",
        "goal",
        "discharged",
        "residuals",
        "errors",
        "replay_digest",
    ] {
        assert!(
            schemas["Certificate"]["required"]
                .as_array()
                .expect("required fields")
                .contains(&serde_json::json!(field))
        );
    }
}

#[test]
fn evidence_and_review_filters_should_match_current_handlers() {
    let document = generated_openapi();
    let evidence = &document["components"]["schemas"]["MemoryEvidenceResponse"]["properties"];
    for field in ["source_id", "locator", "excerpt", "observed_at"] {
        assert!(
            evidence.get(field).is_some(),
            "missing evidence field {field}"
        );
    }
    for absent in ["start_byte", "end_byte", "source_hash"] {
        assert!(
            evidence.get(absent).is_none(),
            "invented evidence field {absent}"
        );
    }
    for path in [
        "/v1/memory/entity-resolutions",
        "/v1/memory/conflicts",
        "/v1/ingestion/review/claims",
    ] {
        assert!(
            document["paths"][path]["get"]["parameters"]
                .as_array()
                .expect("parameters")
                .iter()
                .any(|parameter| parameter["name"] == "status" && parameter["in"] == "query")
        );
    }
}

#[test]
fn create_service_account_response_should_match_its_generated_wire_schema() {
    let payload = serde_json::to_value(CreateServiceAccountResponse {
        id: "service_account_contract".to_string(),
        name: "Contract test".to_string(),
        tenant_id: "tenant_contract".to_string(),
        project_id: "project_contract".to_string(),
        role: "project_operator".to_string(),
        scopes: vec!["jobs:write".to_string()],
        expires_at: None,
        revoked_at: None,
        last_used_at: None,
        created_at: "2026-08-31T00:00:00Z".to_string(),
        token: "secret-test-token".to_string(),
    })
    .expect("service-account response serializes");
    let document = generated_openapi();
    let schema = &document["components"]["schemas"]["CreateServiceAccountResponse"];
    let properties = schema["properties"].as_object().expect("schema properties");
    let required = schema["required"].as_array().expect("required fields");

    for field in payload.as_object().expect("response object").keys() {
        assert!(
            properties.contains_key(field),
            "schema rejects emitted field {field}"
        );
        assert!(
            required.contains(&serde_json::json!(field)),
            "serialized field {field} is not required by the schema"
        );
    }
    for nullable in ["expires_at", "revoked_at", "last_used_at"] {
        assert_eq!(
            properties[nullable]["type"],
            serde_json::json!(["string", "null"]),
            "{nullable} must accept the emitted null value"
        );
    }
}

#[test]
fn nested_wire_models_should_have_named_component_schemas() {
    let document = generated_openapi();
    let schemas = &document["components"]["schemas"];
    let expected_references = [
        (
            "CreateWorkflowRequest",
            "steps",
            "CreateWorkflowStepRequest",
        ),
        (
            "CreateWorkflowRequest",
            "dependencies",
            "CreateWorkflowDependencyRequest",
        ),
        (
            "CreateAutomationRequest",
            "triggers",
            "CreateAutomationTriggerRequest",
        ),
        (
            "CompiledMemoryPolicyResponse",
            "relations",
            "RelationPolicyResponse",
        ),
        (
            "CompiledMemoryPolicyResponse",
            "retrieval_policies",
            "RetrievalPolicyResponse",
        ),
    ];

    for (owner, field, nested) in expected_references {
        assert!(
            schemas[nested]["properties"].is_object(),
            "missing {nested}"
        );
        assert_eq!(
            schemas[owner]["properties"][field]["items"]["$ref"],
            format!("#/components/schemas/{nested}"),
            "{owner}.{field} must reference its real nested wire model"
        );
    }
    for (owner, field, nested) in [
        (
            "MemoryContractResponse",
            "compiled",
            "CompiledMemoryPolicyResponse",
        ),
        (
            "CompiledMemoryPolicyResponse",
            "claim_policy",
            "ClaimPolicyResponse",
        ),
        (
            "CreateIngestionConnectorRequest",
            "config",
            "LocalTextConnectorConfigRequest",
        ),
        (
            "IngestionConnectorResponse",
            "config",
            "LocalTextConnectorConfigResponse",
        ),
    ] {
        assert!(
            schemas[nested]["properties"].is_object(),
            "missing {nested}"
        );
        assert_eq!(
            schemas[owner]["properties"][field]["$ref"],
            format!("#/components/schemas/{nested}"),
            "{owner}.{field} must reference its real nested wire model"
        );
    }
}

#[test]
fn endpoint_contract_metadata_should_be_typed() {
    for endpoint in endpoint_contracts() {
        let _: HttpMethod = endpoint.method;
        let _: Stability = endpoint.stability;
        let _: RequiredPermission = endpoint.required_permission;
        let _: ProjectContextRule = endpoint.project_context;
        let _: AuthenticationMode = endpoint.authentication;
        let _: ResponseStatus = endpoint.success_status;
        let _: MediaType = endpoint.response_content_type;
        let _: Option<SchemaId> = endpoint.request_schema;
        let _: SchemaId = endpoint.response_schema;
        let _: ErrorContract = endpoint.error_contract;
    }
}
