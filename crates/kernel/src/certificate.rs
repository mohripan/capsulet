//! What the kernel emits: a verdict, what it discharged, and what it did not.

use serde::{Deserialize, Serialize};

use crate::{error::RepairOwner, ir::Proposition};

/// The three-valued outcome.
///
/// A boolean would force interpretation to be either silently accepted or
/// treated as failure. Neither is honest, so it gets its own value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Every step discharged mechanically. No interpretation was required.
    Accepted,
    /// Sound given the recorded interpretation obligations.
    Conditional,
    /// A premise failed.
    Rejected,
}

impl Verdict {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Conditional => "conditional",
            Self::Rejected => "rejected",
        }
    }
}

/// One mechanically discharged step, recorded so the certificate is auditable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DischargedStep {
    pub rule: String,
    pub concluded: String,
    pub detail: String,
}

/// An interpretation the kernel could not make, pinned to where it was needed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Residual {
    /// The attributed content the reading starts from.
    pub from: String,
    /// The proposition the proposer read out of it.
    pub to: Proposition,
    /// The proposer's stated reason, retained for whoever discharges this.
    pub rationale: String,
    /// Evidence the reading depends on, so a reviewer can go straight to it.
    pub evidence_ids: Vec<String>,
}

/// A recorded failure, with its routing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CertificateError {
    pub code: String,
    pub message: String,
    pub repair_owner: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corrected_value: Option<f64>,
}

/// The kernel's decision on one proposal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Certificate {
    pub verdict: Verdict,
    pub goal: Proposition,
    pub discharged: Vec<DischargedStep>,
    pub residuals: Vec<Residual>,
    pub errors: Vec<CertificateError>,
    /// Digest over the goal and the derivation, so a certificate can be tied
    /// back to the exact proposal that produced it.
    pub replay_digest: String,
}

impl Certificate {
    #[must_use]
    pub const fn is_accepted(&self) -> bool {
        matches!(self.verdict, Verdict::Accepted)
    }

    #[must_use]
    pub const fn is_rejected(&self) -> bool {
        matches!(self.verdict, Verdict::Rejected)
    }

    /// Subsystems that need to act before this proposal could succeed.
    #[must_use]
    pub fn repair_owners(&self) -> Vec<&str> {
        let mut owners: Vec<&str> = self
            .errors
            .iter()
            .map(|error| error.repair_owner.as_str())
            .collect();
        owners.sort_unstable();
        owners.dedup();
        owners
    }

    /// Whether every failure can be fixed without calling a model.
    #[must_use]
    pub fn is_auto_repairable(&self) -> bool {
        !self.errors.is_empty()
            && self
                .errors
                .iter()
                .all(|error| error.repair_owner == RepairOwner::AutoRepairable.as_str())
    }
}
