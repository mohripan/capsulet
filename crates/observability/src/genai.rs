//! The OpenTelemetry semantic conventions for generative AI, pinned to one revision.
//!
//! The conventions are still marked as in development and have renamed attributes
//! between releases (`gen_ai.system` became `gen_ai.provider.name` in 1.37.0), so
//! Capsulet names the revision it follows instead of tracking whatever is newest.
//! Spans keep the attribute names of this revision until somebody moves the pin, and
//! moving it is a change to `docs/operations/observability.md` in the same commit.
//!
//! 1.41.0 is the last release of the main semantic-conventions repository that
//! carries the generative-AI pages; from 1.42.0 they moved to a repository of their own.

/// The semantic-conventions release that `gen_ai` spans and metrics follow.
pub const SEMCONV_VERSION: &str = "1.41.0";

/// The resource attribute that says so, on every exported span.
pub const SEMCONV_RESOURCE_ATTRIBUTE: &str = "capsulet.telemetry.genai_semconv";

/// `gen_ai.operation.name` for a call to a completion endpoint.
pub const TEXT_COMPLETION: &str = "text_completion";
/// `gen_ai.operation.name` for a call to an embedding endpoint.
pub const EMBEDDINGS: &str = "embeddings";
/// `gen_ai.operation.name` for a lookup in a data source.
pub const RETRIEVAL: &str = "retrieval";
/// `gen_ai.operation.name` for something an agent does to the world.
pub const EXECUTE_TOOL: &str = "execute_tool";
