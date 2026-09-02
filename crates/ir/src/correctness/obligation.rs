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

/// A proposition a contract requires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObligationStatement {
    pub id: Identifier,
    /// What must hold, in words a reviewer can act on.
    pub statement: String,
    /// Who has to act if it does not hold.
    pub owner: RepairOwner,
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
    },
    /// Nobody checked it; the run proceeded on the stated assumption.
    Assumed { rationale: String },
    /// Policy allowed release without it, on a named authority's decision.
    Waived {
        policy: Identifier,
        authority: Identifier,
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
            Self::Assumed { .. } => "assumed",
            Self::Waived { .. } => "waived",
            Self::Residual { .. } => "residual",
            Self::Failed { .. } => "failed",
        }
    }

    /// Whether this state was reached by checking rather than by deciding not
    /// to check.
    #[must_use]
    pub const fn was_checked(&self) -> bool {
        matches!(self, Self::Discharged { .. } | Self::Failed { .. })
    }

    /// The evidence this state rests on.
    #[must_use]
    pub fn evidence(&self) -> &[Digest] {
        match self {
            Self::Discharged { evidence, .. } | Self::Residual { evidence, .. } => evidence,
            Self::Assumed { .. } | Self::Waived { .. } | Self::Failed { .. } => &[],
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
        )
    }

    /// Whether this obligation failed.
    #[must_use]
    pub const fn has_failed(&self) -> bool {
        matches!(self.state, DischargeState::Failed { .. })
    }
}
