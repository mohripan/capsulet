//! What crosses the boundary between the host and a validator.
//!
//! One request in on standard input, one response out on standard output, both
//! JSON, both carrying the protocol version (ADR 0021 §4). Inputs travel as
//! files, named in the request with their digests, so a validator reads exactly
//! the bytes the host pinned. What a validator produces goes to an outputs
//! directory, and the host digests it: a validator never says what its own
//! outputs were.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The protocol this build speaks.
pub const PROTOCOL: &str = "capsulet.verifier/v1";

/// The largest response a host reads. A validator says what it decided and why;
/// anything bigger belongs in its outputs, where it is digested.
pub const MAX_RESPONSE_BYTES: usize = 256 * 1024;

/// What a host asks of a validator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyRequest {
    pub protocol: String,
    /// The obligations to decide, each once.
    pub obligations: Vec<RequestedObligation>,
    /// The validator's declared parameters, as its registration gives them.
    #[serde(default)]
    pub parameters: BTreeMap<String, String>,
    /// The files in the inputs directory, read-only.
    pub inputs: Vec<InputFile>,
}

/// One obligation a validator is asked to decide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestedObligation {
    pub id: String,
    pub contract: String,
    /// What must hold, in the contract's words.
    pub statement: String,
}

/// One file the host placed in the inputs directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputFile {
    /// Its name in the inputs directory.
    pub name: String,
    /// The digest of its bytes, which the host checks again after the run.
    pub digest: String,
    pub media_type: String,
}

/// What a validator concluded about one obligation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Holds,
    DoesNotHold,
    /// It ran and could not tell. Never read as either answer.
    CannotDecide,
}

/// One result, for one requested obligation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObligationResult {
    pub obligation: String,
    pub contract: String,
    pub outcome: Outcome,
    /// Why, in words a reviewer can act on.
    pub reason: String,
}

/// What a validator returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyResponse {
    pub protocol: String,
    pub results: Vec<ObligationResult>,
}
