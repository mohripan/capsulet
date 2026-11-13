//! Schemas for wire shapes that have no serde type of their own in this crate.
//!
//! Every other component of the `OpenAPI` document is derived from the Rust type a
//! handler serializes or deserializes, and endpoint contracts name those types,
//! not strings. What is left here has no such type to derive from: bodies that are
//! not JSON objects (text, binary, an event stream, the document itself), a webhook
//! payload that is arbitrary by design, and the status vocabularies of the core
//! domain, which carries no serde derives. Each is still a type, so an endpoint
//! names it the same way and a misspelling is a compile error, not an empty schema.
//!
//! The kernel's certificate and the proposer's raw proposal derive theirs, behind
//! those crates' `openapi` feature, which only this crate turns on.

use std::borrow::Cow;

use capsulet_core::{AutomationStatus, JobRunStatus, WorkflowRunStatus, WorkflowStatus};
use serde_json::{Value, json};
use utoipa::openapi::{RefOr, Schema};
use utoipa::{PartialSchema, ToSchema};

/// A schema read from JSON, which is how these are clearest to write down.
fn parse(value: Value) -> RefOr<Schema> {
    serde_json::from_value(value).expect("hand-written wire schemas are valid OpenAPI schemas")
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
