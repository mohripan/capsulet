//! Assembling a platform certificate.
//!
//! Everything here is arithmetic over things that already happened: an
//! admission record, the obligations a run produced, the evidence it captured,
//! and the mode it ran under. The kernel does not decide what to check here —
//! it records what was checked, in a shape someone else can verify.
//!
//! The verdict is not an argument. It is computed from the obligations under
//! the recorded mode, which is the same computation replay performs, so a
//! certificate whose verdict does not follow from its own contents cannot be
//! produced in the first place.

use capsulet_ir::admission::AdmissionRecord;
use capsulet_ir::correctness::certificate::{
    AssuranceVerdict, Certificate, CertificateBody, CertificateError, Subject, VerifierRecord,
};
use capsulet_ir::correctness::evidence::EvidenceRef;
use capsulet_ir::correctness::obligation::Obligation;
use capsulet_ir::definition::AssuranceMode;
use capsulet_ir::id::Identifier;
use capsulet_ir::loop_region::LoopOutcome;

/// The kernel build that decides and replays.
pub const KERNEL_VERSION: &str = concat!("capsulet-kernel ", env!("CARGO_PKG_VERSION"));

/// What a run produced, before it is sealed into a certificate.
#[derive(Debug, Clone)]
pub struct Assembly {
    pub id: Identifier,
    pub admission: AdmissionRecord,
    pub mode: AssuranceMode,
    pub subject: Subject,
    pub policy_version: String,
    pub contracts: Vec<Identifier>,
    pub verifiers: Vec<VerifierRecord>,
    pub evidence: Vec<EvidenceRef>,
    pub obligations: Vec<Obligation>,
    pub loops: Vec<LoopOutcome>,
}

/// Seals a run into a certificate.
///
/// # Errors
///
/// Returns [`CertificateError`] when the assembled body is inconsistent — an
/// obligation resting on evidence that is not carried, a duplicated obligation,
/// or a body that cannot be canonically encoded.
pub fn certify(assembly: Assembly) -> Result<Certificate, CertificateError> {
    let verdict = AssuranceVerdict::under_mode(assembly.mode, &assembly.obligations);

    Certificate::seal(CertificateBody {
        schema_version: Certificate::current_schema_version(),
        id: assembly.id,
        admission: assembly.admission,
        mode: assembly.mode,
        subject: assembly.subject,
        policy_version: assembly.policy_version,
        kernel_version: KERNEL_VERSION.to_string(),
        contracts: assembly.contracts,
        verifiers: assembly.verifiers,
        evidence: assembly.evidence,
        obligations: assembly.obligations,
        loops: assembly.loops,
        verdict,
    })
}
