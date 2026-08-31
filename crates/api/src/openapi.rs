//! Deterministic OpenAPI generation from the runtime endpoint contract registry.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};
use utoipa::openapi::OpenApi;

use crate::endpoint_contract::endpoint_contracts;

const OPENAPI_VERSION: &str = "3.1.0";

/// Builds the public API contract from the same endpoint metadata used at runtime.
#[must_use]
pub fn generated_openapi() -> Value {
    let mut paths = Map::new();
    let mut schema_names = BTreeSet::from([
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
    ]);

    for endpoint in endpoint_contracts() {
        if let Some(request_schema) = endpoint.request_schema {
            schema_names.insert(request_schema);
        }
        schema_names.insert(endpoint.response_schema);

        let item = paths
            .entry(endpoint.path.to_owned())
            .or_insert_with(|| Value::Object(Map::new()));
        let methods = item
            .as_object_mut()
            .expect("path items are created as JSON objects");
        methods.insert(
            endpoint.method.to_ascii_lowercase(),
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

/// Parses the generated JSON through utoipa's OpenAPI model.
pub fn validated_openapi() -> Result<OpenApi, serde_json::Error> {
    serde_json::from_value(generated_openapi())
}

/// Returns the canonical checked-in representation.
pub fn canonical_openapi_json() -> Result<String, serde_json::Error> {
    let mut output = serde_json::to_string_pretty(&generated_openapi())?;
    output.push('\n');
    Ok(output)
}

fn operation_document(endpoint: &crate::EndpointContract) -> Value {
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
        .chain(endpoint.project_context.then(|| {
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
    if endpoint.method == "GET" && matches!(endpoint.path, "/v1/jobs/runs" | "/v1/workflow-runs") {
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
    } else if endpoint.method == "GET" && endpoint.path == "/v1/job-definitions" {
        parameters.push(query_parameter("limit"));
    }
    if endpoint.method == "GET"
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

    let request_body = endpoint.request_schema.map(|name| {
        json!({
            "required": true,
            "content": {
                "application/json": {
                    "schema": schema_reference(name)
                }
            }
        })
    });
    let security = if endpoint.authenticated {
        json!([{"bearerAuth": []}])
    } else {
        json!([])
    };

    let mut operation = json!({
        "operationId": endpoint.operation_id,
        "summary": humanize(endpoint.operation_id),
        "parameters": parameters,
        "responses": {
            endpoint.success_status: {
                "description": "Successful response",
                "content": {
                    endpoint.response_content_type: {
                        "schema": schema_reference(endpoint.response_schema)
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
        "x-capsulet-stability": endpoint.stability,
        "x-capsulet-required-scope": endpoint.required_scope,
        "x-capsulet-project-context": endpoint.project_context
    });
    if let Some(body) = request_body {
        operation["requestBody"] = body;
    }
    if endpoint.operation_id == "startAgentRun" {
        operation["description"] = json!(
            "Persists an experimental agent run in queued state. This endpoint does not claim that a production graph worker will execute it."
        );
    }
    if endpoint.success_status == "204" {
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
        other if control_plane_schema(other).is_some() => {
            control_plane_schema(other).expect("matched control-plane component")
        }
        other if agent_memory_schema(other).is_some() => {
            agent_memory_schema(other).expect("matched agent/memory component")
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

fn control_plane_schema(name: &str) -> Option<Value> {
    let schema = match name {
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
        "ProjectResponse" => fields(&["id", "tenant_id", "name"]),
        "UpsertProjectMembershipRequest" => fields(&["principal_kind", "principal_name", "role"]),
        "ProjectMembershipResponse" => fields(&[
            "id",
            "tenant_id",
            "project_id",
            "principal_kind",
            "principal_name",
            "role",
            "created_by",
            "created_at",
            "updated_at",
        ]),
        "CreateServiceAccountRequest" => object_schema(
            &["name", "role"],
            &[
                ("id", nullable_string_schema()),
                ("name", string_schema()),
                ("role", string_schema()),
                ("tenant_id", nullable_string_schema()),
                ("project_id", nullable_string_schema()),
                ("scopes", string_array_schema()),
                ("expires_at_unix", nullable_integer_schema()),
            ],
        ),
        "ServiceAccountResponse" => object_schema(
            &[
                "id",
                "name",
                "tenant_id",
                "project_id",
                "role",
                "scopes",
                "created_at",
            ],
            &[
                ("id", string_schema()),
                ("name", string_schema()),
                ("tenant_id", string_schema()),
                ("project_id", string_schema()),
                ("role", string_schema()),
                ("scopes", string_array_schema()),
                ("expires_at", nullable_string_schema()),
                ("revoked_at", nullable_string_schema()),
                ("last_used_at", nullable_string_schema()),
                ("created_at", string_schema()),
            ],
        ),
        "CreateServiceAccountResponse" => object_schema(
            &[
                "id",
                "name",
                "tenant_id",
                "project_id",
                "role",
                "scopes",
                "created_at",
                "token",
            ],
            &[
                ("id", string_schema()),
                ("name", string_schema()),
                ("tenant_id", string_schema()),
                ("project_id", string_schema()),
                ("role", string_schema()),
                ("scopes", string_array_schema()),
                ("created_at", string_schema()),
                ("token", string_schema()),
            ],
        ),
        "CreateRunRequest" => object_schema(
            &["job_definition_id", "execution_pool"],
            &[
                ("job_definition_id", string_schema()),
                ("execution_pool", string_schema()),
                ("run_id", nullable_string_schema()),
                ("python_script", nullable_string_schema()),
                ("input", nullable_object_schema()),
            ],
        ),
        "CreateJobDefinitionRequest" => object_schema(
            &["name", "python_script"],
            &[
                ("id", nullable_string_schema()),
                ("name", string_schema()),
                ("runtime_image", nullable_string_schema()),
                ("python_script", string_schema()),
                ("python_dependencies", string_array_schema()),
                ("input_schema", nullable_object_schema()),
                ("retry_max_attempts", nullable_integer_schema()),
                ("retry_delay_seconds", nullable_integer_schema()),
            ],
        ),
        "JobDefinitionResponse" => object_schema(
            &[
                "id",
                "name",
                "runtime_image",
                "command",
                "python_dependencies",
                "bundle_object_key",
                "input_schema",
                "retry_max_attempts",
                "retry_delay_seconds",
            ],
            &[
                ("id", string_schema()),
                ("name", string_schema()),
                ("runtime_image", string_schema()),
                ("command", string_array_schema()),
                ("python_dependencies", string_array_schema()),
                ("bundle_object_key", string_schema()),
                ("input_schema", json!({"type": "object"})),
                ("retry_max_attempts", integer_schema()),
                ("retry_delay_seconds", integer_schema()),
            ],
        ),
        "JobDefinitionSourceResponse" => object_schema(
            &["python_script", "python_dependencies"],
            &[
                ("python_script", string_schema()),
                ("python_dependencies", string_array_schema()),
            ],
        ),
        "JobRunResponse" => object_schema(
            &[
                "id",
                "job_definition_id",
                "status",
                "execution_pool",
                "host_group",
                "attempt_count",
                "created_at",
                "input",
            ],
            &[
                ("id", string_schema()),
                ("job_definition_id", string_schema()),
                ("status", schema_reference("JobRunStatus")),
                ("execution_pool", string_schema()),
                ("host_group", string_schema()),
                ("attempt_count", integer_schema()),
                ("created_at", string_schema()),
                ("input", json!({"type": "object"})),
            ],
        ),
        "JobRunLogsResponse" => object_schema(
            &["run_id", "logs", "object_log_available"],
            &[
                ("run_id", string_schema()),
                ("logs", string_schema()),
                ("object_log_available", bool_schema()),
            ],
        ),
        "ArtifactResponse" => object_schema(
            &["id", "run_id", "name", "content_type", "size_bytes", "kind"],
            &[
                ("id", string_schema()),
                ("run_id", string_schema()),
                ("name", string_schema()),
                ("content_type", string_schema()),
                ("size_bytes", integer_schema()),
                ("kind", string_schema()),
            ],
        ),
        "CreateWorkflowRequest" => object_schema(
            &["name", "steps"],
            &[
                ("id", nullable_string_schema()),
                ("name", string_schema()),
                ("description", nullable_string_schema()),
                (
                    "steps",
                    json!({"type": "array", "items": {"type": "object"}}),
                ),
                (
                    "dependencies",
                    json!({"type": ["array", "null"], "items": {"type": "object"}}),
                ),
                ("deadline_seconds", nullable_integer_schema()),
            ],
        ),
        "WorkflowResponse" => object_schema(
            &[
                "id",
                "name",
                "description",
                "status",
                "steps",
                "dependencies",
            ],
            &[
                ("id", string_schema()),
                ("name", string_schema()),
                ("description", string_schema()),
                ("status", schema_reference("WorkflowStatus")),
                (
                    "steps",
                    json!({"type": "array", "items": schema_reference("WorkflowStepResponse")}),
                ),
                (
                    "dependencies",
                    json!({"type": "array", "items": schema_reference("WorkflowDependencyResponse")}),
                ),
            ],
        ),
        "WorkflowStepResponse" => object_schema(
            &[
                "id",
                "position",
                "name",
                "job_definition_id",
                "execution_pool",
                "host_group",
                "timeout_seconds",
            ],
            &[
                ("id", string_schema()),
                ("position", integer_schema()),
                ("name", string_schema()),
                ("job_definition_id", string_schema()),
                ("execution_pool", string_schema()),
                ("host_group", string_schema()),
                ("timeout_seconds", nullable_integer_schema()),
            ],
        ),
        "WorkflowDependencyResponse" => fields(&["from_step_id", "to_step_id", "policy"]),
        "WorkflowEditabilityResponse" => object_schema(
            &["editable", "reason"],
            &[
                ("editable", bool_schema()),
                ("reason", nullable_string_schema()),
            ],
        ),
        "WorkflowRunResponse" => object_schema(
            &[
                "id",
                "workflow_id",
                "automation_id",
                "status",
                "current_step_position",
                "created_at",
                "step_runs",
            ],
            &[
                ("id", string_schema()),
                ("workflow_id", string_schema()),
                ("automation_id", nullable_string_schema()),
                ("status", schema_reference("WorkflowRunStatus")),
                ("current_step_position", integer_schema()),
                ("created_at", string_schema()),
                (
                    "step_runs",
                    json!({"type": "array", "items": schema_reference("WorkflowStepRunResponse")}),
                ),
            ],
        ),
        "WorkflowStepRunResponse" => object_schema(
            &["id", "workflow_step_id", "job_run_id", "position", "status"],
            &[
                ("id", string_schema()),
                ("workflow_step_id", string_schema()),
                ("job_run_id", nullable_string_schema()),
                ("position", integer_schema()),
                ("status", schema_reference("WorkflowRunStatus")),
            ],
        ),
        "WorkflowRunLogsResponse" => object_schema(
            &["workflow_run_id", "workflow_id", "status", "entries"],
            &[
                ("workflow_run_id", string_schema()),
                ("workflow_id", string_schema()),
                ("status", schema_reference("WorkflowRunStatus")),
                (
                    "entries",
                    json!({"type": "array", "items": schema_reference("WorkflowRunLogEntryResponse")}),
                ),
            ],
        ),
        "WorkflowRunLogEntryResponse" => object_schema(
            &[
                "step_run_id",
                "workflow_step_id",
                "job_run_id",
                "position",
                "status",
                "logs",
                "object_log_available",
            ],
            &[
                ("step_run_id", string_schema()),
                ("workflow_step_id", string_schema()),
                ("job_run_id", nullable_string_schema()),
                ("position", integer_schema()),
                ("status", schema_reference("WorkflowRunStatus")),
                ("logs", string_schema()),
                ("object_log_available", bool_schema()),
            ],
        ),
        "CreateAutomationRequest" => object_schema(
            &["name", "workflow_id"],
            &[
                ("id", nullable_string_schema()),
                ("name", string_schema()),
                ("description", nullable_string_schema()),
                ("workflow_id", string_schema()),
                ("status", nullable_string_schema()),
                ("job_input", nullable_object_schema()),
                (
                    "triggers",
                    json!({"type": ["array", "null"], "items": {"type": "object"}}),
                ),
                ("condition", nullable_object_schema()),
            ],
        ),
        "AutomationResponse" => object_schema(
            &[
                "id",
                "name",
                "description",
                "workflow_id",
                "status",
                "triggers",
                "condition",
                "job_input",
            ],
            &[
                ("id", string_schema()),
                ("name", string_schema()),
                ("description", string_schema()),
                ("workflow_id", string_schema()),
                ("status", schema_reference("AutomationStatus")),
                (
                    "triggers",
                    json!({"type": "array", "items": schema_reference("TriggerResponse")}),
                ),
                ("condition", json!({"type": "object"})),
                ("job_input", json!({"type": "object"})),
            ],
        ),
        "ListAutomationTriggersResponse" => object_schema(
            &["triggers", "condition"],
            &[
                (
                    "triggers",
                    json!({"type": "array", "items": schema_reference("TriggerResponse")}),
                ),
                ("condition", json!({"type": "object"})),
            ],
        ),
        "TriggerResponse" => object_schema(
            &["name", "kind", "config", "plugin_id", "enabled"],
            &[
                ("name", string_schema()),
                ("kind", string_schema()),
                ("config", json!({"type": "object"})),
                ("plugin_id", nullable_string_schema()),
                ("enabled", bool_schema()),
            ],
        ),
        "CreateTriggerPluginRequest" => object_schema(
            &["id", "name", "runtime_image"],
            &[
                ("id", string_schema()),
                ("name", string_schema()),
                ("description", nullable_string_schema()),
                ("runtime_image", string_schema()),
                (
                    "command",
                    json!({"type": ["array", "null"], "items": {"type": "string"}}),
                ),
                ("python_script", nullable_string_schema()),
                ("config_schema", nullable_object_schema()),
            ],
        ),
        "TriggerPluginResponse" => object_schema(
            &[
                "id",
                "name",
                "description",
                "runtime_image",
                "command",
                "python_script",
                "config_schema",
            ],
            &[
                ("id", string_schema()),
                ("name", string_schema()),
                ("description", string_schema()),
                ("runtime_image", string_schema()),
                ("command", string_array_schema()),
                ("python_script", string_schema()),
                ("config_schema", json!({"type": "object"})),
            ],
        ),
        "ExecutionPoolResponse" => object_schema(
            &["name", "description", "is_default", "host_group"],
            &[
                ("name", string_schema()),
                ("description", string_schema()),
                ("is_default", bool_schema()),
                ("host_group", string_schema()),
            ],
        ),
        "HostGroupResponse" => object_schema(
            &[
                "name",
                "description",
                "is_default",
                "execution_pool",
                "host_count",
            ],
            &[
                ("name", string_schema()),
                ("description", string_schema()),
                ("is_default", bool_schema()),
                ("execution_pool", string_schema()),
                ("host_count", nullable_integer_schema()),
            ],
        ),
        "AuditEventResponse" => object_schema(
            &[
                "id",
                "principal",
                "role",
                "method",
                "path",
                "status_code",
                "request_id",
                "created_at",
            ],
            &[
                ("id", integer_schema()),
                ("principal", string_schema()),
                ("role", string_schema()),
                ("method", string_schema()),
                ("path", string_schema()),
                ("status_code", integer_schema()),
                ("request_id", nullable_string_schema()),
                ("created_at", string_schema()),
            ],
        ),
        "TopologyResponse" => object_schema(
            &["nodes", "edges"],
            &[
                (
                    "nodes",
                    json!({"type": "array", "items": schema_reference("TopologyNodeResponse")}),
                ),
                (
                    "edges",
                    json!({"type": "array", "items": schema_reference("TopologyEdgeResponse")}),
                ),
            ],
        ),
        "TopologyNodeResponse" => fields(&["id", "label", "kind", "status"]),
        "TopologyEdgeResponse" => fields(&["from", "to", "label"]),
        "WebhookRequest" => json!({"type": "object", "additionalProperties": true}),
        "WebhookResponse" => object_schema(
            &["accepted", "event_id"],
            &[("accepted", bool_schema()), ("event_id", string_schema())],
        ),
        _ => return None,
    };
    Some(schema)
}

fn agent_memory_schema(name: &str) -> Option<Value> {
    let schema = match name {
        "CreateGraphRequest" => object_schema(
            &["name", "nodes", "hyperedges"],
            &[
                ("id", nullable_string_schema()),
                ("name", string_schema()),
                ("nodes", array_ref("GraphNodeRequest")),
                ("hyperedges", array_ref("GraphHyperedgeRequest")),
                (
                    "transition_policy",
                    nullable_ref("GraphTransitionPolicyRequest"),
                ),
            ],
        ),
        "GraphNodeRequest" | "GraphNodeResponse" => all_required_schema(&[
            ("id", string_schema()),
            ("name", string_schema()),
            ("kind", string_schema()),
            (
                "ports",
                array_ref(if name.ends_with("Request") {
                    "GraphPortRequest"
                } else {
                    "GraphPortResponse"
                }),
            ),
        ]),
        "GraphPortRequest" | "GraphPortResponse" => fields(&["id", "direction", "value_type"]),
        "GraphHyperedgeRequest" | "GraphHyperedgeResponse" => all_required_schema(&[
            ("id", string_schema()),
            (
                "sources",
                array_ref(if name.ends_with("Request") {
                    "HyperedgeEndpointRequest"
                } else {
                    "HyperedgeEndpointResponse"
                }),
            ),
            (
                "targets",
                array_ref(if name.ends_with("Request") {
                    "HyperedgeEndpointRequest"
                } else {
                    "HyperedgeEndpointResponse"
                }),
            ),
        ]),
        "HyperedgeEndpointRequest" | "HyperedgeEndpointResponse" => all_required_schema(&[
            ("kind", string_schema()),
            ("node_id", nullable_string_schema()),
            ("port_id", nullable_string_schema()),
            ("field", nullable_string_schema()),
            ("value_type", nullable_string_schema()),
        ]),
        "GraphTransitionPolicyRequest" => object_schema(
            &["mode"],
            &[
                ("mode", string_schema()),
                ("actions", string_array_schema()),
                ("cycles_allowed", bool_schema()),
            ],
        ),
        "GraphTransitionPolicyResponse" => all_required_schema(&[
            ("mode", string_schema()),
            ("actions", string_array_schema()),
            ("cycles_allowed", bool_schema()),
        ]),
        "GraphResponse" => all_required_schema(&[
            ("id", string_schema()),
            ("name", string_schema()),
            ("nodes", array_ref("GraphNodeResponse")),
            ("hyperedges", array_ref("GraphHyperedgeResponse")),
            (
                "transition_policy",
                schema_reference("GraphTransitionPolicyResponse"),
            ),
            ("static_order", string_array_schema()),
        ]),
        "CreateAgentRequest" => object_schema(
            &["name", "graph_id", "budget"],
            &[
                ("id", nullable_string_schema()),
                ("name", string_schema()),
                ("graph_id", string_schema()),
                ("budget", schema_reference("AgentBudgetRequest")),
                ("termination_conditions", string_array_schema()),
            ],
        ),
        "AgentBudgetRequest" | "AgentBudgetResponse" => all_required_schema(&[
            ("max_steps", integer_schema()),
            ("max_tokens", integer_schema()),
            ("max_seconds", integer_schema()),
            ("max_cost_micros", integer_schema()),
        ]),
        "AgentResponse" => all_required_schema(&[
            ("id", string_schema()),
            ("name", string_schema()),
            ("graph_id", string_schema()),
            ("budget", schema_reference("AgentBudgetResponse")),
            ("termination_conditions", string_array_schema()),
        ]),
        "StartAgentRunRequest" => object_schema(
            &[],
            &[
                ("id", nullable_string_schema()),
                ("initial_state", json!({"type": "object"})),
            ],
        ),
        "AgentRunResponse" => {
            let mut value = all_required_schema(&[
                ("id", string_schema()),
                ("agent_id", string_schema()),
                (
                    "status",
                    enum_schema(&[
                        "queued",
                        "running",
                        "succeeded",
                        "failed",
                        "cancelled",
                        "stopped",
                    ]),
                ),
                ("state_version", integer_schema()),
                ("state", json!({"type": "object"})),
            ]);
            value["description"] = json!(
                "Current experimental persisted agent-run state. Starting a run creates queued work; a production graph worker is not yet claimed."
            );
            value
        }
        "AskRequest" => object_schema(
            &["text", "question"],
            &[
                ("text", string_schema()),
                ("question", string_schema()),
                ("title", nullable_string_schema()),
                ("authority", nullable_string_schema()),
                ("model", nullable_string_schema()),
            ],
        ),
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
        "CreateMemorySourceRequest" => object_schema(
            &["kind", "title", "authority"],
            &[
                ("id", nullable_string_schema()),
                ("kind", string_schema()),
                ("uri", nullable_string_schema()),
                ("title", string_schema()),
                ("authority", string_schema()),
            ],
        ),
        "MemorySourceResponse" => scoped_schema(&[
            ("kind", string_schema()),
            ("uri", nullable_string_schema()),
            ("title", string_schema()),
            ("authority", string_schema()),
        ]),
        "CreateMemoryEvidenceRequest" => object_schema(
            &["source_id", "locator", "excerpt", "observed_at"],
            &[
                ("id", nullable_string_schema()),
                ("source_id", string_schema()),
                ("locator", string_schema()),
                ("excerpt", string_schema()),
                ("observed_at", string_schema()),
            ],
        ),
        "MemoryEvidenceResponse" => scoped_schema(&[
            ("source_id", string_schema()),
            ("locator", string_schema()),
            ("excerpt", string_schema()),
            ("observed_at", string_schema()),
        ]),
        "CreateMemoryEntityRequest" => object_schema(
            &["entity_type", "name"],
            &[
                ("id", nullable_string_schema()),
                ("entity_type", string_schema()),
                ("name", string_schema()),
                ("aliases", string_array_schema()),
            ],
        ),
        "MemoryEntityResponse" => scoped_schema(&[
            ("entity_type", string_schema()),
            ("name", string_schema()),
            ("aliases", string_array_schema()),
        ]),
        "CreateMemoryClaimRequest" => object_schema(
            &[
                "subject_id",
                "predicate",
                "object",
                "evidence_ids",
                "confidence",
                "authority",
                "observed_at",
            ],
            &[
                ("id", nullable_string_schema()),
                ("subject_id", string_schema()),
                ("predicate", string_schema()),
                ("object", string_schema()),
                ("evidence_ids", string_array_schema()),
                ("confidence", number_schema()),
                ("authority", string_schema()),
                ("status", nullable_string_schema()),
                ("observed_at", string_schema()),
                ("valid_from", nullable_string_schema()),
                ("valid_until", nullable_string_schema()),
            ],
        ),
        "MemoryClaimResponse" => scoped_schema(&[
            ("subject_id", string_schema()),
            ("predicate", string_schema()),
            ("object", string_schema()),
            ("evidence_ids", string_array_schema()),
            ("confidence", number_schema()),
            ("authority", string_schema()),
            (
                "status",
                enum_schema(&[
                    "candidate",
                    "active",
                    "rejected",
                    "superseded",
                    "contradicted",
                    "expired",
                ]),
            ),
            ("observed_at", string_schema()),
            ("valid_from", nullable_string_schema()),
            ("valid_until", nullable_string_schema()),
        ]),
        "ReviewClaimResponse" => {
            let mut properties = vec![
                ("subject_id", string_schema()),
                ("predicate", string_schema()),
                ("object", string_schema()),
                ("evidence_ids", string_array_schema()),
                ("confidence", number_schema()),
                ("authority", string_schema()),
                (
                    "status",
                    enum_schema(&[
                        "candidate",
                        "active",
                        "rejected",
                        "superseded",
                        "contradicted",
                        "expired",
                    ]),
                ),
                ("observed_at", string_schema()),
                ("valid_from", nullable_string_schema()),
                ("valid_until", nullable_string_schema()),
                ("evidence", array_ref("ReviewEvidenceResponse")),
                ("sources", array_ref("ReviewSourceResponse")),
            ];
            let mut base = vec![
                ("id", string_schema()),
                ("tenant_id", string_schema()),
                ("project_id", string_schema()),
            ];
            base.append(&mut properties);
            all_required_schema(&base)
        }
        "CreateMemoryEventRequest" => object_schema(
            &["event_type", "occurred_at", "evidence_ids"],
            &[
                ("id", nullable_string_schema()),
                ("event_type", string_schema()),
                ("occurred_at", string_schema()),
                ("entity_ids", string_array_schema()),
                ("evidence_ids", string_array_schema()),
            ],
        ),
        "MemoryEventResponse" => scoped_schema(&[
            ("event_type", string_schema()),
            ("occurred_at", string_schema()),
            ("entity_ids", string_array_schema()),
            ("evidence_ids", string_array_schema()),
        ]),
        "CreateMemoryRelationshipRequest" => object_schema(
            &[
                "relationship_type",
                "from_entity_id",
                "to_entity_id",
                "evidence_ids",
            ],
            &[
                ("id", nullable_string_schema()),
                ("relationship_type", string_schema()),
                ("from_entity_id", string_schema()),
                ("to_entity_id", string_schema()),
                ("evidence_ids", string_array_schema()),
            ],
        ),
        "MemoryRelationshipResponse" => scoped_schema(&[
            ("relationship_type", string_schema()),
            ("from_entity_id", string_schema()),
            ("to_entity_id", string_schema()),
            ("evidence_ids", string_array_schema()),
        ]),
        "CreateMemoryContractRequest" => object_schema(
            &["name", "source"],
            &[
                ("id", nullable_string_schema()),
                ("name", string_schema()),
                ("source", string_schema()),
            ],
        ),
        "MemoryContractResponse" => scoped_schema(&[
            ("name", string_schema()),
            ("source", string_schema()),
            (
                "compiled",
                json!({"type": "object", "description": "Compiled current memory-policy DSL."}),
            ),
        ]),
        "CreateMemorySubgraphRequest" => object_schema(
            &["name"],
            &[
                ("id", nullable_string_schema()),
                ("parent_subgraph_id", nullable_string_schema()),
                ("name", string_schema()),
                ("description", nullable_string_schema()),
            ],
        ),
        "ActivateMemorySubgraphRequest" => object_schema(
            &[
                "owner_kind",
                "owner_id",
                "contract_id",
                "permissions",
                "summary_claim_id",
            ],
            &[
                ("owner_kind", string_schema()),
                ("owner_id", string_schema()),
                ("contract_id", string_schema()),
                ("permissions", json!({"type": "object"})),
                ("summary_claim_id", string_schema()),
            ],
        ),
        "MemorySubgraphResponse" => scoped_schema(&[
            ("parent_subgraph_id", nullable_string_schema()),
            ("name", string_schema()),
            ("description", nullable_string_schema()),
            ("owner_kind", nullable_string_schema()),
            ("owner_id", nullable_string_schema()),
            ("contract_id", nullable_string_schema()),
            ("summary_claim_id", nullable_string_schema()),
            ("permissions", nullable_object_schema()),
            ("status", enum_schema(&["draft", "active", "archived"])),
        ]),
        "CreateMemorySubgraphMemberRequest" => object_schema(
            &["member_kind", "member_id", "role"],
            &[
                ("id", nullable_string_schema()),
                ("member_kind", string_schema()),
                ("member_id", string_schema()),
                ("role", string_schema()),
            ],
        ),
        "MemorySubgraphMemberResponse" => scoped_schema(&[
            ("subgraph_id", string_schema()),
            ("member_kind", string_schema()),
            ("member_id", string_schema()),
            ("role", string_schema()),
        ]),
        "CreateCanonicalEntityRequest" => object_schema(
            &["entity_type", "display_name"],
            &[
                ("id", nullable_string_schema()),
                ("entity_type", string_schema()),
                ("display_name", string_schema()),
                ("aliases", string_array_schema()),
            ],
        ),
        "CanonicalEntityResponse" => scoped_schema(&[
            ("entity_type", string_schema()),
            ("display_name", string_schema()),
            ("aliases", string_array_schema()),
        ]),
        "CreateEntityResolutionRequest" => object_schema(
            &[
                "subgraph_id",
                "entity_id",
                "canonical_entity_id",
                "confidence",
                "status",
                "evidence_ids",
            ],
            &[
                ("id", nullable_string_schema()),
                ("subgraph_id", string_schema()),
                ("entity_id", string_schema()),
                ("canonical_entity_id", string_schema()),
                ("confidence", number_schema()),
                ("status", string_schema()),
                ("evidence_ids", string_array_schema()),
            ],
        ),
        "EntityResolutionResponse" => scoped_schema(&[
            ("subgraph_id", string_schema()),
            ("entity_id", string_schema()),
            ("canonical_entity_id", string_schema()),
            ("confidence", number_schema()),
            (
                "status",
                enum_schema(&["candidate", "confirmed", "rejected"]),
            ),
            ("evidence_ids", string_array_schema()),
        ]),
        "ResolveClaimConflictRequest" => fields(&["preferred_claim_id"]),
        "ClaimConflictResponse" => scoped_schema(&[
            ("subject_id", string_schema()),
            ("canonical_entity_id", nullable_string_schema()),
            ("predicate", string_schema()),
            ("claim_ids", string_array_schema()),
            (
                "status",
                enum_schema(&["candidate", "resolved", "dismissed"]),
            ),
            ("reason", string_schema()),
            ("preferred_claim_id", nullable_string_schema()),
        ]),
        "CreateSummaryTraceRequest" => object_schema(
            &["subgraph_id", "summary_claim_id"],
            &[
                ("id", nullable_string_schema()),
                ("subgraph_id", string_schema()),
                ("summary_claim_id", string_schema()),
                ("inner_claim_ids", string_array_schema()),
                ("evidence_ids", string_array_schema()),
            ],
        ),
        "SummaryTraceResponse" => scoped_schema(&[
            ("subgraph_id", string_schema()),
            ("summary_claim_id", string_schema()),
            ("inner_claim_ids", string_array_schema()),
            ("evidence_ids", string_array_schema()),
        ]),
        "CreateEntityGraphAttachmentRequest" => object_schema(
            &["canonical_entity_id", "subgraph_id", "attachment_type"],
            &[
                ("id", nullable_string_schema()),
                ("canonical_entity_id", string_schema()),
                ("subgraph_id", string_schema()),
                ("attachment_type", string_schema()),
            ],
        ),
        "EntityGraphAttachmentResponse" => scoped_schema(&[
            ("canonical_entity_id", string_schema()),
            ("subgraph_id", string_schema()),
            ("attachment_type", string_schema()),
        ]),
        "CreateSubgraphEdgeRequest" => object_schema(
            &[
                "edge_type",
                "from_subgraph_id",
                "to_subgraph_id",
                "from_member_kind",
                "from_member_id",
                "to_member_kind",
                "to_member_id",
            ],
            &[
                ("id", nullable_string_schema()),
                ("edge_type", string_schema()),
                ("from_subgraph_id", string_schema()),
                ("to_subgraph_id", string_schema()),
                ("from_member_kind", string_schema()),
                ("from_member_id", string_schema()),
                ("to_member_kind", string_schema()),
                ("to_member_id", string_schema()),
                ("claim_ids", string_array_schema()),
                ("evidence_ids", string_array_schema()),
            ],
        ),
        "SubgraphEdgeResponse" => scoped_schema(&[
            ("edge_type", string_schema()),
            ("from_subgraph_id", string_schema()),
            ("to_subgraph_id", string_schema()),
            ("from_member_kind", string_schema()),
            ("from_member_id", string_schema()),
            ("to_member_kind", string_schema()),
            ("to_member_id", string_schema()),
            ("claim_ids", string_array_schema()),
            ("evidence_ids", string_array_schema()),
        ]),
        "CreateIngestionConnectorRequest" => object_schema(
            &["name", "kind", "config"],
            &[
                ("id", nullable_string_schema()),
                ("name", string_schema()),
                ("kind", enum_schema(&["local_text"])),
                (
                    "config",
                    all_required_schema(&[
                        ("title", string_schema()),
                        ("content", string_schema()),
                        ("content_type", string_schema()),
                        ("uri", nullable_string_schema()),
                        ("authority", string_schema()),
                    ]),
                ),
                ("enabled", bool_schema()),
            ],
        ),
        "IngestionConnectorResponse" => scoped_schema(&[
            ("name", string_schema()),
            ("kind", enum_schema(&["local_text"])),
            ("enabled", bool_schema()),
            (
                "config",
                all_required_schema(&[
                    ("title", string_schema()),
                    ("content_type", string_schema()),
                    ("uri", nullable_string_schema()),
                    ("authority", string_schema()),
                ]),
            ),
        ]),
        "IngestionRunResponse" => scoped_schema(&[
            ("connector_id", string_schema()),
            (
                "status",
                enum_schema(&["queued", "running", "succeeded", "failed"]),
            ),
            ("error", nullable_string_schema()),
            ("source_count", integer_schema()),
            ("evidence_count", integer_schema()),
            ("entity_count", integer_schema()),
            ("claim_count", integer_schema()),
            ("event_count", integer_schema()),
            ("relationship_count", integer_schema()),
        ]),
        "IngestionRunOutputsResponse" => all_required_schema(&[
            ("sources", string_array_schema()),
            ("evidence", string_array_schema()),
            ("entities", string_array_schema()),
            ("claims", string_array_schema()),
            ("events", string_array_schema()),
            ("relationships", string_array_schema()),
        ]),
        "IngestionRunWithOutputsResponse" => all_required_schema(&[
            ("run", schema_reference("IngestionRunResponse")),
            ("outputs", schema_reference("IngestionRunOutputsResponse")),
        ]),
        "ReviewEvidenceResponse" => {
            fields(&["id", "source_id", "locator", "excerpt", "observed_at"])
        }
        "ReviewSourceResponse" => all_required_schema(&[
            ("id", string_schema()),
            ("kind", string_schema()),
            ("uri", nullable_string_schema()),
            ("title", string_schema()),
            ("authority", string_schema()),
        ]),
        _ => return None,
    };
    Some(schema)
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

fn scoped_schema(extra: &[(&str, Value)]) -> Value {
    let mut properties = vec![
        ("id", string_schema()),
        ("tenant_id", string_schema()),
        ("project_id", string_schema()),
    ];
    properties.extend(extra.iter().cloned());
    all_required_schema(&properties)
}

fn all_required_schema(properties: &[(&str, Value)]) -> Value {
    let required = properties.iter().map(|(name, _)| *name).collect::<Vec<_>>();
    object_schema(&required, properties)
}

fn array_ref(name: &str) -> Value {
    json!({"type": "array", "items": schema_reference(name)})
}

fn nullable_ref(name: &str) -> Value {
    json!({"oneOf": [schema_reference(name), {"type": "null"}]})
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
fn nullable_string_schema() -> Value {
    json!({"type": ["string", "null"]})
}
fn integer_schema() -> Value {
    json!({"type": "integer"})
}
fn number_schema() -> Value {
    json!({"type": "number"})
}
fn nullable_integer_schema() -> Value {
    json!({"type": ["integer", "null"]})
}
fn bool_schema() -> Value {
    json!({"type": "boolean"})
}
fn string_array_schema() -> Value {
    json!({"type": "array", "items": {"type": "string"}})
}
fn nullable_object_schema() -> Value {
    json!({"type": ["object", "null"], "additionalProperties": true})
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
