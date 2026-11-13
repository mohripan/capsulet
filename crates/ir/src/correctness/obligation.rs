//! Obligations and the contracts that state them.
//!
//! An obligation is a proposition someone has to answer for. The important
//! property of [`DischargeState`] is that it has no absent case: an obligation
//! is discharged, assumed, waived, left residual, or failed. There is no way to
//! represent one that was simply not looked at, so a certificate cannot quietly
//! omit the checks that did not happen — the surrounding assurance mode decides
//! which obligations are evaluated, and every one it lists ends up in a state
//! someone can read.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::digest::Digest;
use crate::effect::EffectKind;
use crate::id::Identifier;
use crate::value::ValueSchema;

/// Which subsystem has to act when an obligation is not met.
///
/// The same vocabulary the correctness kernel already routes failures by, so
/// there is one answer to "whose problem is this" rather than one per layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairOwner {
    /// Fixable mechanically from what is already recorded.
    AutoRepairable,
    Retrieval,
    Proposer,
    Verifier,
    Policy,
    Runtime,
    /// Needs a person: an interpretation, a judgement, an approval.
    Human,
}

impl RepairOwner {
    /// A short name for messages and certificates.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AutoRepairable => "auto_repairable",
            Self::Retrieval => "retrieval",
            Self::Proposer => "proposer",
            Self::Verifier => "verifier",
            Self::Policy => "policy",
            Self::Runtime => "runtime",
            Self::Human => "human",
        }
    }
}

/// A kind of checker that may discharge an obligation.
///
/// A model is not one. A model-backed verifier endorses or disputes an
/// obligation and leaves it outstanding (ADR 0021): one unreliable component
/// agreeing with another is not a check, and a certificate that let it count as
/// one would launder a score into a verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BasisKind {
    /// A deterministic family a replayer re-decides offline from pinned inputs.
    Mechanical,
    /// An external tool with no network and no model, in an environment pinned
    /// by digest: a compiler, a test runner, a policy engine. A replayer re-runs it.
    ReplayableTool,
    /// A named person, under the authority the obligation names.
    HumanReview,
}

impl BasisKind {
    /// A short name for messages and certificates.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mechanical => "mechanical",
            Self::ReplayableTool => "replayable_tool",
            Self::HumanReview => "human_review",
        }
    }
}

/// What kind of checker discharged an obligation, recorded beside who did.
///
/// "Discharged" from a recomputed sum and "discharged" from a person's judgement
/// are different claims, and a reader should not have to look the checker up to
/// tell them apart.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DischargeBasis {
    /// Discharged before bases were recorded. It reads as it always did, with
    /// the verifier record that discharged it, and is left out of the encoding
    /// so a certificate sealed then still verifies.
    #[default]
    Unrecorded,
    /// Re-decidable offline; the discharging verifier's record is deterministic.
    Mechanical,
    /// Re-runnable in a pinned environment; the record is a declared oracle.
    ReplayableTool,
    /// Decided by the person named as `by`, under `authority`, which must be the
    /// authority the obligation names.
    HumanReview { authority: Identifier },
}

impl DischargeBasis {
    /// Whether nothing was recorded, which is left out of the encoding.
    #[must_use]
    pub const fn is_unrecorded(&self) -> bool {
        matches!(self, Self::Unrecorded)
    }

    /// The kind of checker, when one was recorded.
    #[must_use]
    pub const fn kind(&self) -> Option<BasisKind> {
        match self {
            Self::Unrecorded => None,
            Self::Mechanical => Some(BasisKind::Mechanical),
            Self::ReplayableTool => Some(BasisKind::ReplayableTool),
            Self::HumanReview { .. } => Some(BasisKind::HumanReview),
        }
    }

    /// A short name for messages and certificates.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self.kind() {
            Some(kind) => kind.as_str(),
            None => "unrecorded",
        }
    }
}

/// A proposition a contract requires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObligationStatement {
    pub id: Identifier,
    /// What must hold, in words a reviewer can act on.
    pub statement: String,
    /// Who has to act if it does not hold.
    pub owner: RepairOwner,
    /// The authority a person must hold to discharge it by review. A review
    /// under any other authority, or of an obligation naming none, is refused
    /// at the seal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority: Option<Identifier>,
    /// The kinds of checker whose discharge counts. Empty accepts any, as every
    /// obligation did before bases were recorded. A certificate discharging it
    /// any other way is refused at the seal: whoever assembles it records the
    /// obligation as residual instead, saying why the discharge did not count.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepts: Vec<BasisKind>,
    /// The authority a person must hold to waive it. A waiver under any other
    /// authority is refused at the seal. One naming none is waived, as before,
    /// under whichever authorities the assurance policy allows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiver_authority: Option<Identifier>,
}

impl ObligationStatement {
    /// A statement that names no review authority and accepts any checker.
    #[must_use]
    pub fn new(id: Identifier, statement: impl Into<String>, owner: RepairOwner) -> Self {
        Self {
            id,
            statement: statement.into(),
            owner,
            authority: None,
            accepts: Vec::new(),
            waiver_authority: None,
        }
    }

    /// Whether a discharge on `basis` counts for this obligation.
    #[must_use]
    pub fn accepts_basis(&self, basis: &DischargeBasis) -> bool {
        self.accepts.is_empty()
            || basis
                .kind()
                .is_some_and(|kind| self.accepts.contains(&kind))
    }
}

/// What a contract requires and promises.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contract {
    pub id: Identifier,
    pub version: String,
    /// What the contract requires of its inputs.
    pub inputs: BTreeMap<String, ValueSchema>,
    /// What it promises about its outputs.
    pub outputs: BTreeMap<String, ValueSchema>,
    /// The effects it permits. An effect kind absent from this list is not
    /// permitted under this contract, whatever the node declares.
    pub allowed_effects: Vec<EffectKind>,
    pub obligations: Vec<ObligationStatement>,
}

impl Contract {
    /// The statement behind an obligation identifier.
    #[must_use]
    pub fn statement(&self, id: &Identifier) -> Option<&ObligationStatement> {
        self.obligations
            .iter()
            .find(|obligation| &obligation.id == id)
    }
}

/// How an obligation came out.
///
/// Every case is a claim someone can be held to. `Assumed` and `Waived` are
/// deliberately separate: an assumption is something the run proceeded on
/// without checking, a waiver is a policy decision by a named authority, and
/// collapsing them would hide who decided.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DischargeState {
    /// A checker decided it, and here is what it decided from.
    Discharged {
        by: Identifier,
        evidence: Vec<Digest>,
        /// What kind of checker `by` is.
        #[serde(default, skip_serializing_if = "DischargeBasis::is_unrecorded")]
        basis: DischargeBasis,
    },
    /// A model-backed verifier, `by`, agreed that it holds. Still outstanding:
    /// an endorsement orders a review queue and is measured against reviews, and
    /// it never certifies.
    Endorsed {
        by: Identifier,
        evidence: Vec<Digest>,
    },
    /// A model-backed verifier, `by`, said it does not hold. Still outstanding
    /// rather than failed, because an unreliable checker's "no" is a reason to
    /// look first, not a refutation. A boundary may refuse to cross over one.
    Disputed {
        by: Identifier,
        reason: String,
        evidence: Vec<Digest>,
    },
    /// Nobody checked it; the run proceeded on the stated assumption.
    Assumed { rationale: String },
    /// Policy allowed release without it, on a named authority's decision.
    /// `policy` names the decision: the waiver granted, or the policy that
    /// grants it.
    Waived {
        policy: Identifier,
        authority: Identifier,
        /// Who waived it, under `authority`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        by: Option<Identifier>,
        /// The waiver as it was granted, which the certificate carries: who,
        /// why, and the failure it accepted.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        evidence: Vec<Digest>,
    },
    /// Still open, with whatever partial evidence exists.
    Residual {
        rationale: String,
        evidence: Vec<Digest>,
    },
    /// Checked, and it did not hold.
    Failed { reason: String, owner: RepairOwner },
}

impl DischargeState {
    /// A short name for messages and certificates.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Discharged { .. } => "discharged",
            Self::Endorsed { .. } => "endorsed",
            Self::Disputed { .. } => "disputed",
            Self::Assumed { .. } => "assumed",
            Self::Waived { .. } => "waived",
            Self::Residual { .. } => "residual",
            Self::Failed { .. } => "failed",
        }
    }

    /// Whether this state was reached by checking rather than by deciding not
    /// to check. An endorsement or a dispute was looked at, by a model, and
    /// decided nothing, so neither counts.
    #[must_use]
    pub const fn was_checked(&self) -> bool {
        matches!(self, Self::Discharged { .. } | Self::Failed { .. })
    }

    /// The evidence this state rests on.
    #[must_use]
    pub fn evidence(&self) -> &[Digest] {
        match self {
            Self::Discharged { evidence, .. }
            | Self::Endorsed { evidence, .. }
            | Self::Disputed { evidence, .. }
            | Self::Waived { evidence, .. }
            | Self::Residual { evidence, .. } => evidence,
            Self::Assumed { .. } | Self::Failed { .. } => &[],
        }
    }
}

/// One obligation, and what became of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Obligation {
    pub statement: ObligationStatement,
    pub contract: Identifier,
    pub state: DischargeState,
}

impl Obligation {
    /// Whether this obligation still stands between the result and a clean
    /// verdict.
    #[must_use]
    pub const fn is_outstanding(&self) -> bool {
        matches!(
            self.state,
            DischargeState::Assumed { .. }
                | DischargeState::Waived { .. }
                | DischargeState::Residual { .. }
                | DischargeState::Endorsed { .. }
                | DischargeState::Disputed { .. }
        )
    }

    /// Whether a model-backed verifier disputed it.
    #[must_use]
    pub const fn is_disputed(&self) -> bool {
        matches!(self.state, DischargeState::Disputed { .. })
    }

    /// Whether this obligation failed.
    #[must_use]
    pub const fn has_failed(&self) -> bool {
        matches!(self.state, DischargeState::Failed { .. })
    }
}
