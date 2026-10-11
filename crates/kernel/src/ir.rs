//! The intermediate language a proposer emits and the kernel decides.
//!
//! Nothing here is free text a model can invent its way through. A term is
//! either a reference the kernel resolves against a snapshot or a literal it
//! can check; a derivation is a tree of rule applications with fixed arity.

use capsulet_ir::version::{SchemaVersion, SchemaVersionError, read_compatible};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::quantity::Quantity;

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

    /// A rendering for people: certificates, residuals and errors show it.
    ///
    /// No decision reads it. The goal check compares fields, and the replay
    /// digest is taken over canonical bytes. It is still unambiguous — a `|` or
    /// `\` inside a term is escaped — so a reader shown two renderings in an
    /// error is never shown the same string for two different propositions.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "{}|{}|{}",
            escape_term(self.subject.trim()),
            escape_term(self.predicate.trim()),
            escape_term(self.object.trim())
        )
    }
}

/// Escapes the separator and the escape character, so a rendering names one
/// proposition. Text without either comes back unchanged.
fn escape_term(term: &str) -> String {
    term.replace('\\', "\\\\").replace('|', "\\|")
}

/// What a derivation concludes.
///
/// `Says` is attributed content — a source states it, in a phrasing its
/// contract declares. `Mentions` is weaker: the source contains the
/// proposition's subject and object, and states the relation between them in
/// no declared phrasing. `Holds` is the system asserting it. [`Rule::Trust`]
/// crosses from `Says` to `Holds` and is discharged; [`Rule::Interpret`]
/// crosses from anything and is not. Nothing crosses from `Mentions` to
/// `Holds` without a reading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Judgment {
    Says {
        source_id: String,
        proposition: Proposition,
    },
    Mentions {
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
            Self::Says { proposition, .. }
            | Self::Mentions { proposition, .. }
            | Self::Holds { proposition } => proposition,
        }
    }

    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Says {
                source_id,
                proposition,
            } => format!("says({source_id},{})", proposition.canonical()),
            Self::Mentions {
                source_id,
                proposition,
            } => format!("mentions({source_id},{})", proposition.canonical()),
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
    /// The exact result, or `None` when there is not one.
    ///
    /// `None` covers both an empty operand list and a result that would not fit
    /// exactly. Neither has an honest answer, and returning an approximate one
    /// is what this type exists to stop.
    #[must_use]
    pub fn apply(self, operands: &[Quantity]) -> Option<Quantity> {
        let (first, rest) = operands.split_first()?;
        let mut accumulated = *first;
        for operand in rest {
            accumulated = match self {
                Self::Sum => accumulated.checked_add(*operand)?,
                Self::Difference => accumulated.checked_sub(*operand)?,
                Self::Product => accumulated.checked_mul(*operand)?,
                Self::Min | Self::Max => {
                    let ordering = accumulated.partial_cmp_exact(*operand)?;
                    let take_operand = match self {
                        Self::Min => ordering.is_gt(),
                        _ => ordering.is_lt(),
                    };
                    if take_operand { *operand } else { accumulated }
                }
            };
        }
        Some(accumulated)
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
    /// The kernel checks that the evidence exists, that its span re-derives
    /// from the stored source bytes, and that the proposition's subject and
    /// object both occur within the cited excerpt as whole words. It concludes
    /// `Says(source, P)` when, in addition, one sentence of the excerpt states
    /// the predicate in a phrasing the snapshot declares for it — see
    /// [`crate::phrasing`] — and `Mentions(source, P)` when none does.
    Cite {
        evidence_id: String,
        proposition: Proposition,
    },
    /// Lifts an already-active claim out of memory. Concludes `Holds(P)`.
    Attest { claim_id: String },
    /// Turns attributed content into an assertion, and the only rule that can
    /// do so *and be discharged*.
    ///
    /// Concludes `Holds(P)` from `Says(s, P)` once the source clears the
    /// configured authority floor. A premise that only `Mentions` the terms is
    /// refused: authority makes a statement believable, and there is no
    /// statement to believe. [`Rule::Interpret`] reaches the same shape of
    /// conclusion, but never discharges: it records a residual, and a
    /// certificate carrying one is `conditional`. Authority says a source is
    /// worth believing; it does not say that a particular reading of it is
    /// right, which is why clearing the floor is not something interpretation
    /// can borrow.
    Trust {
        premise: Box<Rule>,
        min_authority: String,
    },
    /// Recomputes a numeric result rather than trusting the proposer's value.
    Arith {
        op: ArithOp,
        operands: Vec<Quantity>,
        claimed: Quantity,
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
    /// Always written; a versionless legacy document is read as v1.
    #[serde(default = "Proposal::current_schema_version")]
    pub schema_version: SchemaVersion,
    pub goal: Proposition,
    pub derivation: Rule,
}

/// The schema identity of a persisted claim-reasoning proposal.
pub const PROPOSAL_SCHEMA_VERSION: SchemaVersion =
    SchemaVersion::new("capsulet.kernel.proposal", 1);

/// Why a persisted proposal cannot be read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProposalReadError {
    #[error("the proposal schema is unreadable: {source}")]
    SchemaVersion {
        #[source]
        source: SchemaVersionError,
    },
    #[error("the proposal is malformed: {detail}")]
    Malformed { detail: String },
}

impl Proposal {
    /// The schema version this build writes.
    #[must_use]
    pub fn current_schema_version() -> SchemaVersion {
        PROPOSAL_SCHEMA_VERSION
    }

    /// Reads the declared schema before interpreting the proposal's shape.
    /// Versionless documents are legacy v1, not an unknown schema.
    ///
    /// # Errors
    /// Returns [`ProposalReadError::SchemaVersion`] for incompatible schemas,
    /// or [`ProposalReadError::Malformed`] for invalid JSON or a malformed v1 shape.
    pub fn read(bytes: &[u8]) -> Result<Self, ProposalReadError> {
        #[derive(Deserialize)]
        struct Header {
            #[serde(default = "legacy_version")]
            schema_version: String,
        }
        fn legacy_version() -> String {
            PROPOSAL_SCHEMA_VERSION.to_string()
        }
        let malformed = |error: serde_json::Error| ProposalReadError::Malformed {
            detail: error.to_string(),
        };
        let header: Header = serde_json::from_slice(bytes).map_err(malformed)?;
        read_compatible(&header.schema_version, &PROPOSAL_SCHEMA_VERSION)
            .map_err(|source| ProposalReadError::SchemaVersion { source })?;
        serde_json::from_slice(bytes).map_err(malformed)
    }
}
