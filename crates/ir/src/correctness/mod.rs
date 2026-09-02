//! Proposals, evidence, obligations, and certificates.
//!
//! These are the objects a correctness claim is made of, and they share one
//! property: nothing here is mutable after the fact. A proposal records what
//! was proposed, evidence records what was captured, an obligation records how
//! it was discharged or why it was not, and a certificate seals the lot under a
//! digest. Anything that could be edited afterwards could be edited to say the
//! run went better than it did.
//!
//! Time is recorded, never read. Nothing in this crate calls a clock, because a
//! replayer running a year later has to reach the same conclusion.

pub mod certificate;
pub mod evidence;
pub mod obligation;
pub mod proposal;

use serde::{Deserialize, Serialize};

use crate::id::Identifier;

pub use certificate::{
    AssuranceVerdict, Certificate, CertificateBody, CertificateError, CheckerVerdict, Subject,
    VerifierRecord, VerifierTrust,
};
pub use evidence::{Artifact, EvidenceRef, RecordedTime};
pub use obligation::{Contract, DischargeState, Obligation, ObligationStatement, RepairOwner};
pub use proposal::{Producer, ProducerKind, Proposal};

/// Who or what produced something.
///
/// Kept structural rather than free text, because "which model version wrote
/// this" is a question a certificate has to answer exactly.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Identity {
    pub name: Identifier,
    pub version: String,
}

impl Identity {
    /// Builds an identity.
    #[must_use]
    pub fn new(name: Identifier, version: impl Into<String>) -> Self {
        Self {
            name,
            version: version.into(),
        }
    }
}
