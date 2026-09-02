//! Obligation families.
//!
//! The kernel used to decide exactly one thing: whether a claim was grounded in
//! its sources. That check is not going away — it becomes the first family in a
//! set, with its behaviour unchanged — but a workflow certificate has to carry
//! obligations from several kinds of checker, and a replayer has to know which
//! of them it can recompute for itself.
//!
//! A family is identified by name and version, and says whether it is
//! deterministic. Deterministic families can be re-decided offline from pinned
//! inputs, which is what makes a certificate checkable by someone who was not
//! there. Everything else is a declared oracle: its identity, version, and
//! environment are pinned, its word is taken, and the certificate says so
//! rather than implying the kernel confirmed it.

use capsulet_ir::correctness::certificate::CheckerVerdict;

use crate::certificate::{Certificate as ClaimCertificate, Verdict};
use crate::ir::Proposal;
use crate::snapshot::Snapshot;

/// The name of the claim-reasoning family, as it appears in certificates.
pub const CLAIM_REASONING: &str = "capsulet-kernel/claim-reasoning";

/// What a family concluded, plus what it needed to conclude it.
#[derive(Debug, Clone, PartialEq)]
pub struct FamilyDecision {
    pub verdict: CheckerVerdict,
    /// The claim-level certificate, where the family produced one.
    pub claim: Option<ClaimCertificate>,
}

/// A checker the kernel can run.
pub trait ObligationFamily {
    /// The family's stable name.
    fn name(&self) -> &'static str;

    /// The version of the rules it applies.
    fn version(&self) -> &'static str;

    /// Whether a replayer can re-decide this family from pinned inputs alone.
    ///
    /// Only a family that reads nothing but its inputs may say `true` here. It
    /// is the difference between a certificate someone else can check and one
    /// they have to believe.
    fn is_deterministic(&self) -> bool;
}

/// The claim-reasoning rules the kernel has always applied.
#[derive(Debug, Clone, Copy, Default)]
pub struct ClaimReasoning;

impl ClaimReasoning {
    /// Decides a proposal against a snapshot.
    ///
    /// Exactly the existing [`crate::check`], wrapped so its conclusion can sit
    /// in a workflow certificate beside other families.
    #[must_use]
    pub fn decide(proposal: &Proposal, snapshot: &Snapshot) -> FamilyDecision {
        let claim = crate::check(proposal, snapshot);
        FamilyDecision {
            verdict: to_checker_verdict(claim.verdict),
            claim: Some(claim),
        }
    }
}

impl ObligationFamily for ClaimReasoning {
    fn name(&self) -> &'static str {
        CLAIM_REASONING
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn is_deterministic(&self) -> bool {
        true
    }
}

/// The kernel verdict, as the platform spells it.
#[must_use]
pub const fn to_checker_verdict(verdict: Verdict) -> CheckerVerdict {
    match verdict {
        Verdict::Accepted => CheckerVerdict::Accepted,
        Verdict::Conditional => CheckerVerdict::Conditional,
        Verdict::Rejected => CheckerVerdict::Rejected,
    }
}

/// The platform verdict, as the kernel spells it.
#[must_use]
pub const fn from_checker_verdict(verdict: CheckerVerdict) -> Verdict {
    match verdict {
        CheckerVerdict::Accepted => Verdict::Accepted,
        CheckerVerdict::Conditional => Verdict::Conditional,
        CheckerVerdict::Rejected => Verdict::Rejected,
    }
}

/// The families this build can re-decide.
///
/// Deliberately a fixed list rather than a plugin point: a replayer that would
/// load a checker it was told about could be told about anything. M4 adds
/// external verifiers through a declared protocol, and they appear in
/// certificates as oracles, not as families the kernel claims to have run.
#[must_use]
pub fn deterministic_families() -> &'static [&'static str] {
    &[CLAIM_REASONING]
}
