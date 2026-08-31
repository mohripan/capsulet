//! Deterministic `OpenAPI` generation from the runtime endpoint contract registry.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};
use utoipa::openapi::OpenApi;

use crate::endpoint_contract::endpoint_contracts;

const OPENAPI_VERSION: &str = "3.1.0";
const BASE_SCHEMA_NAMES: &[&str] = &[
    "Error",
    "JobRunStatus",
    "WorkflowStatus",
    "WorkflowRunStatus",
    "AutomationStatus",
    "ProjectResponse",
    "ProjectMembershipResponse",
    "ServiceAccountResponse",
    "JobDefinitionResponse",
    "JobRunResponse",
    "WorkflowResponse",
    "WorkflowRunResponse",
    "AutomationResponse",
    "TriggerResponse",
    "TriggerPluginResponse",
    "ExecutionPoolResponse",
    "HostGroupResponse",
    "AuditEventResponse",
    "TopologyNodeResponse",
    "TopologyEdgeResponse",
    "WorkflowStepResponse",
    "WorkflowDependencyResponse",
    "WorkflowStepRunResponse",
    "WorkflowRunLogEntryResponse",
    "ArtifactResponse",
    "GraphNodeRequest",
    "GraphPortRequest",
    "GraphHyperedgeRequest",
    "HyperedgeEndpointRequest",
    "GraphTransitionPolicyRequest",
    "GraphNodeResponse",
    "GraphPortResponse",
    "GraphHyperedgeResponse",
    "HyperedgeEndpointResponse",
    "GraphTransitionPolicyResponse",
    "AgentBudgetRequest",
    "AgentBudgetResponse",
    "KernelVerdict",
    "Certificate",
    "Proposition",
    "DischargedStep",
    "Residual",
    "CertificateError",
    "RawProposal",
    "CertificateSummary",
    "IngestionRunOutputsResponse",
    "ReviewClaimResponse",
    "ReviewEvidenceResponse",
    "ReviewSourceResponse",
    "CreateWorkflowStepRequest",
    "CreateWorkflowDependencyRequest",
    "CreateAutomationTriggerRequest",
    "CompiledMemoryPolicyResponse",
    "RelationPolicyResponse",
    "ClaimPolicyResponse",
    "RetrievalPolicyResponse",
    "LocalTextConnectorConfigRequest",
    "LocalTextConnectorConfigResponse",
];

/// Builds the public API contract from the same endpoint metadata used at runtime.
#[must_use]
/// # Panics
///
/// Panics only if an internally-created path entry is not a JSON object or a
/// derived Rust schema cannot be serialized. Both conditions are programmer errors.
pub fn generated_openapi() -> Value {
    let mut paths = Map::new();
    let mut schema_names = BASE_SCHEMA_NAMES.iter().copied().collect::<BTreeSet<_>>();

    for endpoint in endpoint_contracts() {
        if let Some(request_schema) = endpoint.request_schema {
            schema_names.insert(request_schema.as_str());
        }
        schema_names.insert(endpoint.response_schema.as_str());

        let item = paths
            .entry(endpoint.path.to_owned())
            .or_insert_with(|| Value::Object(Map::new()));
        let methods = item
            .as_object_mut()
            .expect("path items are created as JSON objects");
        methods.insert(
            endpoint.method.as_lowercase().to_owned(),
            operation_document(endpoint),
        );
    }

    let schemas = schema_names
        .into_iter()
        .map(|name| (name.to_owned(), component_schema(name)))
        .collect::<Map<_, _>>();

    json!({
        "openapi": OPENAPI_VERSION,
        "info": {
            "title": "Capsulet API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Generated correctness-first Capsulet HTTP contract. Operations are experimental unless promoted by the stability policy."
        },
        "servers": [{"url": "/"}],
        "paths": paths,
        "components": {
            "securitySchemes": {
                "bearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "JWT"
                }
            },
            "schemas": schemas
        },
        "x-capsulet-claims": ["CAP-OPENAPI-001"]
    })
}

/// Parses the generated JSON through utoipa's `OpenAPI` model.
///
/// # Errors
///
/// Returns an error when generated JSON is not representable by `utoipa`.
pub fn validated_openapi() -> Result<OpenApi, serde_json::Error> {
    serde_json::from_value(generated_openapi())
}

/// Returns the canonical, deterministically formatted `OpenAPI` document.
///
/// # Errors
///
/// Returns an error when the generated document cannot be serialized.
pub fn canonical_openapi_json() -> Result<String, serde_json::Error> {
    validated_openapi()?;
    let mut output = serde_json::to_string_pretty(&generated_openapi())?;
    output.push('\n');
    Ok(output)
}

fn operation_document(endpoint: &crate::EndpointContract) -> Value {
    let parameters = operation_parameters(endpoint);
    let request_body = endpoint.request_schema.map(|name| {
        json!({
            "required": true,
            "content": {
                "application/json": {
                    "schema": schema_reference(name.as_str())
                }
            }
        })
    });
    let security = if endpoint.authentication.requires_authentication() {
        json!([{"bearerAuth": []}])
    } else {
        json!([])
    };

    let mut operation = json!({
        "operationId": endpoint.operation_id,
        "summary": humanize(endpoint.operation_id),
        "parameters": parameters,
        "responses": {
            endpoint.success_status.as_str(): {
                "description": "Successful response",
                "content": {
                    endpoint.response_content_type.as_str(): {
                        "schema": schema_reference(endpoint.response_schema.as_str())
                    }
                }
            },
            "default": {
                "description": "Error response",
                "content": {
                    "application/json": {
                        "schema": schema_reference("Error")
                    }
                }
            }
        },
        "security": security,
        "x-capsulet-stability": endpoint.stability.as_str(),
        "x-capsulet-required-scope": endpoint.required_permission.as_str(),
        "x-capsulet-project-context": endpoint.project_context.is_required()
    });
    if let Some(body) = request_body {
        operation["requestBody"] = body;
    }
    if endpoint.operation_id == "startAgentRun" {
        operation["description"] = json!(
            "Persists an experimental agent run in queued state. This endpoint does not claim that a production graph worker will execute it."
        );
    }
    if endpoint.success_status == crate::ResponseStatus::NoContent {
        operation["responses"]["204"] = json!({"description": "No content"});
    }
    if endpoint.operation_id == "ingestWebhook" {
        operation["responses"]["200"] = json!({
            "description": "Delivery was already accepted",
            "content": {"application/json": {"schema": schema_reference("WebhookResponse")}}
        });
    }
    operation
}

fn operation_parameters(endpoint: &crate::EndpointContract) -> Vec<Value> {
    let mut parameters = endpoint
        .path_parameters()
        .into_iter()
        .map(|name| {
            json!({
                "name": name,
                "in": "path",
                "required": true,
                "schema": {"type": "string"}
            })
        })
        .chain(endpoint.project_context.is_required().then(|| {
            json!({
                "name": "x-capsulet-project-id",
                "in": "header",
                "required": false,
                "description": "Project context. Required when the principal can access more than one project.",
                "schema": {"type": "string"}
            })
        }))
        .collect::<Vec<_>>();
    parameters.push(json!({
        "name": "x-request-id",
        "in": "header",
        "required": false,
        "description": "Optional caller-supplied request correlation identifier.",
        "schema": {"type": "string"}
    }));
    if endpoint.method == crate::HttpMethod::Get
        && matches!(endpoint.path, "/v1/jobs/runs" | "/v1/workflow-runs")
    {
        for name in [
            "limit",
            "start_at",
            "end_at",
            "q",
            "state",
            "sort",
            "direction",
        ] {
            parameters.push(query_parameter(name));
        }
    } else if endpoint.method == crate::HttpMethod::Get && endpoint.path == "/v1/job-definitions" {
        parameters.push(query_parameter("limit"));
    }
    if endpoint.method == crate::HttpMethod::Get
        && matches!(
            endpoint.path,
            "/v1/memory/entity-resolutions"
                | "/v1/memory/conflicts"
                | "/v1/ingestion/review/claims"
        )
    {
        parameters.push(query_parameter("status"));
    }
    if endpoint.path == "/v1/webhooks/{automation_id}/{trigger_name}" {
        for (name, required) in [
            ("x-capsulet-timestamp", true),
            ("x-capsulet-delivery", true),
            ("x-capsulet-correlation", false),
            ("x-capsulet-signature", true),
        ] {
            parameters.push(json!({
                "name": name,
                "in": "header",
                "required": required,
                "schema": {"type": "string"}
            }));
        }
    }

    parameters
}

fn schema_reference(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
}

fn query_parameter(name: &str) -> Value {
    let schema = if name == "limit" {
        json!({"type": "integer", "minimum": 1, "maximum": 1000})
    } else {
        json!({"type": "string"})
    };
    json!({"name": name, "in": "query", "required": false, "schema": schema})
}

fn component_schema(name: &str) -> Value {
    if let Some(schema) = crate::wire_schemas::schema(name) {
        return schema;
    }
    match name {
        "Error" => json!({
            "type": "object",
            "required": ["code", "message"],
            "properties": {
                "code": {"type": "string", "example": "not_found"},
                "message": {"type": "string"},
                "details": {"type": ["object", "null"], "additionalProperties": true}
            }
        }),
        "HealthResponse" => json!({
            "type": "object",
            "required": ["status"],
            "properties": {"status": {"type": "string", "enum": ["ok", "ready", "not_ready"]}}
        }),
        "EmptyResponse" => json!({"type": "object", "maxProperties": 0}),
        "StringResponse" => json!({"type": "string"}),
        "EventStreamResponse" => json!({
            "type": "string",
            "format": "event-stream",
            "description": "UTF-8 Server-Sent Events stream. Each data field contains a JSON activity or log event."
        }),
        "BinaryResponse" => json!({"type": "string", "format": "binary"}),
        "OpenApiDocument" => json!({"type": "object", "additionalProperties": true}),
        "JobRunStatus" => enum_schema(&[
            "queued",
            "leased",
            "running",
            "succeeded",
            "failed",
            "cancelled",
            "timed_out",
            "retry_scheduled",
        ]),
        "WorkflowStatus" => enum_schema(&["draft", "enabled", "disabled"]),
        "WorkflowRunStatus" => enum_schema(&[
            "queued",
            "running",
            "removed",
            "succeeded",
            "failed",
            "cancelled",
            "timed_out",
            "skipped",
        ]),
        "AutomationStatus" => enum_schema(&["enabled", "disabled"]),
        other if list_component(other).is_some() => {
            let (property, item) = list_component(other).expect("matched list component");
            list_schema(property, item)
        }
        other if special_schema(other).is_some() => {
            special_schema(other).expect("matched special component")
        }
        other => json!({
            "type": "object",
            "description": format!("{other} payload."),
            "additionalProperties": true
        }),
    }
}

fn list_component(name: &str) -> Option<(&'static str, &'static str)> {
    Some(match name {
        "ListProjectsResponse" => ("projects", "ProjectResponse"),
        "ListProjectMembershipsResponse" => ("memberships", "ProjectMembershipResponse"),
        "ListServiceAccountsResponse" => ("service_accounts", "ServiceAccountResponse"),
        "ListJobDefinitionsResponse" => ("job_definitions", "JobDefinitionResponse"),
        "ListRunsResponse" => ("runs", "JobRunResponse"),
        "ListExecutionPoolsResponse" => ("execution_pools", "ExecutionPoolResponse"),
        "ListHostGroupsResponse" => ("host_groups", "HostGroupResponse"),
        "ListAuditEventsResponse" => ("audit_events", "AuditEventResponse"),
        "ListWorkflowsResponse" => ("workflows", "WorkflowResponse"),
        "ListWorkflowRunsResponse" => ("workflow_runs", "WorkflowRunResponse"),
        "ListAutomationsResponse" => ("automations", "AutomationResponse"),
        "ListTriggerPluginsResponse" => ("trigger_plugins", "TriggerPluginResponse"),
        "ListArtifactsResponse" => ("artifacts", "ArtifactResponse"),
        "ListGraphsResponse" => ("graphs", "GraphResponse"),
        "ListAgentsResponse" => ("agents", "AgentResponse"),
        "ListAgentRunsResponse" => ("agent_runs", "AgentRunResponse"),
        "ListCertificatesResponse" => ("certificates", "CertificateSummary"),
        "ListMemorySourcesResponse" => ("sources", "MemorySourceResponse"),
        "ListMemoryEvidenceResponse" => ("evidence", "MemoryEvidenceResponse"),
        "ListMemoryEntitiesResponse" => ("entities", "MemoryEntityResponse"),
        "ListMemoryClaimsResponse" => ("claims", "MemoryClaimResponse"),
        "ListMemoryEventsResponse" => ("events", "MemoryEventResponse"),
        "ListMemoryRelationshipsResponse" => ("relationships", "MemoryRelationshipResponse"),
        "ListMemoryContractsResponse" => ("contracts", "MemoryContractResponse"),
        "ListMemorySubgraphsResponse" => ("subgraphs", "MemorySubgraphResponse"),
        "ListCanonicalEntitiesResponse" => ("canonical_entities", "CanonicalEntityResponse"),
        "ListEntityResolutionsResponse" => ("entity_resolutions", "EntityResolutionResponse"),
        "ListClaimConflictsResponse" => ("conflicts", "ClaimConflictResponse"),
        "ListIngestionConnectorsResponse" => ("connectors", "IngestionConnectorResponse"),
        "ListIngestionRunsResponse" => ("runs", "IngestionRunResponse"),
        "ListIngestionReviewClaimsResponse" => ("claims", "ReviewClaimResponse"),
        _ => return None,
    })
}

fn list_schema(property: &str, item: &str) -> Value {
    object_schema(
        &[property],
        &[(
            property,
            json!({"type": "array", "items": schema_reference(item)}),
        )],
    )
}

fn special_schema(name: &str) -> Option<Value> {
    special_http_schema(name).or_else(|| special_certificate_schema(name))
}

fn special_http_schema(name: &str) -> Option<Value> {
    Some(match name {
        "PrincipalResponse" => object_schema(
            &[
                "name",
                "role",
                "platform_admin",
                "tenant_id",
                "project_id",
                "project_memberships",
                "scopes",
            ],
            &[
                ("name", string_schema()),
                ("role", string_schema()),
                ("platform_admin", bool_schema()),
                ("tenant_id", string_schema()),
                ("project_id", string_schema()),
                (
                    "project_memberships",
                    json!({"type": "array", "items": {
                        "type": "object",
                        "required": ["tenant_id", "project_id", "role"],
                        "properties": {
                            "tenant_id": {"type": "string"},
                            "project_id": {"type": "string"},
                            "role": {"type": "string"}
                        },
                        "additionalProperties": false
                    }}),
                ),
                ("scopes", string_array_schema()),
            ],
        ),
        "WebhookRequest" => json!({"type": "object", "additionalProperties": true}),
        "WebhookResponse" => object_schema(
            &["accepted", "event_id"],
            &[("accepted", bool_schema()), ("event_id", string_schema())],
        ),
        _ => return None,
    })
}

fn special_certificate_schema(name: &str) -> Option<Value> {
    Some(match name {
        "KernelVerdict" => enum_schema(&["accepted", "conditional", "rejected"]),
        "Proposition" => fields(&["subject", "predicate", "object"]),
        "DischargedStep" => fields(&["rule", "concluded", "detail"]),
        "Residual" => all_required_schema(&[
            ("from", string_schema()),
            ("to", schema_reference("Proposition")),
            ("rationale", string_schema()),
            ("evidence_ids", string_array_schema()),
        ]),
        "CertificateError" => object_schema(
            &["code", "message", "repair_owner"],
            &[
                ("code", string_schema()),
                ("message", string_schema()),
                ("repair_owner", string_schema()),
                ("corrected_value", json!({"type": ["number", "null"]})),
            ],
        ),
        "Certificate" => certificate_schema(),
        "RawProposal" => all_required_schema(&[
            ("subject", string_schema()),
            ("predicate", string_schema()),
            ("object", string_schema()),
            ("evidence_id", string_schema()),
            ("quote", string_schema()),
            ("needs_interpretation", bool_schema()),
            ("rationale", string_schema()),
        ]),
        "CertificateResponse" => all_required_schema(&[
            ("certificate_id", string_schema()),
            ("question", string_schema()),
            ("source_id", string_schema()),
            ("model", string_schema()),
            ("alphabet_digest", string_schema()),
            ("alphabet_size", integer_schema()),
            ("certificate", schema_reference("Certificate")),
            ("proposal", schema_reference("RawProposal")),
        ]),
        "CertificateSummary" => all_required_schema(&[
            ("id", string_schema()),
            ("question", string_schema()),
            ("verdict", schema_reference("KernelVerdict")),
            ("model", string_schema()),
            ("alphabet_digest", string_schema()),
            ("certificate", schema_reference("Certificate")),
        ]),
        _ => return None,
    })
}

fn certificate_schema() -> Value {
    let mut schema = all_required_schema(&[
        ("verdict", schema_reference("KernelVerdict")),
        ("goal", schema_reference("Proposition")),
        ("discharged", array_ref("DischargedStep")),
        ("residuals", array_ref("Residual")),
        ("errors", array_ref("CertificateError")),
        ("replay_digest", string_schema()),
    ]);
    schema["description"] = json!(
        "Current deterministic kernel certificate. `unverified` is not a kernel verdict; it belongs to the future platform assurance layer."
    );
    schema["examples"] = json!([
        {"verdict":"accepted","goal":{"subject":"document","predicate":"states","object":"fact"},"discharged":[{"rule":"cite","concluded":"fact","detail":"literal evidence matched"}],"residuals":[],"errors":[],"replay_digest":"sha256:accepted"},
        {"verdict":"conditional","goal":{"subject":"document","predicate":"implies","object":"fact"},"discharged":[],"residuals":[{"from":"attributed text","to":{"subject":"document","predicate":"implies","object":"fact"},"rationale":"interpretation required","evidence_ids":["evidence_1"]}],"errors":[],"replay_digest":"sha256:conditional"},
        {"verdict":"rejected","goal":{"subject":"document","predicate":"states","object":"missing"},"discharged":[],"residuals":[],"errors":[{"code":"quote_mismatch","message":"quote was not found","repair_owner":"proposer"}],"replay_digest":"sha256:rejected"}
    ]);
    schema
}

fn all_required_schema(properties: &[(&str, Value)]) -> Value {
    let required = properties.iter().map(|(name, _)| *name).collect::<Vec<_>>();
    object_schema(&required, properties)
}

fn array_ref(name: &str) -> Value {
    json!({"type": "array", "items": schema_reference(name)})
}

fn fields(names: &[&str]) -> Value {
    let properties = names
        .iter()
        .map(|name| (*name, string_schema()))
        .collect::<Vec<_>>();
    object_schema(names, &properties)
}

fn object_schema(required: &[&str], properties: &[(&str, Value)]) -> Value {
    let properties = properties
        .iter()
        .map(|(name, schema)| ((*name).to_owned(), schema.clone()))
        .collect::<Map<_, _>>();
    json!({"type": "object", "required": required, "properties": properties, "additionalProperties": false})
}

fn string_schema() -> Value {
    json!({"type": "string"})
}
fn integer_schema() -> Value {
    json!({"type": "integer"})
}
fn bool_schema() -> Value {
    json!({"type": "boolean"})
}
fn string_array_schema() -> Value {
    json!({"type": "array", "items": {"type": "string"}})
}
fn enum_schema(values: &[&str]) -> Value {
    json!({"type": "string", "enum": values})
}

fn humanize(operation_id: &str) -> String {
    let mut output = String::with_capacity(operation_id.len() + 8);
    for (index, character) in operation_id.chars().enumerate() {
        if index > 0 && character.is_ascii_uppercase() {
            output.push(' ');
        }
        output.push(character.to_ascii_lowercase());
    }
    output
}
