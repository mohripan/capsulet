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
    let mut schema_names = BTreeSet::from(["Error"]);

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
    let parameters = endpoint
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
    operation
}

fn schema_reference(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
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
        "StringResponse" | "EventStreamResponse" => json!({"type": "string"}),
        "BinaryResponse" => json!({"type": "string", "format": "binary"}),
        "OpenApiDocument" => json!({"type": "object", "additionalProperties": true}),
        other if other.starts_with("List") => json!({
            "type": "object",
            "description": format!("{other} envelope."),
            "required": ["items"],
            "properties": {"items": {"type": "array", "items": {"type": "object", "additionalProperties": true}}},
            "additionalProperties": true
        }),
        other => json!({
            "type": "object",
            "description": format!("{other} payload."),
            "additionalProperties": true
        }),
    }
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
