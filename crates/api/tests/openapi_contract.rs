use std::collections::BTreeSet;

use capsulet_api::{
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

fn declared_router_operations() -> BTreeSet<(String, String)> {
    let source = include_str!("../src/http/internal.rs");
    let mut operations = BTreeSet::new();
    let mut remainder = source;

    while let Some(route_start) = remainder.find(".route(") {
        remainder = &remainder[route_start + ".route(".len()..];
        let mut depth = 1_i32;
        let mut in_string = false;
        let mut escaped = false;
        let mut end = 0;
        for (index, character) in remainder.char_indices() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == '"' {
                    in_string = false;
                }
                continue;
            }
            match character {
                '"' => in_string = true,
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = index;
                        break;
                    }
                }
                _ => {}
            }
        }
        let declaration = &remainder[..end];
        let first_quote = declaration.find('"').expect("route path opening quote") + 1;
        let last_quote = declaration[first_quote..]
            .find('"')
            .expect("route path closing quote")
            + first_quote;
        let path = &declaration[first_quote..last_quote];
        for (token, method) in [
            ("get(", "GET"),
            ("post(", "POST"),
            ("put(", "PUT"),
            ("delete(", "DELETE"),
            ("patch(", "PATCH"),
        ] {
            if declaration.contains(token) {
                operations.insert((method.to_owned(), path.to_owned()));
            }
        }
        remainder = &remainder[end + 1..];
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
    assert_eq!(declared_router_operations(), runtime);
    assert_eq!(
        runtime
            .iter()
            .map(|(_, path)| path)
            .collect::<BTreeSet<_>>()
            .len(),
        90
    );
}

#[test]
fn every_operation_should_have_stable_metadata_and_complete_shapes() {
    let document = generated_openapi();
    let mut operation_ids = BTreeSet::new();

    for endpoint in endpoint_contracts() {
        let operation = &document["paths"][endpoint.path][endpoint.method.to_ascii_lowercase()];
        let operation_id = operation["operationId"].as_str().expect("operationId");
        assert!(!operation_id.is_empty());
        assert!(operation_ids.insert(operation_id));
        assert_eq!(operation["x-capsulet-stability"], endpoint.stability);
        assert_eq!(
            operation["x-capsulet-required-scope"],
            endpoint.required_scope
        );
        assert_eq!(
            operation["x-capsulet-project-context"],
            endpoint.project_context
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
        if endpoint.success_status == "204" {
            assert!(
                operation["responses"][endpoint.success_status]
                    .get("content")
                    .is_none()
            );
        } else {
            assert!(
                operation["responses"][endpoint.success_status]["content"]
                    [endpoint.response_content_type]["schema"]
                    .is_object()
            );
        }
        assert!(
            operation["responses"]["default"]["content"]["application/json"]["schema"].is_object()
        );

        if endpoint.authenticated {
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
        schemas["JobRunResponse"]["properties"]["status"]["$ref"],
        "#/components/schemas/JobRunStatus"
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
