//! Offline replay.
//!
//! Replay answers one question: does this certificate still justify its verdict,
//! given the evidence it points at? It answers it with no network, no model, no
//! database, and no clock — only the certificate, the bytes, and this kernel.
//!
//! What replay can and cannot do is worth stating plainly, because a claim that
//! overstates itself is the failure mode this whole design exists to avoid.
//!
//! Replay **can**: re-check the seal, re-check that every piece of evidence
//! digests to what the certificate says, re-decide any deterministic family
//! from its pinned inputs, and recompute the verdict from the obligations under
//! the recorded mode.
//!
//! Replay **cannot**: re-run an external tool. A scanner, a test runner, or a
//! compiler that produced evidence is a declared oracle: its identity, version,
//! and environment are pinned and checked, its word is taken, and the outcome
//! says so out loud rather than letting a reader assume the kernel confirmed it.

use std::collections::BTreeMap;

use capsulet_ir::correctness::certificate::{
    AssuranceVerdict, Certificate, CheckerVerdict, VerifierTrust,
};
use capsulet_ir::correctness::obligation::{DischargeState, Obligation};
use capsulet_ir::digest::Digest;
use capsulet_ir::version::{CERTIFICATE_SCHEMA_VERSION, SchemaVersionError, read_compatible};

use crate::family::{CLAIM_REASONING, ClaimReasoning, deterministic_families};
use crate::ir::Proposal;
use crate::snapshot_document::SnapshotDocument;
use crate::workflow::KERNEL_VERSION;

/// Where replay gets the bytes a certificate refers to.
///
/// A trait rather than a filesystem path, because replay must work the same way
/// against a bundle on disk, a blob store, or an in-memory fixture — and
/// because a trait with one method is hard to accidentally give network access.
pub trait EvidenceSource {
    /// The bytes behind a digest, if this source has them.
    fn bytes(&self, digest: &Digest) -> Option<&[u8]>;
}

/// The simplest source: content-addressed bytes held in memory.
#[derive(Debug, Clone, Default)]
pub struct EvidenceMap {
    blobs: BTreeMap<Digest, Vec<u8>>,
}

impl EvidenceMap {
    /// An empty source.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds bytes, keyed by their own digest.
    pub fn insert(&mut self, bytes: Vec<u8>) -> Digest {
        let digest = Digest::of(&bytes);
        self.blobs.insert(digest, bytes);
        digest
    }

    /// Adds bytes under a digest the caller chose.
    ///
    /// Used by tests that need to simulate tampering; replay notices, which is
    /// the point of having the case.
    pub fn insert_as(&mut self, digest: Digest, bytes: Vec<u8>) {
        self.blobs.insert(digest, bytes);
    }

    /// How many blobs this source holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.blobs.len()
    }

    /// Whether the source is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.blobs.is_empty()
    }

    /// The digests this source holds, in canonical order.
    pub fn digests(&self) -> impl Iterator<Item = &Digest> {
        self.blobs.keys()
    }
}

impl EvidenceSource for EvidenceMap {
    fn bytes(&self, digest: &Digest) -> Option<&[u8]> {
        self.blobs.get(digest).map(Vec::as_slice)
    }
}

/// Something replay found that the certificate does not account for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayFinding {
    /// The body no longer digests to the recorded seal.
    SealBroken,
    /// The certificate says a piece of evidence exists; the source does not
    /// have it.
    EvidenceMissing { digest: Digest },
    /// The bytes are there, and they are not the bytes the certificate cites.
    EvidenceTampered { recorded: Digest, found: Digest },
    /// A verifier claimed to be deterministic under a name this kernel does not
    /// know. Fails closed: an unknown checker is not a trusted one.
    UnknownDeterministicVerifier { identity: String },
    /// A deterministic family was re-decided and reached a different verdict.
    FamilyDisagrees {
        identity: String,
        recorded: CheckerVerdict,
        recomputed: CheckerVerdict,
    },
    /// A deterministic family could not be re-decided because its pinned inputs
    /// were not in the bundle.
    FamilyInputsMissing { identity: String },
    /// The verdict recomputed from the certificate's own contents differs from
    /// the one it records.
    VerdictDiffers {
        recorded: AssuranceVerdict,
        recomputed: AssuranceVerdict,
    },
}

/// Something replay wants the reader to know, which is not a problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayNote {
    /// A declared oracle's word was taken. Its identity, version, and
    /// environment matched; the tool itself was not re-run.
    OracleNotReExecuted { identity: String, rationale: String },
    /// A different kernel build produced this certificate. The verdicts still
    /// agree, but the reader should know two builds were involved.
    KernelVersionDiffers { recorded: String, replaying: String },
    /// A deterministic family was re-decided and agreed.
    FamilyReDecided { identity: String },
}

/// Why replay could not read the certificate at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unreadable {
    /// A schema version this build does not understand. Fails closed rather
    /// than guessing: a document from the future is not an empty document.
    SchemaVersion { source: SchemaVersionError },
}

/// What replay concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayOutcome {
    /// The certificate still justifies its verdict.
    Reproduced {
        verdict: AssuranceVerdict,
        notes: Vec<ReplayNote>,
    },
    /// It does not. The findings say why, and the recomputed verdict is what
    /// the evidence actually supports now.
    Diverged {
        recorded: AssuranceVerdict,
        recomputed: AssuranceVerdict,
        findings: Vec<ReplayFinding>,
        notes: Vec<ReplayNote>,
    },
    /// This build cannot interpret the document.
    Unreadable { reason: Unreadable },
}

impl ReplayOutcome {
    /// Whether replay reached the recorded verdict.
    #[must_use]
    pub const fn reproduced(&self) -> bool {
        matches!(self, Self::Reproduced { .. })
    }

    /// The verdict the evidence supports now.
    #[must_use]
    pub const fn verdict(&self) -> Option<AssuranceVerdict> {
        match self {
            Self::Reproduced { verdict, .. } => Some(*verdict),
            Self::Diverged { recomputed, .. } => Some(*recomputed),
            Self::Unreadable { .. } => None,
        }
    }
}

/// Replays a certificate against the evidence it cites.
///
/// Pure and total: no I/O, no clock, no randomness, and a decision for every
/// input.
#[must_use]
pub fn replay(certificate: &Certificate, evidence: &impl EvidenceSource) -> ReplayOutcome {
    let body = certificate.body();

    if let Err(source) = read_compatible(
        &body.schema_version.to_string(),
        &CERTIFICATE_SCHEMA_VERSION,
    ) {
        return ReplayOutcome::Unreadable {
            reason: Unreadable::SchemaVersion { source },
        };
    }

    let mut findings = Vec::new();
    let mut notes = Vec::new();

    if certificate.verify_seal().is_err() {
        findings.push(ReplayFinding::SealBroken);
    }

    if body.kernel_version != KERNEL_VERSION {
        notes.push(ReplayNote::KernelVersionDiffers {
            recorded: body.kernel_version.clone(),
            replaying: KERNEL_VERSION.to_string(),
        });
    }

    let unusable = check_evidence(body, evidence, &mut findings);
    check_verifiers(body, evidence, &mut findings, &mut notes);

    // An obligation that rested on evidence nobody can produce is not
    // discharged any more, whatever the certificate says.
    let obligations: Vec<Obligation> = body
        .obligations
        .iter()
        .map(|obligation| downgrade_if_unusable(obligation, &unusable))
        .collect();

    let recomputed = if findings.iter().any(is_disqualifying) {
        AssuranceVerdict::Rejected
    } else {
        AssuranceVerdict::under_mode(body.mode, &obligations)
    };

    if recomputed == body.verdict && findings.is_empty() {
        ReplayOutcome::Reproduced {
            verdict: recomputed,
            notes,
        }
    } else {
        if recomputed != body.verdict {
            findings.push(ReplayFinding::VerdictDiffers {
                recorded: body.verdict,
                recomputed,
            });
        }
        ReplayOutcome::Diverged {
            recorded: body.verdict,
            recomputed,
            findings,
            notes,
        }
    }
}

/// Re-checks that every piece of evidence is present and is the bytes the
/// certificate cites. Returns the digests that turned out to be unusable.
fn check_evidence(
    body: &capsulet_ir::correctness::certificate::CertificateBody,
    evidence: &impl EvidenceSource,
    findings: &mut Vec<ReplayFinding>,
) -> Vec<Digest> {
    let mut unusable = Vec::new();
    for reference in &body.evidence {
        match evidence.bytes(&reference.content) {
            None => {
                findings.push(ReplayFinding::EvidenceMissing {
                    digest: reference.content,
                });
                unusable.push(reference.content);
            }
            Some(bytes) => {
                let found = Digest::of(bytes);
                if found != reference.content {
                    findings.push(ReplayFinding::EvidenceTampered {
                        recorded: reference.content,
                        found,
                    });
                    unusable.push(reference.content);
                }
            }
        }
    }
    unusable
}

/// Re-decides what can be re-decided, and records what was taken on trust.
fn check_verifiers(
    body: &capsulet_ir::correctness::certificate::CertificateBody,
    evidence: &impl EvidenceSource,
    findings: &mut Vec<ReplayFinding>,
    notes: &mut Vec<ReplayNote>,
) {
    for record in &body.verifiers {
        let identity = record.identity.name.to_string();
        match &record.trust {
            VerifierTrust::DeclaredOracle { rationale } => {
                notes.push(ReplayNote::OracleNotReExecuted {
                    identity,
                    rationale: rationale.clone(),
                });
            }
            VerifierTrust::Deterministic => {
                if !deterministic_families().contains(&identity.as_str()) {
                    findings.push(ReplayFinding::UnknownDeterministicVerifier { identity });
                    continue;
                }
                match redecide(&identity, record.inputs.as_slice(), evidence) {
                    Redecision::Missing => {
                        findings.push(ReplayFinding::FamilyInputsMissing { identity });
                    }
                    Redecision::Verdict(recomputed) if recomputed != record.verdict => {
                        findings.push(ReplayFinding::FamilyDisagrees {
                            identity,
                            recorded: record.verdict,
                            recomputed,
                        });
                    }
                    Redecision::Verdict(_) => {
                        notes.push(ReplayNote::FamilyReDecided { identity });
                    }
                }
            }
        }
    }
}

/// Whether a finding means the certificate can no longer support any positive
/// verdict.
const fn is_disqualifying(finding: &ReplayFinding) -> bool {
    matches!(
        finding,
        ReplayFinding::SealBroken
            | ReplayFinding::UnknownDeterministicVerifier { .. }
            | ReplayFinding::FamilyDisagrees { .. }
    )
}

fn downgrade_if_unusable(obligation: &Obligation, unusable: &[Digest]) -> Obligation {
    let rests_on_missing = obligation
        .state
        .evidence()
        .iter()
        .any(|digest| unusable.contains(digest));

    if !rests_on_missing {
        return obligation.clone();
    }

    let mut downgraded = obligation.clone();
    downgraded.state = DischargeState::Failed {
        reason: "the evidence this rested on is missing or does not match its digest".to_string(),
        owner: obligation.statement.owner,
    };
    downgraded
}

enum Redecision {
    Missing,
    Verdict(CheckerVerdict),
}

/// Re-decides a deterministic family from the inputs the certificate pinned.
///
/// The claim-reasoning family pins two inputs: the proposal and the snapshot it
/// was decided against. Given both, the kernel reaches its own conclusion
/// rather than reading the recorded one.
fn redecide(identity: &str, inputs: &[Digest], evidence: &impl EvidenceSource) -> Redecision {
    if identity != CLAIM_REASONING {
        return Redecision::Missing;
    }
    let [proposal, snapshot] = inputs else {
        return Redecision::Missing;
    };
    let (Some(proposal), Some(snapshot)) = (evidence.bytes(proposal), evidence.bytes(snapshot))
    else {
        return Redecision::Missing;
    };
    let (Ok(proposal), Ok(document)) = (
        serde_json::from_slice::<Proposal>(proposal),
        serde_json::from_slice::<SnapshotDocument>(snapshot),
    ) else {
        return Redecision::Missing;
    };
    let Ok(snapshot) = document.rebuild() else {
        return Redecision::Missing;
    };

    Redecision::Verdict(ClaimReasoning::decide(&proposal, &snapshot).verdict)
}
