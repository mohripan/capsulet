//! The certificate: what was checked, from what, by whom, and what is left.
//!
//! A certificate is sealed. Its body is digested at construction, and the
//! digest travels with it, so editing any field afterwards is detectable — by
//! anyone, including someone who has only the document and not this
//! installation. Deserialization re-computes the digest and refuses a document
//! whose seal does not match, which means a tampered certificate does not
//! become a `Certificate` value at all.
//!
//! The verdict is derived from the obligations rather than set by the caller.
//! A field someone can assign is a field someone can assign optimistically.

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

use crate::admission::AdmissionRecord;
use crate::canonical::CanonicalError;
use crate::correctness::Identity;
use crate::correctness::evidence::EvidenceRef;
use crate::correctness::obligation::Obligation;
use crate::definition::AssuranceMode;
use crate::digest::Digest;
use crate::id::Identifier;
use crate::loop_region::{LoopOutcome, StopReason};
use crate::version::{CERTIFICATE_SCHEMA_VERSION, SchemaVersion};

/// What a checker concluded.
///
/// Three-valued, mirroring the correctness kernel: a checker that ran always
/// reaches one of these. "Not evaluated" is not a conclusion a checker can
/// reach, so it does not appear here — see [`AssuranceVerdict`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckerVerdict {
    Accepted,
    Conditional,
    Rejected,
}

/// What the platform can say about a value or a boundary.
///
/// Four-valued, because the platform has a case a checker does not: nothing
/// ran. `Unverified` is the honest answer when the correctness plane was not
/// invoked, and it is never a synonym for "fine".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssuranceVerdict {
    /// No checker ran. No correctness inference is permitted.
    Unverified,
    /// Every required obligation was discharged.
    Accepted,
    /// Justified only under named assumptions, waivers, or residuals.
    Conditional,
    /// A required obligation failed.
    Rejected,
}

impl AssuranceVerdict {
    /// A short name for messages, metrics, and storage.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unverified => "unverified",
            Self::Accepted => "accepted",
            Self::Conditional => "conditional",
            Self::Rejected => "rejected",
        }
    }

    /// The checker conclusion behind this verdict, if a checker ran.
    ///
    /// `None` for `Unverified`, which is precisely the case a three-valued
    /// checker cannot express.
    #[must_use]
    pub const fn checker_verdict(self) -> Option<CheckerVerdict> {
        match self {
            Self::Unverified => None,
            Self::Accepted => Some(CheckerVerdict::Accepted),
            Self::Conditional => Some(CheckerVerdict::Conditional),
            Self::Rejected => Some(CheckerVerdict::Rejected),
        }
    }

    /// The verdict the obligations justify.
    ///
    /// Derived, never assigned: a failure anywhere is `Rejected`, an
    /// outstanding assumption, waiver, or residual is `Conditional`, a full set
    /// of discharged obligations is `Accepted`, and an empty set is
    /// `Unverified` because nothing was evaluated.
    #[must_use]
    pub fn from_obligations(obligations: &[Obligation]) -> Self {
        if obligations.is_empty() {
            return Self::Unverified;
        }
        if obligations.iter().any(Obligation::has_failed) {
            return Self::Rejected;
        }
        if obligations.iter().any(Obligation::is_outstanding) {
            return Self::Conditional;
        }
        Self::Accepted
    }
}

impl From<CheckerVerdict> for AssuranceVerdict {
    fn from(verdict: CheckerVerdict) -> Self {
        match verdict {
            CheckerVerdict::Accepted => Self::Accepted,
            CheckerVerdict::Conditional => Self::Conditional,
            CheckerVerdict::Rejected => Self::Rejected,
        }
    }
}

/// How much a certificate relies on a verifier it cannot re-run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VerifierTrust {
    /// The kernel can recompute this decision from the recorded inputs.
    Deterministic,
    /// An external checker whose word is taken, with its identity, version, and
    /// environment pinned. Replay re-checks those and says plainly that it did
    /// not re-run the tool.
    DeclaredOracle { rationale: String },
}

/// What a verifier did, in enough detail to check the claim later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifierRecord {
    pub identity: Identity,
    /// The environment it ran in, pinned by digest: image, toolchain, ruleset.
    pub environment: Digest,
    pub inputs: Vec<Digest>,
    pub outputs: Vec<Digest>,
    pub trust: VerifierTrust,
    pub verdict: CheckerVerdict,
}

/// What the certificate is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subject {
    /// The definition, pinned by the digest of its canonical bytes.
    pub definition: Digest,
    pub definition_version: String,
    /// The run this came from, where there was one.
    pub run: Option<Identifier>,
    /// The inputs the run was given, pinned.
    pub inputs: Vec<Digest>,
    /// The values it produced.
    pub outputs: Vec<Digest>,
}

/// Everything a certificate asserts, before it is sealed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CertificateBody {
    pub schema_version: SchemaVersion,
    pub id: Identifier,
    pub subject: Subject,
    /// Proof that the definition passed structural admission. There is no way
    /// to build this without admission having run, so a certificate cannot
    /// describe a definition nobody could read.
    pub admission: AdmissionRecord,
    /// The mode this run was actually decided under, after any policy tightened
    /// what the definition declared. A verdict is only readable next to the
    /// mode that produced it: `unverified` under observe means nobody was asked
    /// to check, which is a different statement from `unverified` under
    /// enforce.
    pub mode: AssuranceMode,
    /// The policy in force, pinned by version, because a verdict means nothing
    /// without the policy that defined "required".
    pub policy_version: String,
    /// The kernel build that decided this. A replayer compares it to its own.
    pub kernel_version: String,
    pub contracts: Vec<Identifier>,
    pub verifiers: Vec<VerifierRecord>,
    pub evidence: Vec<EvidenceRef>,
    pub obligations: Vec<Obligation>,
    /// Why any loop in the run stopped. Present because a budget-exhausted loop
    /// producing a plausible answer is exactly the case a reader must not
    /// mistake for a finished one.
    pub loops: Vec<LoopOutcome>,
    pub verdict: AssuranceVerdict,
}

/// Why a certificate was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CertificateError {
    #[error(
        "certificate `{id}` records verdict `{recorded}`, but its obligations justify `{justified}`"
    )]
    VerdictNotJustified {
        id: Identifier,
        recorded: &'static str,
        justified: &'static str,
    },
    #[error(
        "certificate `{id}` rests on evidence `{digest}`, which it does not carry; a citation to \
         something absent cannot be checked"
    )]
    MissingEvidence { id: Identifier, digest: Digest },
    #[error("certificate `{id}` carries obligation `{obligation}` twice")]
    DuplicateObligation {
        id: Identifier,
        obligation: Identifier,
    },
    #[error("certificate `{id}` has been altered since it was sealed")]
    SealBroken { id: Identifier },
    #[error("certificate could not be encoded: {source}")]
    Encoding {
        #[source]
        source: CanonicalError,
    },
}

impl CertificateBody {
    /// Checks the body is internally consistent.
    ///
    /// # Errors
    ///
    /// Returns [`CertificateError`] when the verdict does not follow from the
    /// obligations under the recorded mode, an obligation rests on evidence the
    /// certificate does not carry, or an obligation appears twice.
    pub fn check(&self) -> Result<(), CertificateError> {
        let justified = AssuranceVerdict::under_mode(self.mode, &self.obligations);
        if justified != self.verdict {
            return Err(CertificateError::VerdictNotJustified {
                id: self.id.clone(),
                recorded: self.verdict.as_str(),
                justified: justified.as_str(),
            });
        }

        let mut seen: Vec<&Identifier> = Vec::new();
        for obligation in &self.obligations {
            if seen.contains(&&obligation.statement.id) {
                return Err(CertificateError::DuplicateObligation {
                    id: self.id.clone(),
                    obligation: obligation.statement.id.clone(),
                });
            }
            seen.push(&obligation.statement.id);

            for digest in obligation.state.evidence() {
                if !self.carries(digest) {
                    return Err(CertificateError::MissingEvidence {
                        id: self.id.clone(),
                        digest: *digest,
                    });
                }
            }
        }
        Ok(())
    }

    /// Whether this certificate carries the named evidence.
    #[must_use]
    pub fn carries(&self, digest: &Digest) -> bool {
        self.evidence
            .iter()
            .any(|evidence| &evidence.content == digest)
    }

    /// The obligations that are still open, in the order recorded.
    pub fn residuals(&self) -> impl Iterator<Item = &Obligation> {
        self.obligations
            .iter()
            .filter(|obligation| obligation.is_outstanding())
    }

    /// Every reason a loop in this run stopped short of finishing.
    pub fn incomplete_loops(&self) -> impl Iterator<Item = &StopReason> {
        self.loops
            .iter()
            .filter(|outcome| !outcome.completed())
            .map(|outcome| &outcome.stopped)
    }
}

/// A sealed certificate.
///
/// The seal is the digest of the body's canonical bytes. It is checked on
/// construction and again on deserialization, so a `Certificate` value always
/// matches the bytes it was made from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Certificate {
    body: CertificateBody,
    replay_digest: Digest,
}

impl Certificate {
    /// Seals a body.
    ///
    /// # Errors
    ///
    /// Returns [`CertificateError`] when the body is inconsistent or cannot be
    /// canonically encoded.
    pub fn seal(body: CertificateBody) -> Result<Self, CertificateError> {
        body.check()?;
        let replay_digest = Self::digest_of_body(&body)?;
        Ok(Self {
            body,
            replay_digest,
        })
    }

    fn digest_of_body(body: &CertificateBody) -> Result<Digest, CertificateError> {
        crate::digest_of(body).map_err(|source| CertificateError::Encoding { source })
    }

    /// What the certificate asserts.
    #[must_use]
    pub const fn body(&self) -> &CertificateBody {
        &self.body
    }

    /// The seal.
    #[must_use]
    pub const fn replay_digest(&self) -> &Digest {
        &self.replay_digest
    }

    /// The verdict.
    #[must_use]
    pub const fn verdict(&self) -> AssuranceVerdict {
        self.body.verdict
    }

    /// Re-computes the seal and compares it.
    ///
    /// # Errors
    ///
    /// Returns [`CertificateError::SealBroken`] when the body no longer digests
    /// to the recorded value.
    pub fn verify_seal(&self) -> Result<(), CertificateError> {
        let recomputed = Self::digest_of_body(&self.body)?;
        if recomputed == self.replay_digest {
            Ok(())
        } else {
            Err(CertificateError::SealBroken {
                id: self.body.id.clone(),
            })
        }
    }

    /// The schema version this build writes.
    #[must_use]
    pub fn current_schema_version() -> SchemaVersion {
        CERTIFICATE_SCHEMA_VERSION
    }
}

/// The wire shape, before the seal is checked.
#[derive(Deserialize)]
struct RawCertificate {
    body: CertificateBody,
    replay_digest: Digest,
}

impl<'de> Deserialize<'de> for Certificate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawCertificate::deserialize(deserializer)?;
        let certificate = Self {
            body: raw.body,
            replay_digest: raw.replay_digest,
        };
        certificate
            .verify_seal()
            .map_err(serde::de::Error::custom)?;
        certificate.body.check().map_err(serde::de::Error::custom)?;
        Ok(certificate)
    }
}
