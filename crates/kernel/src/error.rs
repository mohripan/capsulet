//! Why a proposal failed, and who should fix it.
//!
//! Every variant names the subsystem that owns the repair. That is the point:
//! a rejection is a routing decision, not a signal to re-prompt the model. Some
//! variants are repairable with no model call at all.

use capsulet_core::ProvenanceError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The subsystem that should act on a rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairOwner {
    /// The referenced record was not in the snapshot. Re-run retrieval; spend
    /// no model tokens.
    Retrieval,
    /// The source text backing a citation is missing or has changed. Re-ingest.
    Ingestion,
    /// The memory record exists but is not in a citable state. A review or
    /// promotion decision, not a generation problem.
    Memory,
    /// The kernel computed the answer itself, so the correct value is already
    /// known and the proposal can be repaired without a model call.
    AutoRepairable,
    /// Policy refused the step. An operator decision.
    Policy,
    /// The proposer genuinely got it wrong. This is the only owner for which
    /// re-generating is the right response.
    Proposer,
}

impl RepairOwner {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Retrieval => "retrieval",
            Self::Ingestion => "ingestion",
            Self::Memory => "memory",
            Self::AutoRepairable => "auto_repairable",
            Self::Policy => "policy",
            Self::Proposer => "proposer",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum CheckError {
    #[error("evidence {evidence_id} is not in the snapshot")]
    DanglingEvidence { evidence_id: String },
    #[error("claim {claim_id} is not in the snapshot")]
    DanglingClaim { claim_id: String },
    #[error("source {source_id} is not in the snapshot")]
    DanglingSource { source_id: String },
    #[error("stored text for source {source_id} at digest {content_hash} is not in the snapshot")]
    SourceContentMissing {
        source_id: String,
        content_hash: String,
    },
    #[error("citation for evidence {evidence_id} does not re-derive: {source}")]
    Provenance {
        evidence_id: String,
        #[source]
        source: ProvenanceError,
    },
    #[error(
        "the cited span for evidence {evidence_id} does not contain the {role} {term:?}; it reads {excerpt:?}"
    )]
    TermNotInSpan {
        evidence_id: String,
        role: &'static str,
        term: String,
        excerpt: String,
    },
    #[error("claim {claim_id} is {status}, so it cannot be attested")]
    ClaimNotActive { claim_id: String, status: String },
    #[error("{op} of {operands:?} is {computed}, not the claimed {claimed}")]
    ArithMismatch {
        op: &'static str,
        operands: Vec<f64>,
        claimed: f64,
        computed: f64,
    },
    #[error("{op} has no operands to compute")]
    ArithNoOperands { op: &'static str },
    #[error("source {source_id} has authority {actual}, below the required {required}")]
    AuthorityBelowFloor {
        source_id: String,
        actual: String,
        required: String,
    },
    #[error("{value:?} is not a recognised authority level")]
    UnknownAuthority { value: String },
    #[error("trust requires attributed content, but its premise concludes {found}")]
    TrustPremiseNotAttributed { found: String },
    #[error("the derivation concludes {derived}, which is not the stated goal {goal}")]
    GoalNotDerived { derived: String, goal: String },
}

impl CheckError {
    /// Which subsystem should act on this failure.
    #[must_use]
    pub const fn repair_owner(&self) -> RepairOwner {
        match self {
            Self::DanglingEvidence { .. }
            | Self::DanglingClaim { .. }
            | Self::DanglingSource { .. } => RepairOwner::Retrieval,
            Self::SourceContentMissing { .. } | Self::Provenance { .. } => RepairOwner::Ingestion,
            Self::ClaimNotActive { .. } => RepairOwner::Memory,
            Self::ArithMismatch { .. } => RepairOwner::AutoRepairable,
            Self::AuthorityBelowFloor { .. } => RepairOwner::Policy,
            Self::TermNotInSpan { .. }
            | Self::ArithNoOperands { .. }
            | Self::UnknownAuthority { .. }
            | Self::TrustPremiseNotAttributed { .. }
            | Self::GoalNotDerived { .. } => RepairOwner::Proposer,
        }
    }

    /// Short machine-readable code, for clients that route on it.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::DanglingEvidence { .. } => "dangling_evidence",
            Self::DanglingClaim { .. } => "dangling_claim",
            Self::DanglingSource { .. } => "dangling_source",
            Self::SourceContentMissing { .. } => "source_content_missing",
            Self::Provenance { .. } => "provenance_failed",
            Self::TermNotInSpan { .. } => "term_not_in_span",
            Self::ClaimNotActive { .. } => "claim_not_active",
            Self::ArithMismatch { .. } => "arith_mismatch",
            Self::ArithNoOperands { .. } => "arith_no_operands",
            Self::AuthorityBelowFloor { .. } => "authority_below_floor",
            Self::UnknownAuthority { .. } => "unknown_authority",
            Self::TrustPremiseNotAttributed { .. } => "trust_premise_not_attributed",
            Self::GoalNotDerived { .. } => "goal_not_derived",
        }
    }

    /// The value the kernel computed, when it knows the right answer.
    ///
    /// Present only for [`RepairOwner::AutoRepairable`] failures, which is what
    /// makes those repairable without calling a model.
    #[must_use]
    pub fn corrected_value(&self) -> Option<f64> {
        match self {
            Self::ArithMismatch { computed, .. } => Some(*computed),
            _ => None,
        }
    }
}
