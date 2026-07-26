//! The intermediate language a proposer emits and the kernel decides.
//!
//! Nothing here is free text a model can invent its way through. A term is
//! either a reference the kernel resolves against a snapshot or a literal it
//! can check; a derivation is a tree of rule applications with fixed arity.

use serde::{Deserialize, Serialize};

/// A statement about the world, in subject-predicate-object form.
///
/// Kept deliberately flat: the kernel's job is to decide whether a proposition
/// is grounded, not to model arbitrary logic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposition {
    pub subject: String,
    pub predicate: String,
    pub object: String,
}

impl Proposition {
    #[must_use]
    pub fn new(
        subject: impl Into<String>,
        predicate: impl Into<String>,
        object: impl Into<String>,
    ) -> Self {
        Self {
            subject: subject.into(),
            predicate: predicate.into(),
            object: object.into(),
        }
    }

    /// Stable rendering used for goal comparison and the replay digest.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "{}|{}|{}",
            self.subject.trim(),
            self.predicate.trim(),
            self.object.trim()
        )
    }
}

/// What a derivation concludes.
///
/// `Says` is attributed content — a source asserted it. `Holds` is the system
/// asserting it. Crossing from one to the other is exactly what [`Rule::Trust`]
/// does, and it is the only rule that can.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Judgment {
    Says {
        source_id: String,
        proposition: Proposition,
    },
    Holds {
        proposition: Proposition,
    },
}

impl Judgment {
    #[must_use]
    pub const fn proposition(&self) -> &Proposition {
        match self {
            Self::Says { proposition, .. } | Self::Holds { proposition } => proposition,
        }
    }

    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Says {
                source_id,
                proposition,
            } => format!("says({source_id},{})", proposition.canonical()),
            Self::Holds { proposition } => format!("holds({})", proposition.canonical()),
        }
    }
}

/// Arithmetic the kernel recomputes rather than trusting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArithOp {
    Sum,
    Difference,
    Product,
    Min,
    Max,
}

impl ArithOp {
    #[must_use]
    pub fn apply(self, operands: &[f64]) -> Option<f64> {
        if operands.is_empty() {
            return None;
        }
        Some(match self {
            Self::Sum => operands.iter().sum(),
            Self::Difference => operands[1..]
                .iter()
                .fold(operands[0], |acc, value| acc - value),
            Self::Product => operands.iter().product(),
            Self::Min => operands.iter().copied().fold(f64::INFINITY, f64::min),
            Self::Max => operands.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        })
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sum => "sum",
            Self::Difference => "difference",
            Self::Product => "product",
            Self::Min => "min",
            Self::Max => "max",
        }
    }
}

/// The closed rule set. A proposer may not extend it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "rule", rename_all = "snake_case")]
pub enum Rule {
    /// Introduces attributed content from a cited span.
    ///
    /// Concludes `Says(source, P)`. The kernel checks that the evidence exists,
    /// that its span re-derives from the stored source bytes, and that the
    /// proposition's object appears literally within the cited excerpt.
    Cite {
        evidence_id: String,
        proposition: Proposition,
    },
    /// Lifts an already-active claim out of memory. Concludes `Holds(P)`.
    Attest { claim_id: String },
    /// Turns attributed content into an assertion. The only rule that can.
    ///
    /// Concludes `Holds(P)` from `Says(s, P)` once the source clears the
    /// configured authority floor.
    Trust {
        premise: Box<Rule>,
        min_authority: String,
    },
    /// Recomputes a numeric result rather than trusting the proposer's value.
    Arith {
        op: ArithOp,
        operands: Vec<f64>,
        claimed: f64,
        proposition: Proposition,
    },
    /// The step no kernel can take.
    ///
    /// Concludes whatever it is asked to, discharges nothing, and records a
    /// residual obligation on the certificate. Its presence is what makes a
    /// verdict conditional rather than accepted.
    Interpret {
        premise: Box<Rule>,
        proposition: Proposition,
        rationale: String,
    },
}

impl Rule {
    /// Name used in errors and the replay digest.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Cite { .. } => "cite",
            Self::Attest { .. } => "attest",
            Self::Trust { .. } => "trust",
            Self::Arith { .. } => "arith",
            Self::Interpret { .. } => "interpret",
        }
    }
}

/// A proposal: the goal a proposer claims, and the derivation offered for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    pub goal: Proposition,
    pub derivation: Rule,
}
