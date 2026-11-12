//! Schemas for wire shapes that have no serde type of their own in this crate.
//!
//! Every other component of the `OpenAPI` document is derived from the Rust type a
//! handler serializes or deserializes, and endpoint contracts name those types,
//! not strings. What is left here has no such type to derive from: bodies that are
//! not JSON objects (text, binary, an event stream, the document itself), a webhook
//! payload that is arbitrary by design, the kernel's certificate types, which live
//! in a crate that deliberately depends on no HTTP schema library, and the status
//! vocabularies of the core domain. Each is still a type, so an endpoint names it
//! the same way and a misspelling is a compile error, not an empty schema.

use std::borrow::Cow;

use capsulet_core::{AutomationStatus, JobRunStatus, WorkflowRunStatus, WorkflowStatus};
use serde_json::{Value, json};
use utoipa::openapi::{RefOr, Schema};
use utoipa::{PartialSchema, ToSchema};

/// A schema read from JSON, which is how these are clearest to write down.
fn parse(value: Value) -> RefOr<Schema> {
    serde_json::from_value(value).expect("hand-written wire schemas are valid OpenAPI schemas")
}

fn reference(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
}

/// Declares a component whose schema is written out, with the components it refers to.
macro_rules! wire_schema {
    ($(#[$meta:meta])* $type:ident = $name:literal, [$($dependency:ty),* $(,)?], $schema:expr) => {
        $(#[$meta])*
        pub(crate) struct $type;

        impl PartialSchema for $type {
            fn schema() -> RefOr<Schema> {
                parse($schema)
            }
        }

        impl ToSchema for $type {
            fn name() -> Cow<'static, str> {
                Cow::Borrowed($name)
            }

            #[allow(unused_variables, reason = "a schema with no dependencies adds none")]
            fn schemas(schemas: &mut Vec<(String, RefOr<Schema>)>) {
                $(
                    schemas.push((<$dependency as ToSchema>::name().into_owned(), <$dependency as PartialSchema>::schema()));
                    <$dependency as ToSchema>::schemas(schemas);
                )*
            }
        }
    };
}

wire_schema!(
    /// A `text/plain` body, such as the Prometheus exposition.
    StringResponse = "StringResponse", [], json!({"type": "string"})
);
wire_schema!(
    /// Raw bytes, such as an artifact download in the media type it was recorded with.
    BinaryResponse = "BinaryResponse", [], json!({"type": "string", "format": "binary"})
);
wire_schema!(
    /// A Server-Sent Events stream.
    EventStreamResponse = "EventStreamResponse",
    [],
    json!({
        "type": "string",
        "format": "event-stream",
        "description": "UTF-8 Server-Sent Events stream. Each data field contains a JSON activity or log event."
    })
);
wire_schema!(
    /// The success body of a `204 No Content` operation.
    EmptyResponse = "EmptyResponse", [], json!({"type": "object", "maxProperties": 0})
);
wire_schema!(
    /// This document, served at `/openapi.json`.
    OpenApiDocument = "OpenApiDocument", [], json!({"type": "object", "additionalProperties": true})
);
wire_schema!(
    /// A webhook delivery: whatever JSON the sender signs is the payload.
    WebhookRequest = "WebhookRequest", [], json!({"type": "object", "additionalProperties": true})
);

wire_schema!(
    /// `capsulet_kernel::Verdict`.
    KernelVerdict = "KernelVerdict",
    [],
    json!({"type": "string", "enum": ["accepted", "conditional", "rejected"]})
);
wire_schema!(
    /// `capsulet_kernel::Proposition`.
    Proposition = "Proposition",
    [],
    all_required(&[
        ("subject", json!({"type": "string"})),
        ("predicate", json!({"type": "string"})),
        ("object", json!({"type": "string"})),
    ])
);
wire_schema!(
    /// A step the kernel discharged.
    DischargedStep = "DischargedStep",
    [],
    all_required(&[
        ("rule", json!({"type": "string"})),
        ("concluded", json!({"type": "string"})),
        ("detail", json!({"type": "string"})),
    ])
);
wire_schema!(
    /// An interpretation the kernel could not discharge, and why.
    Residual = "Residual",
    [Proposition],
    all_required(&[
        ("from", json!({"type": "string"})),
        ("to", reference("Proposition")),
        ("rationale", json!({"type": "string"})),
        ("evidence_ids", json!({"type": "array", "items": {"type": "string"}})),
    ])
);
wire_schema!(
    /// A reason the kernel rejected a proposal.
    CertificateError = "CertificateError",
    [],
    json!({
        "type": "object",
        "required": ["code", "message", "repair_owner"],
        "properties": {
            "code": {"type": "string"},
            "message": {"type": "string"},
            "repair_owner": {"type": "string"},
            // A fixed-point decimal as text, never a JSON number: the kernel
            // computes exactly, and a number here would invite a float back
            // into a pipeline that refuses them.
            "corrected_value": {"type": ["string", "null"], "example": "46"}
        },
        "additionalProperties": false
    })
);
wire_schema!(
    /// `capsulet_kernel::Certificate`.
    KernelCertificate = "Certificate",
    [KernelVerdict, Proposition, DischargedStep, Residual, CertificateError],
    {
        let mut schema = all_required(&[
            ("verdict", reference("KernelVerdict")),
            ("goal", reference("Proposition")),
            ("discharged", json!({"type": "array", "items": reference("DischargedStep")})),
            ("residuals", json!({"type": "array", "items": reference("Residual")})),
            ("errors", json!({"type": "array", "items": reference("CertificateError")})),
            ("replay_digest", json!({"type": "string"})),
        ]);
        // Optional: the kernel leaves it out when the bound was not recorded,
        // which is not the same as today's bound.
        schema["properties"]["derivation_depth_limit"] = json!({
            "type": "integer",
            "minimum": 0,
            "description": "How deep a derivation the kernel would walk when it decided this. Absent when the bound was not recorded."
        });
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
);
wire_schema!(
    /// `capsulet_proposer::RawProposal`, what a model emitted for the kernel to check.
    RawProposal = "RawProposal",
    [],
    all_required(&[
        ("subject", json!({"type": "string"})),
        ("predicate", json!({"type": "string"})),
        ("object", json!({"type": "string"})),
        ("evidence_id", json!({"type": "string"})),
        ("quote", json!({"type": "string"})),
        ("needs_interpretation", json!({"type": "boolean"})),
        ("rationale", json!({"type": "string"})),
    ])
);

wire_schema!(
    /// The statuses of a job run, from `capsulet_core::JobRunStatus`.
    JobRunStatusSchema = "JobRunStatus",
    [],
    string_enum(&job_run_statuses())
);
wire_schema!(
    /// The statuses of a workflow, from `capsulet_core::WorkflowStatus`.
    WorkflowStatusSchema = "WorkflowStatus",
    [],
    string_enum(&workflow_statuses())
);
wire_schema!(
    /// The statuses of a workflow run, from `capsulet_core::WorkflowRunStatus`.
    WorkflowRunStatusSchema = "WorkflowRunStatus",
    [],
    string_enum(&workflow_run_statuses())
);
wire_schema!(
    /// The statuses of an automation, from `capsulet_core::AutomationStatus`.
    AutomationStatusSchema = "AutomationStatus",
    [],
    string_enum(&automation_statuses())
);

/// An object whose every listed property is required and nothing else is allowed.
fn all_required(properties: &[(&str, Value)]) -> Value {
    let required = properties.iter().map(|(name, _)| *name).collect::<Vec<_>>();
    let properties = properties
        .iter()
        .map(|(name, schema)| ((*name).to_owned(), schema.clone()))
        .collect::<serde_json::Map<_, _>>();
    json!({"type": "object", "required": required, "properties": properties, "additionalProperties": false})
}

fn string_enum(values: &[String]) -> Value {
    json!({"type": "string", "enum": values})
}

// Each list is checked against its enum by an exhaustive match, so a status
// added to the core domain fails to compile here until it is published too.

fn job_run_statuses() -> Vec<String> {
    use JobRunStatus as Status;
    [
        Status::Queued,
        Status::Leased,
        Status::Running,
        Status::Succeeded,
        Status::Failed,
        Status::Cancelled,
        Status::TimedOut,
        Status::RetryScheduled,
    ]
    .into_iter()
    .map(|status| match status {
        Status::Queued
        | Status::Leased
        | Status::Running
        | Status::Succeeded
        | Status::Failed
        | Status::Cancelled
        | Status::TimedOut
        | Status::RetryScheduled => status.to_string(),
    })
    .collect()
}

fn workflow_statuses() -> Vec<String> {
    use WorkflowStatus as Status;
    [Status::Draft, Status::Enabled, Status::Disabled]
        .into_iter()
        .map(|status| match status {
            Status::Draft | Status::Enabled | Status::Disabled => status.to_string(),
        })
        .collect()
}

fn workflow_run_statuses() -> Vec<String> {
    use WorkflowRunStatus as Status;
    [
        Status::Queued,
        Status::Running,
        Status::Removed,
        Status::Succeeded,
        Status::Failed,
        Status::Cancelled,
        Status::TimedOut,
        Status::Skipped,
    ]
    .into_iter()
    .map(|status| match status {
        Status::Queued
        | Status::Running
        | Status::Removed
        | Status::Succeeded
        | Status::Failed
        | Status::Cancelled
        | Status::TimedOut
        | Status::Skipped => status.to_string(),
    })
    .collect()
}

fn automation_statuses() -> Vec<String> {
    use AutomationStatus as Status;
    [Status::Enabled, Status::Disabled]
        .into_iter()
        .map(|status| match status {
            Status::Enabled | Status::Disabled => status.to_string(),
        })
        .collect()
}
