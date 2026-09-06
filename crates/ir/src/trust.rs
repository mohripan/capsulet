//! Trust classes: what a value's assurance is, as a type.
//!
//! The rule this module enforces is short. Trust never strengthens by
//! assertion. Not by a cast, not by a setter, and not by a field in a document
//! someone posted. The only way to reach [`TrustClass::Verified`] is to present
//! a [`VerificationRecord`], and the only way to build one of those is
//! [`VerificationRecord::admit`], which resolves the certificate the record
//! names and takes every field it decides on from that certificate rather than
//! from the document.
//!
//! Weakening, by contrast, is always allowed. A value may be treated as less
//! assured than it is; that is a conservative mistake, not an unsound one.
//!
//! [`TrustClass`] has no [`serde::Deserialize`], and that absence is the design.
//! A wire document is whatever the sender wrote, and a type whose invariant
//! lives in a certificate somewhere else cannot re-establish it from bytes
//! alone. [`Certificate`] *can* have one, because it carries its own seal and
//! re-checks it. A verification record has no seal of its own, so it is admitted
//! against a [`CertificateSource`] the same way replay resolves evidence
//! against an evidence source.
//!
//! What a document may still say is which certificate and which contract. Both
//! are then checked, and neither can be made true by writing it down.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::correctness::certificate::{AssuranceVerdict, Certificate};
use crate::digest::Digest;

/// Why a trust claim was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TrustError {
    #[error("a verification record must name the contract it discharged")]
    MissingContract,
    #[error("no certificate resolves to {certificate}")]
    CertificateNotFound { certificate: Digest },
    #[error("certificate {certificate} does not cover the contract `{contract}`")]
    ContractNotCovered {
        contract: String,
        certificate: Digest,
    },
}

/// Whether the value a record is about reached here without crossing a boundary
/// the model does not describe.
///
/// Deliberately *not* part of the wire record, and deliberately not read off the
/// certificate. A certificate says what a run proved; whether a particular value
/// travelled here intact is a property of that value's path, which only the code
/// that moved it knows. Passing it separately keeps it out of documents, where
/// it would be exactly the unearned strengthening this module exists to refuse.
///
/// It is still an input rather than a derivation. See `CAP-IR-006`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// No unmodelled hop; the value that is here is the value that was checked.
    Complete,
    /// The value crossed a boundary this model does not describe.
    Lost,
}

/// Where an admission resolves the certificate a record names.
///
/// A trait rather than a store handle, for the reason [`crate::correctness`]
/// replay takes one: admission must work the same against a bundle on disk, a
/// database, or a fixture, and a trait with one method is hard to accidentally
/// give network access.
pub trait CertificateSource {
    /// The certificate with this digest, if this source has it.
    fn certificate(&self, digest: &Digest) -> Option<&Certificate>;
}

/// The simplest source: certificates held in memory, keyed by their own digest.
#[derive(Debug, Clone, Default)]
pub struct CertificateMap {
    by_digest: BTreeMap<Digest, Certificate>,
}

impl CertificateMap {
    /// An empty source.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a certificate, keyed by its own replay digest.
    pub fn insert(&mut self, certificate: Certificate) -> Digest {
        let digest = *certificate.replay_digest();
        self.by_digest.insert(digest, certificate);
        digest
    }

    /// How many certificates this source holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_digest.len()
    }

    /// Whether the source is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_digest.is_empty()
    }
}

impl CertificateSource for CertificateMap {
    fn certificate(&self, digest: &Digest) -> Option<&Certificate> {
        self.by_digest.get(digest)
    }
}

/// The wire shape of a verification record.
///
/// Two fields, because two is all a sender may usefully say: *which* certificate
/// and *which* contract. Everything a decision reads — the verdict, how many
/// obligations are still open — is taken from the certificate once it resolves.
/// A document cannot make any of that true by stating it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawVerificationRecord {
    pub contract: String,
    pub certificate: Digest,
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
    verdict: AssuranceVerdict,
    residual_count: u32,
    provenance: Provenance,
}

impl VerificationRecord {
    /// Admits a raw record against the certificate it names.
    ///
    /// The certificate must resolve, and it must cover the contract claimed.
    /// The verdict and the residual count are then read from it — not from
    /// `raw`, which has no way to say either.
    ///
    /// # Errors
    ///
    /// [`TrustError::MissingContract`] when no contract is named,
    /// [`TrustError::CertificateNotFound`] when nothing resolves the digest, and
    /// [`TrustError::ContractNotCovered`] when the certificate says nothing
    /// about the contract claimed.
    pub fn admit(
        raw: RawVerificationRecord,
        certificates: &impl CertificateSource,
        provenance: Provenance,
    ) -> Result<Self, TrustError> {
        if raw.contract.trim().is_empty() {
            return Err(TrustError::MissingContract);
        }

        let Some(certificate) = certificates.certificate(&raw.certificate) else {
            return Err(TrustError::CertificateNotFound {
                certificate: raw.certificate,
            });
        };

        if !covers(certificate, &raw.contract) {
            return Err(TrustError::ContractNotCovered {
                contract: raw.contract,
                certificate: raw.certificate,
            });
        }

        let body = certificate.body();
        Ok(Self {
            contract: raw.contract,
            certificate: raw.certificate,
            verdict: certificate.verdict(),
            residual_count: u32::try_from(body.residuals().count()).unwrap_or(u32::MAX),
            provenance,
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

    /// The verdict, as the named certificate recorded it.
    #[must_use]
    pub const fn verdict(&self) -> AssuranceVerdict {
        self.verdict
    }

    /// How many of that certificate's obligations are still open.
    #[must_use]
    pub const fn residual_count(&self) -> u32 {
        self.residual_count
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
            AssuranceVerdict::Accepted
                if self.residual_count == 0 && self.provenance == Provenance::Complete =>
            {
                TrustClass::Verified {
                    record: Box::new(self.clone()),
                }
            }
            AssuranceVerdict::Accepted | AssuranceVerdict::Conditional => TrustClass::Conditional {
                record: Box::new(self.clone()),
            },
            AssuranceVerdict::Rejected | AssuranceVerdict::Unverified => TrustClass::Unverified,
        }
    }
}

/// Whether a certificate covers a contract.
///
/// Today this reads the list the certificate declares, which is a weaker
/// question than it looks: nothing checks that any obligation is *about* the
/// contract. Task 3 of the correctness robustness plan replaces the body of this
/// function with coverage computed from the definition, and it is a function so
/// that there is one place to do it.
fn covers(certificate: &Certificate, contract: &str) -> bool {
    certificate
        .body()
        .contracts
        .iter()
        .any(|declared| declared.as_str() == contract)
}

/// The assurance attached to a value.
///
/// No `Deserialize`, on purpose — see the module documentation. A value read
/// from a document starts at [`TrustClass::Unverified`] and strengthens only
/// through [`TrustClass::from_record`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrustClass {
    /// Nothing checked this value, or what checked it failed.
    ///
    /// The default, so that anywhere a class is absent — including a field a
    /// document is not allowed to set — lands on the weakest answer.
    #[default]
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

    /// The same record, held at a weaker class than it justifies.
    ///
    /// Treating a value as less assured than it is stays available, because it
    /// is the safe direction. There is no matching way up.
    #[must_use]
    pub fn weakened(&self) -> Self {
        match self {
            Self::Unverified | Self::Conditional { .. } => Self::Unverified,
            Self::Verified { record } => Self::Conditional {
                record: record.clone(),
            },
        }
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
