//! Deterministic `OpenAPI` generation from the runtime endpoint contract registry.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use serde_json::{Map, Value, json};
use utoipa::openapi::OpenApi;

use crate::endpoint_contract::{SchemaId, endpoint_contracts};

const OPENAPI_VERSION: &str = "3.1.0";

/// Components published whether or not an operation refers to them: the error
/// body every operation shares, the status vocabularies of the core domain, and
/// shapes clients use without reaching them through an operation.
fn always_published() -> Vec<SchemaId> {
    vec![
        SchemaId::of::<crate::error::ErrorResponse>(),
        SchemaId::of::<crate::wire_schemas::JobRunStatusSchema>(),
        SchemaId::of::<crate::wire_schemas::WorkflowStatusSchema>(),
        SchemaId::of::<crate::wire_schemas::WorkflowRunStatusSchema>(),
        SchemaId::of::<crate::wire_schemas::AutomationStatusSchema>(),
    ]
}

/// Builds the public API contract from the same endpoint metadata used at runtime.
#[must_use]
/// # Panics
///
/// Panics if an internally-created path entry is not a JSON object, if a Rust
/// schema cannot be serialized, or if two different wire types claim one
/// component name. Each is a programmer error, caught by the contract tests.
pub fn generated_openapi() -> Value {
    let mut paths = Map::new();
    let mut schemas = BTreeMap::new();
    for schema in always_published() {
        publish(&mut schemas, schema);
    }

    for endpoint in endpoint_contracts() {
        if let Some(request_schema) = endpoint.request_schema {
            publish(&mut schemas, request_schema);
        }
        publish(&mut schemas, endpoint.response_schema);

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

    let schemas = schemas.into_iter().collect::<Map<_, _>>();

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
                    "schema": schema_reference(&name.name())
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
                        "schema": schema_reference(&endpoint.response_schema.name())
                    }
                }
            },
            "default": {
                "description": "Error response",
                "content": {
                    "application/json": {
                        "schema": schema_reference(&SchemaId::of::<crate::error::ErrorResponse>().name())
                    }
                }
            }
        },
        "security": security,
        "x-capsulet-stability": endpoint.stability.as_str(),
        "x-capsulet-required-scope": endpoint.policy.permission.as_str(),
        "x-capsulet-project-context": endpoint.project_context.is_required(),
        "x-capsulet-resource": endpoint.policy.resource.as_str(),
        "x-capsulet-ownership": endpoint.policy.ownership.as_str(),
        "x-capsulet-project-roles": ([crate::ProjectRole::Viewer, crate::ProjectRole::Operator, crate::ProjectRole::Admin]
            .into_iter().filter(|role| endpoint.policy.allows(*role, false))
            .map(crate::ProjectRole::as_str).collect::<Vec<_>>())
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
            "content": {"application/json": {"schema": schema_reference(&SchemaId::of::<crate::webhooks::WebhookResponse>().name())}}
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
    // The parameters an endpoint reads are the type its handler takes them from.
    for parameter in endpoint
        .parameters
        .iter()
        .flat_map(|parameters| parameters.parameters())
    {
        parameters.push(serde_json::to_value(parameter).expect("query parameters serialize"));
    }
    parameters
}

fn schema_reference(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
}

/// Adds a type's schema and every schema it refers to.
fn publish(schemas: &mut BTreeMap<String, Value>, schema: SchemaId) {
    let own = (schema.name().into_owned(), schema.schema());
    for (name, component) in std::iter::once(own).chain(schema.dependencies()) {
        let component = serde_json::to_value(component).expect("wire schemas serialize to JSON");
        match schemas.entry(name) {
            Entry::Vacant(slot) => {
                slot.insert(component);
            }
            Entry::Occupied(existing) => assert_eq!(
                existing.get(),
                &component,
                "two different wire types claim the component name {}",
                existing.key()
            ),
        }
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

/// Holds every response the API's own tests receive to the published contract.
///
/// In test builds each route carries a check, added by `ContractRouter`, that
/// compares what the handler returned with what its contract publishes: the
/// declared success status, a body matching the response schema for JSON, the
/// declared content type for text, binary and event streams (which are never
/// buffered, since a stream does not end), and the shared `Error` body for every
/// failure. A mismatch fails the test that made the request. The suite makes
/// several hundred real requests through every operation it exercises, so this
/// turns them into conformance checks of the operations' real payloads.
#[cfg(any(test, feature = "contract-conformance"))]
pub(crate) mod conformance {
    use std::sync::OnceLock;

    use axum::body::Body;
    use axum::extract::Request;
    use axum::http::header::CONTENT_TYPE;
    use axum::middleware::Next;
    use axum::response::Response;
    use serde_json::{Value, json};

    use crate::{EndpointContract, MediaType, ResponseStatus};

    fn document() -> &'static Value {
        static DOCUMENT: OnceLock<Value> = OnceLock::new();
        DOCUMENT.get_or_init(super::generated_openapi)
    }

    /// Checks one response against the contract of the route that produced it.
    ///
    /// # Panics
    ///
    /// When the response is not what the contract publishes.
    pub(crate) async fn check(
        contract: &'static EndpointContract,
        request: Request,
        next: Next,
    ) -> Response {
        let response = next.run(request).await;
        let status = response.status();
        // `CAPSULET_CONFORMANCE_TRACE=<file> cargo test` appends one line per
        // checked response to `<file>`, to measure which operations and
        // statuses the suite reaches.
        if let Some(trace) = std::env::var_os("CAPSULET_CONFORMANCE_TRACE") {
            use std::io::Write as _;
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(trace)
            {
                // One write per line, so lines from parallel tests do not interleave.
                let line = format!("{} {}\n", contract.operation_id, status.as_u16());
                let _ = file.write_all(line.as_bytes());
            }
        }
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let operation = contract.operation_id;
        if content_type.starts_with("text/event-stream") {
            assert_eq!(
                contract.response_content_type,
                MediaType::EventStream,
                "{operation} streamed events, which its contract does not declare"
            );
            assert_eq!(
                status.as_str(),
                contract.success_status.as_str(),
                "{operation} streamed with an undeclared status"
            );
            return response;
        }
        let (parts, body) = response.into_parts();
        let bytes = axum::body::to_bytes(body, usize::MAX)
            .await
            .expect("a response body can be read");
        if status.is_success() {
            let alternative = (operation == "ingestWebhook" && status.as_u16() == 200)
                .then(super::SchemaId::of::<crate::webhooks::WebhookResponse>);
            assert!(
                status.as_str() == contract.success_status.as_str() || alternative.is_some(),
                "{operation} answered {status}, but its contract declares {}",
                contract.success_status.as_str()
            );
            if contract.success_status == ResponseStatus::NoContent {
                assert!(bytes.is_empty(), "{operation} answered 204 with a body");
            } else {
                let declared = contract.response_content_type.as_str();
                assert!(
                    contract.response_content_type == MediaType::Any
                        || content_type.starts_with(declared),
                    "{operation} answered with content type `{content_type}`, but its contract declares `{declared}`"
                );
                if contract.response_content_type == MediaType::Json {
                    let schema = alternative.unwrap_or(contract.response_schema);
                    assert_body(operation, &schema.name(), &bytes);
                }
            }
        } else {
            assert!(
                content_type.starts_with("application/json"),
                "{operation} failed with {status} and content type `{content_type}`, not the JSON error every contract declares: {}",
                String::from_utf8_lossy(&bytes)
            );
            let error = super::SchemaId::of::<crate::error::ErrorResponse>().name();
            assert_body(operation, &error, &bytes);
        }
        Response::from_parts(parts, Body::from(bytes))
    }

    fn assert_body(operation: &str, component: &str, bytes: &[u8]) {
        let value: Value = serde_json::from_slice(bytes).unwrap_or_else(|error| {
            panic!(
                "{operation} sent a body that is not JSON ({error}): {}",
                String::from_utf8_lossy(bytes)
            )
        });
        let schema = json!({"$ref": format!("#/components/schemas/{component}")});
        if let Err(problem) = conforms(document(), &schema, &value, component) {
            panic!(
                "{operation} sent a {component} its contract does not describe: {problem}\n{value:#}"
            );
        }
    }

    /// Whether `value` is what `schema` describes, following references into `document`.
    ///
    /// Covers what these schemas use: types (including nullable unions), enums,
    /// required properties, nested objects and arrays, and `oneOf`/`allOf`/`anyOf`.
    /// A property the schema does not list is drift: the payload carries something
    /// the contract does not describe. Only an explicitly open object
    /// (`additionalProperties: true`) may carry fields it does not list.
    pub(crate) fn conforms(
        document: &Value,
        schema: &Value,
        value: &Value,
        path: &str,
    ) -> Result<(), String> {
        if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
            let name = reference.trim_start_matches("#/components/schemas/");
            let target = &document["components"]["schemas"][name];
            if target.is_null() {
                return Err(format!("{path}: {reference} does not resolve"));
            }
            return conforms(document, target, value, path);
        }
        if let Some(options) = schema.get("oneOf").and_then(Value::as_array) {
            let matching = options
                .iter()
                .filter(|option| conforms(document, option, value, path).is_ok())
                .count();
            return if matching == 1 {
                Ok(())
            } else {
                Err(format!(
                    "{path}: {value} matches {matching} oneOf options, not one"
                ))
            };
        }
        if let Some(options) = schema.get("anyOf").and_then(Value::as_array) {
            return if options
                .iter()
                .any(|option| conforms(document, option, value, path).is_ok())
            {
                Ok(())
            } else {
                Err(format!("{path}: {value} matches no anyOf option"))
            };
        }
        if let Some(parts) = schema.get("allOf").and_then(Value::as_array) {
            for part in parts {
                conforms(document, part, value, path)?;
            }
        }
        let types = match schema.get("type") {
            Some(Value::String(single)) => vec![single.as_str()],
            Some(Value::Array(many)) => many.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        if !types.is_empty() && !types.iter().any(|kind| is_type(kind, value)) {
            return Err(format!("{path}: {value} is not {types:?}"));
        }
        if let Some(allowed) = schema.get("enum").and_then(Value::as_array)
            && !allowed.contains(value)
        {
            return Err(format!("{path}: {value} is not one of {allowed:?}"));
        }
        match value {
            Value::Object(object) => {
                for required in schema
                    .get("required")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    if !object.contains_key(required) {
                        return Err(format!("{path}.{required}: required but absent"));
                    }
                }
                let properties = schema.get("properties").and_then(Value::as_object);
                let open = schema.get("additionalProperties") == Some(&Value::Bool(true));
                for (key, item) in object {
                    match properties.and_then(|properties| properties.get(key)) {
                        Some(property) => {
                            conforms(document, property, item, &format!("{path}.{key}"))?;
                        }
                        None if open || properties.is_none() => {}
                        None => return Err(format!("{path}.{key}: not described by the schema")),
                    }
                }
            }
            Value::Array(items) => {
                if let Some(item_schema) = schema.get("items") {
                    for (index, item) in items.iter().enumerate() {
                        conforms(document, item_schema, item, &format!("{path}[{index}]"))?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn is_type(kind: &str, value: &Value) -> bool {
        match kind {
            "string" => value.is_string(),
            "integer" => value.is_i64() || value.is_u64(),
            "number" => value.is_number(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            "object" => value.is_object(),
            "array" => value.is_array(),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::response::IntoResponse;
    use capsulet_kernel::{
        Certificate, CertificateError, DischargedStep, Proposition, Quantity, Residual, Verdict,
    };
    use serde_json::{Value, json};

    use super::conformance::conforms;
    use super::{SchemaId, generated_openapi, publish};

    /// Asserts a real payload is what the published component `name` describes.
    fn assert_conforms(name: &str, value: &Value) {
        let document = generated_openapi();
        let schema = json!({"$ref": format!("#/components/schemas/{name}")});
        if let Err(problem) = conforms(&document, &schema, value, name) {
            panic!(
                "a real {name} payload does not match its published schema: {problem}\n{value:#}"
            );
        }
    }

    #[test]
    fn every_schema_reference_resolves() {
        let document = generated_openapi();
        let text = serde_json::to_string(&document).expect("serializes");
        let components = document["components"]["schemas"]
            .as_object()
            .expect("components");
        let dangling = text
            .split("\"#/components/schemas/")
            .skip(1)
            .filter_map(|rest| rest.split('"').next())
            .filter(|name| !components.contains_key(*name))
            .collect::<std::collections::BTreeSet<_>>();
        assert!(
            dangling.is_empty(),
            "references to missing components: {dangling:?}"
        );
    }

    #[test]
    fn the_checker_refuses_what_a_schema_does_not_describe() {
        let document = json!({"components": {"schemas": {}}});
        let schema = json!({"type": "object", "required": ["a"], "properties": {"a": {"type": ["string", "null"]}}});
        assert!(conforms(&document, &schema, &json!({"a": null}), "x").is_ok());
        assert!(
            conforms(&document, &schema, &json!({}), "x").is_err(),
            "missing required"
        );
        assert!(
            conforms(&document, &schema, &json!({"a": 1}), "x").is_err(),
            "wrong type"
        );
        assert!(
            conforms(&document, &schema, &json!({"a": "", "b": 1}), "x").is_err(),
            "undeclared"
        );
    }

    #[derive(utoipa::ToSchema)]
    #[schema(as = Clash)]
    #[allow(dead_code, reason = "only its schema is used")]
    struct One {
        one: String,
    }

    #[derive(utoipa::ToSchema)]
    #[schema(as = Clash)]
    #[allow(dead_code, reason = "only its schema is used")]
    struct Other {
        other: bool,
    }

    #[test]
    #[should_panic(expected = "two different wire types claim the component name Clash")]
    fn two_types_cannot_publish_one_component_name() {
        let mut schemas = BTreeMap::new();
        publish(&mut schemas, SchemaId::of::<One>());
        publish(&mut schemas, SchemaId::of::<Other>());
    }

    #[tokio::test]
    async fn an_error_response_is_what_the_error_component_describes() {
        use http_body_util::BodyExt;

        let response = crate::error::ApiError::ResourceNotFound.into_response();
        let body = response
            .into_body()
            .collect()
            .await
            .expect("reads the body")
            .to_bytes();
        let value: Value = serde_json::from_slice(&body).expect("an error body is JSON");
        assert_conforms("Error", &value);
    }

    #[test]
    fn a_service_account_with_nothing_recorded_yet_matches_its_schema() {
        let account = crate::models::CreateServiceAccountResponse {
            id: "sa_1".to_string(),
            name: "builder".to_string(),
            tenant_id: "tenant_1".to_string(),
            project_id: "project_1".to_string(),
            role: "operator".to_string(),
            scopes: vec!["jobs:run".to_string()],
            expires_at: None,
            revoked_at: None,
            last_used_at: None,
            created_at: "2026-11-08T00:00:00Z".to_string(),
            token: "token".to_string(),
        };
        let value = serde_json::to_value(&account).expect("serializes");
        for nullable in ["expires_at", "revoked_at", "last_used_at"] {
            assert_eq!(value[nullable], Value::Null, "{nullable} is sent, as null");
        }
        assert_conforms("CreateServiceAccountResponse", &value);
    }

    #[test]
    fn a_webhook_reply_matches_its_schema() {
        let reply = crate::webhooks::WebhookResponse {
            accepted: true,
            event_id: "evt_1".to_string(),
        };
        assert_conforms(
            "WebhookResponse",
            &serde_json::to_value(&reply).expect("serializes"),
        );
    }

    #[test]
    fn a_raw_proposal_matches_its_schema() {
        let proposal = capsulet_proposer::RawProposal {
            subject: "invoice".to_string(),
            predicate: "totals".to_string(),
            object: "46".to_string(),
            evidence_id: "evidence_1".to_string(),
            quote: "total: 46".to_string(),
            needs_interpretation: false,
            rationale: String::new(),
            asks_relation: "totals".to_string(),
            asks_subject: "invoice".to_string(),
            asks_object: "?".to_string(),
            asks_rationale: "asks for the invoice's total".to_string(),
        };
        assert_conforms(
            "RawProposal",
            &serde_json::to_value(&proposal).expect("serializes"),
        );
    }

    /// The response check fails a request whose body its contract does not describe.
    #[tokio::test]
    #[should_panic(expected = "getHealth sent a HealthResponse its contract does not describe")]
    async fn a_response_its_contract_does_not_describe_fails_the_request() {
        use axum::body::Body;
        use axum::http::Request;
        use axum::{Json, Router, middleware, routing::get};
        use tower::ServiceExt;

        let contract = crate::find_operation("getHealth").expect("a declared operation");
        let app = Router::new()
            .route(
                contract.path,
                get(|| async { Json(json!({"status": "asleep", "extra": true})) }),
            )
            .route_layer(middleware::from_fn(move |request, next| {
                super::conformance::check(contract, request, next)
            }));
        let _ = app
            .oneshot(
                Request::get(contract.path)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await;
    }

    /// The signed-delivery headers the document lists are the ones the handler reads.
    #[test]
    fn webhook_headers_publish_the_names_the_handler_reads() {
        use crate::webhooks::{
            CORRELATION_HEADER, DELIVERY_HEADER, SIGNATURE_HEADER, TIMESTAMP_HEADER,
        };

        let document = generated_openapi();
        let published =
            document["paths"]["/v1/webhooks/{automation_id}/{trigger_name}"]["post"]["parameters"]
                .as_array()
                .expect("parameters")
                .iter()
                .filter(|parameter| parameter["in"] == "header")
                .filter_map(|parameter| {
                    let name = parameter["name"].as_str()?;
                    (name != "x-request-id" && name != "x-capsulet-project-id")
                        .then(|| (name.to_string(), parameter["required"] == true))
                })
                .collect::<Vec<_>>();
        let read = [
            (TIMESTAMP_HEADER, true),
            (DELIVERY_HEADER, true),
            (CORRELATION_HEADER, false),
            (SIGNATURE_HEADER, true),
        ]
        .map(|(name, required)| (name.to_string(), required));
        assert_eq!(published, read);
    }

    /// A listing's published `limit` is what its handler clamps to and falls back on.
    #[test]
    fn published_list_limits_are_the_ones_handlers_apply() {
        use crate::models::{
            LIST_LIMIT_MAX, ListJobDefinitionsQuery, ListRunsQuery, ListWorkflowRunsQuery,
        };

        let document = generated_openapi();
        for (method, path, default) in [
            ("get", "/v1/jobs/runs", ListRunsQuery::DEFAULT_LIMIT),
            (
                "get",
                "/v1/workflow-runs",
                ListWorkflowRunsQuery::DEFAULT_LIMIT,
            ),
            (
                "get",
                "/v1/job-definitions",
                ListJobDefinitionsQuery::DEFAULT_LIMIT,
            ),
        ] {
            let limit = document["paths"][path][method]["parameters"]
                .as_array()
                .expect("parameters")
                .iter()
                .find(|parameter| parameter["name"] == "limit")
                .unwrap_or_else(|| panic!("{path} publishes no limit"));
            assert_eq!(limit["in"], "query", "{path}");
            assert_eq!(limit["schema"]["maximum"], json!(LIST_LIMIT_MAX), "{path}");
            assert_eq!(limit["schema"]["minimum"], json!(1), "{path}");
            assert_eq!(limit["schema"]["default"], json!(default), "{path}");
        }
    }

    /// The certificate schema is written out by hand, because the kernel crate
    /// carries no schema derivation. So it is held to real certificates: one
    /// with every optional field present, and one with none.
    #[test]
    fn kernel_certificates_match_the_published_certificate() {
        let goal = Proposition {
            subject: "invoice".to_string(),
            predicate: "totals".to_string(),
            object: "46".to_string(),
        };
        let full = Certificate {
            verdict: Verdict::Rejected,
            goal: goal.clone(),
            discharged: vec![DischargedStep {
                rule: "cite".to_string(),
                concluded: "invoice totals 46".to_string(),
                detail: "literal evidence matched".to_string(),
            }],
            residuals: vec![Residual {
                kind: capsulet_kernel::ResidualKind::Source,
                from: "attributed text".to_string(),
                to: goal.clone(),
                rationale: "interpretation required".to_string(),
                evidence_ids: vec!["evidence_1".to_string()],
            }],
            errors: vec![CertificateError {
                code: "arithmetic_mismatch".to_string(),
                message: "the total does not add up".to_string(),
                repair_owner: "proposer".to_string(),
                corrected_value: Some(Quantity::integer(46)),
            }],
            replay_digest: "sha256:full".to_string(),
            derivation_depth_limit: Some(64),
        };
        assert_conforms(
            "Certificate",
            &serde_json::to_value(&full).expect("serializes"),
        );

        let bare = Certificate {
            verdict: Verdict::Accepted,
            goal,
            discharged: Vec::new(),
            residuals: Vec::new(),
            errors: Vec::new(),
            replay_digest: "sha256:bare".to_string(),
            derivation_depth_limit: None,
        };
        assert_conforms(
            "Certificate",
            &serde_json::to_value(&bare).expect("serializes"),
        );
    }
}
