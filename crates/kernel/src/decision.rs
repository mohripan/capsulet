//! What a verifier node decided, and a run's certificate assembled from it.
//!
//! A verifier node leaves its decision behind as a value, like any other node's
//! output: the obligations it decided, the record of each checker it ran, the
//! evidence those rest on, and that evidence's bytes. The run's log names the
//! value by digest, so the certificate is assembled from what the log says
//! happened and nothing a worker held in memory, which is what lets a resumed
//! run and an uninterrupted one certify the same (see `capsulet-runtime`'s
//! `certify`).
//!
//! [`assemble`] reads a run's decisions into an [`Assembly`]. Every obligation
//! the definition's contracts state ends up in it: in the state the last node to
//! decide it left it, or residual when no node of the run decided it. Nothing
//! here knows what any obligation means.

use std::collections::BTreeMap;

use capsulet_ir::admission::AdmissionRecord;
use capsulet_ir::correctness::certificate::{Subject, VerifierRecord};
use capsulet_ir::correctness::evidence::EvidenceRef;
use capsulet_ir::correctness::obligation::{DischargeState, Obligation};
use capsulet_ir::definition::{AssuranceMode, Definition};
use capsulet_ir::digest::Digest;
use capsulet_ir::id::Identifier;
use capsulet_ir::loop_region::LoopOutcome;
use serde::{Deserialize, Serialize};

use crate::bundle::{BundleError, base64_decode, base64_encode};
use crate::replay::{EvidenceMap, EvidenceSource};
use crate::workflow::Assembly;

/// The name of the output port a verifier node leaves its decision on.
pub const DECISION_PORT: &str = "decision";

/// What one verifier node decided, as the value it leaves behind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeDecision {
    pub obligations: Vec<Obligation>,
    pub verifiers: Vec<VerifierRecord>,
    pub evidence: Vec<EvidenceRef>,
    /// The evidence's bytes, keyed by digest and base64-encoded, as a bundle
    /// carries them.
    blobs: BTreeMap<String, String>,
}

impl NodeDecision {
    /// A decision, carrying the bytes of every piece of evidence it cites.
    ///
    /// # Errors
    ///
    /// Returns [`BundleError::MissingEvidence`] when `bytes` cannot supply
    /// something the evidence cites.
    pub fn new(
        obligations: Vec<Obligation>,
        verifiers: Vec<VerifierRecord>,
        evidence: Vec<EvidenceRef>,
        bytes: &impl EvidenceSource,
    ) -> Result<Self, BundleError> {
        let mut blobs = BTreeMap::new();
        for reference in &evidence {
            let found = bytes
                .bytes(&reference.content)
                .ok_or(BundleError::MissingEvidence {
                    digest: reference.content,
                })?;
            blobs.insert(reference.content.to_string(), base64_encode(found));
        }
        Ok(Self {
            obligations,
            verifiers,
            evidence,
            blobs,
        })
    }

    /// The evidence's bytes, each checked against the digest it is kept under.
    ///
    /// # Errors
    ///
    /// Returns [`BundleError`] when a blob is not base64, or its bytes are not
    /// the ones its digest names.
    pub fn bytes(&self) -> Result<EvidenceMap, BundleError> {
        let mut map = EvidenceMap::new();
        for (key, encoded) in &self.blobs {
            let recorded: Digest = key.parse().map_err(|_| BundleError::Malformed {
                detail: format!("`{key}` is not a digest"),
            })?;
            let bytes =
                base64_decode(encoded).ok_or(BundleError::MalformedBlob { digest: recorded })?;
            let found = Digest::of(&bytes);
            if found != recorded {
                return Err(BundleError::BlobDigestMismatch { recorded, found });
            }
            map.insert(bytes);
        }
        Ok(map)
    }
}

/// What a run's log says, read for its certificate.
#[derive(Debug, Clone)]
pub struct RunRecord<'a> {
    /// The certificate's id.
    pub id: Identifier,
    pub definition: &'a Definition,
    pub admission: AdmissionRecord,
    /// The mode the run was decided under.
    pub mode: AssuranceMode,
    pub subject: Subject,
    pub loops: Vec<LoopOutcome>,
    /// Each verifier node's decision, in the order the log finished them.
    pub decisions: &'a [NodeDecision],
}

/// Reads a run into what its certificate is sealed from, and the bytes of the
/// evidence that certificate will cite.
///
/// Each obligation the definition's contracts state is the last decision's
/// that states it, or residual when none does. Verifier records and evidence
/// are every decision's, each once. The policy version is the definition's own
/// id and version, since its contracts are the policy defaults the run was
/// held to.
///
/// # Errors
///
/// Returns [`BundleError`] when a decision's bytes do not read.
pub fn assemble(run: RunRecord<'_>) -> Result<(Assembly, EvidenceMap), BundleError> {
    let mut bytes = EvidenceMap::new();
    let mut verifiers: Vec<VerifierRecord> = Vec::new();
    let mut evidence: Vec<EvidenceRef> = Vec::new();
    for decision in run.decisions {
        bytes.merge(decision.bytes()?);
        for record in &decision.verifiers {
            if !verifiers.contains(record) {
                verifiers.push(record.clone());
            }
        }
        for reference in &decision.evidence {
            if !evidence.contains(reference) {
                evidence.push(reference.clone());
            }
        }
    }

    let mut obligations = Vec::new();
    for contract in &run.definition.contracts {
        for statement in &contract.obligations {
            let decided = run
                .decisions
                .iter()
                .rev()
                .flat_map(|decision| &decision.obligations)
                .find(|obligation| {
                    obligation.contract == contract.id && obligation.statement.id == statement.id
                });
            obligations.push(decided.cloned().unwrap_or_else(|| Obligation {
                statement: statement.clone(),
                contract: contract.id.clone(),
                state: DischargeState::Residual {
                    rationale: "no verifier node of the run decided it".to_string(),
                    evidence: Vec::new(),
                },
            }));
        }
    }

    let assembly = Assembly {
        id: run.id,
        admission: run.admission,
        mode: run.mode,
        subject: run.subject,
        policy_version: format!("{}/{}", run.definition.id, run.definition.version),
        contracts: run
            .definition
            .contracts
            .iter()
            .map(|contract| contract.id.clone())
            .collect(),
        verifiers,
        evidence,
        obligations,
        loops: run.loops,
    };
    Ok((assembly, bytes))
}
