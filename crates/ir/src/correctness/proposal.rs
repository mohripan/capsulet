//! Proposals: candidates, and nothing more.
//!
//! The type has no method that promotes a proposal to an accepted value. That
//! is deliberate and it is the whole design: acceptance happens somewhere else,
//! by something that checked. A proposer can only ever hand over what it
//! produced, what it produced it from, and what it claims justifies it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::correctness::Identity;
use crate::digest::Digest;
use crate::id::Identifier;

/// What kind of thing proposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProducerKind {
    Model,
    Tool,
    Retrieval,
    Human,
    /// A deterministic computation. Still a proposal: being deterministic makes
    /// it reproducible, not correct.
    Deterministic,
}

/// The thing that proposed, identified exactly.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Producer {
    pub kind: ProducerKind,
    pub identity: Identity,
}

/// A candidate value, with what it was derived from and what is claimed to
/// justify it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    pub id: Identifier,
    /// The node that produced it.
    pub node: Identifier,
    pub producer: Producer,
    /// The inputs it was produced from, pinned by digest. "The same prompt"
    /// means nothing unless the inputs are the same bytes.
    pub inputs: BTreeMap<Identifier, Digest>,
    /// The candidate value, by digest.
    pub candidate: Digest,
    /// A derivation record, where the producer supplied one.
    pub derivation: Option<Digest>,
    /// Evidence the producer claims justifies the candidate.
    ///
    /// A claim, not a finding: a checker decides whether this evidence exists,
    /// says what the producer thinks it says, and covers the obligation.
    pub claims_evidence: Vec<Digest>,
}
