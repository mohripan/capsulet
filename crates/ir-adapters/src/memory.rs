//! Governed memory into the IR.
//!
//! A claim in memory is a proposition somebody asserted, backed by evidence
//! that points into a source. In IR terms that is exactly an obligation with
//! evidence attached: the claim is grounded in its source, or it is not, and
//! the kernel already decides which. Admitting it to trusted memory is a
//! protected boundary, because a write into governed knowledge is a trust
//! transition and not merely a side effect.

use capsulet_core::{Claim, Evidence, SourceContent};
use capsulet_ir::correctness::Identity;
use capsulet_ir::correctness::certificate::CheckerVerdict;
use capsulet_ir::correctness::evidence::{EvidenceRef, RecordedTime};
use capsulet_ir::correctness::obligation::{
    DischargeState, Obligation, ObligationStatement, RepairOwner,
};
use capsulet_ir::correctness::proposal::{Producer, ProducerKind};
use capsulet_ir::digest::Digest;
use capsulet_ir::effect::{Crossing, ProtectedBoundary};
use capsulet_ir::id::Identifier;

use crate::AdapterError;

/// A claim, translated: the bytes it rests on and the obligation it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryEvidence {
    /// The source bytes, so the citation can be re-derived rather than
    /// believed.
    pub bytes: Vec<u8>,
    pub reference: EvidenceRef,
    pub obligation: Obligation,
}

/// Translates a claim and the evidence behind it.
///
/// The `observed_at` value is carried as recorded milliseconds; this function
/// reads no clock, because a translation that timestamps itself would make two
/// runs over the same records produce two different digests.
///
/// # Errors
///
/// Returns [`AdapterError`] when an identifier in the records is not a legal IR
/// identifier.
pub fn from_memory_claim(
    claim: &Claim,
    evidence: &Evidence,
    content: &SourceContent,
    observed_at_millis: i64,
) -> Result<MemoryEvidence, AdapterError> {
    let bytes = content.text().as_bytes().to_vec();
    let reference = EvidenceRef {
        id: identifier(evidence.id().as_str())?,
        content: Digest::of(&bytes),
        media_type: "text/plain".to_string(),
        byte_length: bytes.len() as u64,
        producer: Producer {
            kind: ProducerKind::Retrieval,
            identity: Identity::new(
                identifier(evidence.source_id().as_str())?,
                content.content_hash().to_string(),
            ),
        },
        captured_at: RecordedTime(observed_at_millis),
    };

    // Whether the span really says what the claim says is the kernel's
    // question, and this translation does not pre-empt it: the obligation
    // arrives residual, owned by whoever can decide it.
    let obligation = Obligation {
        statement: ObligationStatement {
            id: identifier(claim.id().as_str())?,
            statement: format!(
                "`{} {} {}` is grounded in the cited span of its source",
                claim.subject_id().as_str(),
                claim.predicate(),
                claim.object()
            ),
            owner: RepairOwner::Verifier,
        },
        contract: identifier("memory/claim-is-grounded")?,
        state: DischargeState::Residual {
            rationale: "the kernel decides grounding; this translation only records the claim"
                .to_string(),
            evidence: vec![reference.content],
        },
    };

    Ok(MemoryEvidence {
        bytes,
        reference,
        obligation,
    })
}

/// The boundary a memory write crosses.
///
/// # Errors
///
/// Returns [`AdapterError`] when a name is not a legal IR identifier.
pub fn memory_write_boundary(
    node: &str,
    effect: &str,
    space: &str,
) -> Result<ProtectedBoundary, AdapterError> {
    Ok(ProtectedBoundary {
        id: identifier(&format!("memory-write:{space}"))?,
        node: identifier(node)?,
        crossing: Crossing::Effect {
            effect: identifier(effect)?,
        },
        description: format!(
            "Admitting a claim into governed memory space `{space}`, which is a trust transition \
             and not only a side effect"
        ),
    })
}

/// Wraps a stored reasoning verdict for a platform certificate.
///
/// The kernel decided this once, against a snapshot that was not stored beside
/// the certificate. Replay therefore cannot re-decide it, and pretending
/// otherwise would be the exact overstatement the whole design refuses. The
/// verdict travels as a declared oracle: pinned, attributed, and openly not
/// re-derived.
#[must_use]
pub fn wrap_reasoning_verdict(verdict: CheckerVerdict) -> (CheckerVerdict, &'static str) {
    (
        verdict,
        "the snapshot this verdict was decided against is not stored with the certificate, so \
         replay checks its identity and version but does not re-decide it",
    )
}

fn identifier(value: &str) -> Result<Identifier, AdapterError> {
    Identifier::parse(value).map_err(|source| AdapterError::Identifier {
        what: "memory identifier",
        source,
    })
}
