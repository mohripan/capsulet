//! Trust classes: what a value's assurance is, as a type.
//!
//! The rule this module exists to enforce is short. Trust never strengthens by
//! assertion. Not by a cast, not by a setter, not by a field in a JSON document
//! someone posted, and not because a model said the output looked right. The
//! only way to reach [`TrustClass::Verified`] is to present a
//! [`VerificationRecord`] that justifies it, and the only way to build one of
//! those is [`VerificationRecord::admit`], which checks the claim it carries.
//!
//! Weakening, by contrast, is always allowed. A value may be treated as less
//! assured than it is; that is a conservative mistake, not an unsound one.
//!
//! Deserialization is the interesting boundary, because a wire document is
//! whatever the sender wrote. [`RawTrustClass`] is the wire shape, and it is
//! plain data with no privileges. It becomes a [`TrustClass`] only by passing
//! the same admission every other path uses.

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

use crate::digest::Digest;

/// Why a trust claim was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TrustError {
    #[error("a verification record must name the contract it discharged")]
    MissingContract,
    #[error(
        "a `{claimed}` trust class needs a record that justifies it, but this record justifies at \
         most `{justified}`"
    )]
    Unjustified {
        claimed: &'static str,
        justified: &'static str,
    },
    #[error("verdict `{found}` cannot support any strengthened trust class")]
    VerdictTooWeak { found: String },
}

/// What a verifier concluded, as it appears inside a verification record.
///
/// This mirrors the platform verdict rather than redefining it; the record only
/// needs to know whether the conclusion can carry trust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordVerdict {
    Unverified,
    Rejected,
    Conditional,
    Accepted,
}

impl RecordVerdict {
    #[must_use]
    const fn as_str(self) -> &'static str {
        match self {
            Self::Unverified => "unverified",
            Self::Rejected => "rejected",
            Self::Conditional => "conditional",
            Self::Accepted => "accepted",
        }
    }
}

/// The wire shape of a verification record.
///
/// Plain data with no authority. It has to be admitted before it means anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawVerificationRecord {
    pub contract: String,
    pub certificate: Digest,
    pub verdict: RecordVerdict,
    pub residual_count: u32,
    pub provenance_complete: bool,
}

/// An admitted statement that a specific certificate discharged a specific
/// contract.
///
/// Fields are private and there is no `Deserialize`: a record cannot be spoken
/// into existence, only admitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerificationRecord {
    contract: String,
    certificate: Digest,
    verdict: RecordVerdict,
    residual_count: u32,
    provenance_complete: bool,
}

impl VerificationRecord {
    /// Admits a raw record.
    ///
    /// # Errors
    ///
    /// Returns [`TrustError::MissingContract`] when no contract is named.
    pub fn admit(raw: RawVerificationRecord) -> Result<Self, TrustError> {
        if raw.contract.trim().is_empty() {
            return Err(TrustError::MissingContract);
        }
        Ok(Self {
            contract: raw.contract,
            certificate: raw.certificate,
            verdict: raw.verdict,
            residual_count: raw.residual_count,
            provenance_complete: raw.provenance_complete,
        })
    }

    /// The contract this record discharged.
    #[must_use]
    pub fn contract(&self) -> &str {
        &self.contract
    }

    /// The certificate that carries the decision.
    #[must_use]
    pub const fn certificate(&self) -> &Digest {
        &self.certificate
    }

    /// The verdict recorded.
    #[must_use]
    pub const fn verdict(&self) -> RecordVerdict {
        self.verdict
    }

    /// The strongest trust class this record justifies.
    ///
    /// `Verified` requires everything to have gone right: an accepted verdict,
    /// no residual obligations, and complete provenance. Anything less is
    /// `Conditional` at best, and a rejected or unevaluated result carries no
    /// trust at all.
    #[must_use]
    pub fn justifies(&self) -> TrustClass {
        match self.verdict {
            RecordVerdict::Accepted if self.residual_count == 0 && self.provenance_complete => {
                TrustClass::Verified {
                    record: Box::new(self.clone()),
                }
            }
            RecordVerdict::Accepted | RecordVerdict::Conditional => TrustClass::Conditional {
                record: Box::new(self.clone()),
            },
            RecordVerdict::Rejected | RecordVerdict::Unverified => TrustClass::Unverified,
        }
    }
}

/// The assurance attached to a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrustClass {
    /// Nothing checked this value, or what checked it failed.
    Unverified,
    /// Justified only under named residuals or with incomplete provenance.
    Conditional { record: Box<VerificationRecord> },
    /// Every obligation of the named contract was discharged.
    Verified { record: Box<VerificationRecord> },
}

impl TrustClass {
    /// The strongest class this record justifies.
    #[must_use]
    pub fn from_record(record: &VerificationRecord) -> Self {
        record.justifies()
    }

    /// A short name, used in messages and certificates.
    #[must_use]
    pub const fn level_name(&self) -> &'static str {
        match self {
            Self::Unverified => "unverified",
            Self::Conditional { .. } => "conditional",
            Self::Verified { .. } => "verified",
        }
    }

    /// Ordering by strength alone, ignoring which contract was discharged.
    #[must_use]
    const fn level(&self) -> u8 {
        match self {
            Self::Unverified => 0,
            Self::Conditional { .. } => 1,
            Self::Verified { .. } => 2,
        }
    }

    /// The contract this class was established under, if any.
    #[must_use]
    pub fn contract(&self) -> Option<&str> {
        match self {
            Self::Unverified => None,
            Self::Conditional { record } | Self::Verified { record } => Some(record.contract()),
        }
    }

    /// The trust of a value derived from `self` and `other`.
    ///
    /// Two inputs verified under the *same* contract yield the weaker of the
    /// two. Two inputs verified under *different* contracts yield nothing:
    /// neither contract covers the combination, and quietly picking one would
    /// be exactly the unearned strengthening this module exists to prevent. A
    /// derivation that deserves better needs a verifier of its own.
    #[must_use]
    pub fn meet(&self, other: &Self) -> Self {
        match (self.contract(), other.contract()) {
            (Some(left), Some(right)) if left == right => {
                if self.level() <= other.level() {
                    self.clone()
                } else {
                    other.clone()
                }
            }
            // Either side unverified, or two different contracts: neither
            // covers the combination.
            _ => Self::Unverified,
        }
    }

    /// The trust of every value in `values`, combined.
    #[must_use]
    pub fn meet_all<'a>(values: impl IntoIterator<Item = &'a Self>) -> Self {
        let mut values = values.into_iter();
        let Some(first) = values.next() else {
            // Nothing was combined, so nothing was checked.
            return Self::Unverified;
        };
        values.fold(first.clone(), |accumulated, next| accumulated.meet(next))
    }

    /// The trust of this value after it passed through an opaque hop, plus the
    /// loss to record.
    ///
    /// A value that crossed an unmodelled boundary is not the value that was
    /// checked. Its assurance does not survive the crossing, and the reason is
    /// carried forward so the certificate can say where structure was lost
    /// rather than leaving a reader to infer it.
    #[must_use]
    pub fn after_opaque_hop(&self, reason: &str) -> (Self, ProvenanceLoss) {
        (
            Self::Unverified,
            ProvenanceLoss {
                reason: reason.to_string(),
                lost_class: self.level_name().to_string(),
            },
        )
    }
}

/// A recorded crossing of an opaque boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceLoss {
    pub reason: String,
    pub lost_class: String,
}

/// The wire shape of a trust class.
///
/// A strengthened class must carry the record that justifies it. A document
/// that simply says `verified` and stops does not deserialize, which is the
/// point.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RawTrustClass {
    Unverified,
    Conditional { record: RawVerificationRecord },
    Verified { record: RawVerificationRecord },
}

impl TryFrom<RawTrustClass> for TrustClass {
    type Error = TrustError;

    fn try_from(raw: RawTrustClass) -> Result<Self, Self::Error> {
        match raw {
            RawTrustClass::Unverified => Ok(Self::Unverified),
            RawTrustClass::Conditional { record } => admit_claim(record, "conditional", 1),
            RawTrustClass::Verified { record } => admit_claim(record, "verified", 2),
        }
    }
}

fn admit_claim(
    raw: RawVerificationRecord,
    claimed: &'static str,
    claimed_level: u8,
) -> Result<TrustClass, TrustError> {
    let verdict = raw.verdict;
    let record = VerificationRecord::admit(raw)?;
    let justified = record.justifies();
    if justified.level() < claimed_level {
        if matches!(justified, TrustClass::Unverified) {
            return Err(TrustError::VerdictTooWeak {
                found: verdict.as_str().to_string(),
            });
        }
        return Err(TrustError::Unjustified {
            claimed,
            justified: justified.level_name(),
        });
    }
    // A claim weaker than the record justifies is honest, so keep the claim.
    Ok(match claimed_level {
        2 => justified,
        1 => TrustClass::Conditional {
            record: Box::new(record),
        },
        _ => TrustClass::Unverified,
    })
}

impl<'de> Deserialize<'de> for TrustClass {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawTrustClass::deserialize(deserializer)?;
        Self::try_from(raw).map_err(serde::de::Error::custom)
    }
}
