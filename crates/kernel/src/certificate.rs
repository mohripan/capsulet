//! What the kernel emits: a verdict, what it discharged, and what it did not.

use serde::{Deserialize, Serialize};

use crate::{error::RepairOwner, ir::Proposition, quantity::Quantity};

/// The three-valued outcome.
///
/// A boolean would force interpretation to be either silently accepted or
/// treated as failure. Neither is honest, so it gets its own value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema), schema(as = KernelVerdict))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DischargedStep {
    pub rule: String,
    pub concluded: String,
    pub detail: String,
}

/// What a residual is a reading of.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ResidualKind {
    /// What a cited source means. Every residual was this before questions were.
    #[default]
    Source,
    /// What the question an answer was asked asks for.
    Question,
}

impl ResidualKind {
    /// Whether this is the default, which is left out so older certificates
    /// encode byte for byte as they did.
    #[must_use]
    pub const fn is_source(&self) -> bool {
        matches!(self, Self::Source)
    }
}

/// An interpretation the kernel could not make, pinned to where it was needed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Residual {
    /// What was read: a source, or the question an answer was asked.
    #[serde(default, skip_serializing_if = "ResidualKind::is_source")]
    pub kind: ResidualKind,
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CertificateError {
    pub code: String,
    pub message: String,
    pub repair_owner: String,
    /// The value the kernel computed, when it knows the right answer.
    ///
    /// A decimal string, never a JSON number: this travels to clients and into
    /// digests, and a number would invite a float back into a pipeline that
    /// refuses them.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>, example = "46"))]
    pub corrected_value: Option<Quantity>,
}

/// The kernel's decision on one proposal.
///
/// `unverified` is not a kernel verdict. It belongs to the platform assurance
/// layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "openapi",
    derive(utoipa::ToSchema),
    schema(examples(
        json!({
            "verdict": "accepted",
            "goal": {"subject": "document", "predicate": "states", "object": "fact"},
            "discharged": [{"rule": "cite", "concluded": "fact", "detail": "literal evidence matched"}],
            "residuals": [], "errors": [], "replay_digest": "sha256:accepted"
        }),
        json!({
            "verdict": "conditional",
            "goal": {"subject": "document", "predicate": "implies", "object": "fact"},
            "discharged": [],
            "residuals": [{
                "from": "attributed text",
                "to": {"subject": "document", "predicate": "implies", "object": "fact"},
                "rationale": "interpretation required", "evidence_ids": ["evidence_1"]
            }],
            "errors": [], "replay_digest": "sha256:conditional"
        }),
        json!({
            "verdict": "rejected",
            "goal": {"subject": "document", "predicate": "states", "object": "missing"},
            "discharged": [], "residuals": [],
            "errors": [{"code": "quote_mismatch", "message": "quote was not found", "repair_owner": "proposer"}],
            "replay_digest": "sha256:rejected"
        })
    ))
)]
pub struct Certificate {
    pub verdict: Verdict,
    pub goal: Proposition,
    pub discharged: Vec<DischargedStep>,
    pub residuals: Vec<Residual>,
    pub errors: Vec<CertificateError>,
    /// Digest over the goal and the derivation, so a certificate can be tied
    /// back to the exact proposal that produced it.
    pub replay_digest: String,
    /// How deep a derivation the kernel would walk when it decided this.
    ///
    /// Recorded rather than looked up from the kernel version, because a build
    /// with a tighter bound may reject a proposal this one accepted, and a
    /// reader comparing two certificates should be able to see that without
    /// knowing which constant each build carried.
    ///
    /// Absent when the bound was not recorded: a certificate decided before
    /// this field existed, or reloaded from a store that does not keep it. That
    /// is deliberately not the same as today's bound: filling in the running
    /// build's constant would claim something about a decision this build did
    /// not make.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derivation_depth_limit: Option<u32>,
    /// The authority a person must decide this certificate's readings under,
    /// such as `review:legal`, when the contract its answer was read under
    /// names one. Absent, they are decided under `review:write`, as every
    /// reading was before a contract could name its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(
        feature = "openapi",
        schema(pattern = "^[a-z][a-z0-9_-]*:[a-z][a-z0-9_-]*$")
    )]
    pub reading_authority: Option<String>,
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
