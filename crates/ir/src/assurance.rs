//! Assurance policy: what a verdict has to be before something happens.
//!
//! One decision procedure lives here, and everything that gates a boundary uses
//! it: the API that answers "may this publish", the worker that will exist in
//! M3, and the CLI a person runs by hand. Three implementations of this rule
//! would eventually disagree, and the one that disagreed in the permissive
//! direction would be the one nobody noticed.
//!
//! The rule itself is short. A boundary is crossed only if a certificate says
//! the run met the policy's minimum verdict under the named contract. No
//! certificate means `unverified`, and `unverified` never satisfies a minimum
//! above it — absence of evidence is not evidence.
//!
//! `Observe` and `Verify` do not gate anything. They differ in whether the
//! obligations were evaluated at all, and the difference is visible in the
//! verdict rather than hidden in the runtime's behaviour.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::correctness::certificate::{
    AssuranceVerdict, Certificate, CheckerVerdict, VerifierRecord,
};
use crate::correctness::evidence::RecordedTime;
use crate::correctness::obligation::{DischargeState, Obligation};
use crate::coverage::{Coverage, coverage};
use crate::definition::{AssuranceMode, Definition};
use crate::digest::Digest;
use crate::id::Identifier;
use crate::port::TrustLevel;
use crate::trust::TrustClass;

impl AssuranceVerdict {
    /// How strong this verdict is.
    ///
    /// Rejected is weaker than unverified on purpose: "a premise failed" is
    /// worse news than "nobody looked", and a minimum that unverified fails to
    /// meet must not be met by a rejection.
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Rejected => 0,
            Self::Unverified => 1,
            Self::Conditional => 2,
            Self::Accepted => 3,
        }
    }

    /// Whether this verdict meets a minimum.
    #[must_use]
    pub const fn satisfies(self, minimum: Self) -> bool {
        self.rank() >= minimum.rank()
    }

    /// The verdict a mode permits, given the obligations.
    ///
    /// `Observe` always concludes `unverified`, whatever the obligations say,
    /// because in observe mode nothing was required to be checked and a verdict
    /// derived from an optional subset would overstate what is known.
    #[must_use]
    pub fn under_mode(mode: AssuranceMode, obligations: &[Obligation]) -> Self {
        if mode.evaluates_obligations() {
            Self::from_obligations(obligations)
        } else {
            Self::Unverified
        }
    }

    /// The verdict one contract's obligations justify.
    ///
    /// A boundary asks about a particular property, and the run's other
    /// properties are not evidence about it either way. An obligation of some
    /// other contract that nobody got round to deciding says nothing here, and
    /// letting it drag this verdict down produces a *false denial* — safe in
    /// direction, but it pushes an operator to lower the boundary's minimum to
    /// get unrelated work through, trading a precise gate for a blunt one.
    ///
    /// A failure is treated differently, and the asymmetry is deliberate. An
    /// undecided obligation is an absence of information. An obligation that was
    /// checked and did not hold is information: the run produced something known
    /// to be wrong, and releasing an effect from it on the strength of an
    /// unrelated contract is exactly the passing check of the wrong property
    /// this layer exists to refuse. So residuals are scoped to their contract
    /// and failures are not.
    ///
    /// A contract the certificate says nothing about is `Unverified`, never
    /// `Accepted`. "No obligations of this contract" must not read as "every
    /// obligation of it was discharged" — that vacuous truth is the shape of the
    /// hole [`crate::coverage`] closed.
    #[must_use]
    pub fn for_contract(
        mode: AssuranceMode,
        contract: &Identifier,
        obligations: &[Obligation],
    ) -> Self {
        if !mode.evaluates_obligations() {
            return Self::Unverified;
        }
        if obligations.iter().any(Obligation::has_failed) {
            return Self::Rejected;
        }

        let mut seen = false;
        let mut outstanding = false;
        for obligation in obligations
            .iter()
            .filter(|obligation| &obligation.contract == contract)
        {
            seen = true;
            outstanding |= obligation.is_outstanding();
        }

        match (seen, outstanding) {
            (false, _) => Self::Unverified,
            (true, true) => Self::Conditional,
            (true, false) => Self::Accepted,
        }
    }
}

/// When a boundary decision is being made, and what has been withdrawn.
///
/// Both are arguments rather than things `decide_boundary` reaches for. A clock
/// read inside the decision would make it unreplayable, which is the same reason
/// the runtime's decision core has none; a revocation list fetched inside it
/// would make it depend on a service being up. The caller holds both, and the
/// decision stays a pure function of what it was given.
#[derive(Debug, Clone, Copy)]
pub struct DecisionContext<'a> {
    /// The moment the crossing is being decided.
    pub now: RecordedTime,
    /// Certificates that have been withdrawn, by their own digest.
    pub revoked: &'a [Digest],
}

impl<'a> DecisionContext<'a> {
    /// A decision at `now`, with nothing revoked.
    #[must_use]
    pub const fn at(now: RecordedTime) -> Self {
        Self { now, revoked: &[] }
    }

    /// The same decision, against a revocation list.
    #[must_use]
    pub const fn revoking(self, revoked: &'a [Digest]) -> Self {
        Self {
            now: self.now,
            revoked,
        }
    }
}

/// What a policy demands of a verifier.
///
/// A bare name was not enough, and the gap it left was the quiet kind: the gate
/// asked only whether a record with the right name was present, so a scanner
/// that ran, concluded `rejected`, and recorded that faithfully *satisfied* a
/// policy requiring the scanner. The requirement is now about the run, not the
/// roll call.
///
/// `version` and `environment` select which records count; `minimum` is what
/// every selected record has to have concluded. Both selectors are optional
/// because pinning them is a real cost — a policy that pins a version has to be
/// edited whenever the tool moves — and a policy that pins neither is still
/// stronger than a name, because the verdict is checked either way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifierRequirement {
    pub name: Identifier,
    /// The exact version required, when the policy pins one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The environment digest required, when the policy pins one.
    ///
    /// An environment is what makes a deterministic verifier's word
    /// reproducible; the same tool at the same version in a different image is
    /// not the same check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<Digest>,
    /// The weakest conclusion that counts as this verifier having passed.
    pub minimum: AssuranceVerdict,
}

impl VerifierRequirement {
    /// A requirement that the named verifier ran and did not reject.
    ///
    /// The default minimum is `Conditional`, which is exactly the gap this type
    /// closes and no more: a verifier that ran, concluded `rejected`, and said so
    /// no longer counts as having satisfied the policy. It is deliberately not
    /// `Accepted`. A verifier that returns `conditional` has done its job and
    /// reported a qualified result; that qualification already flows into the
    /// certificate's verdict, and the boundary's own `minimum` is where it
    /// belongs. Demanding `Accepted` here as well would deny the same crossing
    /// twice and report the less informative of the two reasons.
    ///
    /// A policy that wants a particular verifier to have accepted outright can
    /// set `minimum` itself.
    #[must_use]
    pub const fn named(name: Identifier) -> Self {
        Self {
            name,
            version: None,
            environment: None,
            minimum: AssuranceVerdict::Conditional,
        }
    }
}

/// What a policy demands before one boundary may be crossed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundaryPolicy {
    /// The weakest verdict that may cross.
    pub minimum: AssuranceVerdict,
    /// The contract the verdict must have been reached under. `None` accepts
    /// any contract, which is rarely what a protected boundary wants.
    pub contract: Option<Identifier>,
    /// An obligation that must be discharged, used for human approvals.
    pub requires_approval: Option<Identifier>,
    /// How stale the evidence behind a certificate may be, in milliseconds.
    ///
    /// Dated by the *newest* evidence the certificate carries: the moment its
    /// picture of the world was last refreshed. Not by when it was sealed —
    /// there is no such field, and it would be the wrong question anyway, since
    /// re-sealing old evidence would make a stale certificate look new.
    ///
    /// A certificate carrying no evidence cannot be dated, so it cannot be shown
    /// to be fresh, so it does not cross a boundary that asks for freshness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age_ms: Option<i64>,
}

/// Which values may reach a named destination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustRoute {
    /// The memory space, downstream node, or consumer being protected.
    pub into: Identifier,
    pub minimum: TrustLevel,
    pub contract: Option<Identifier>,
}

/// A versioned assurance policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssurancePolicy {
    pub id: Identifier,
    pub version: String,
    /// The mode this policy imposes. The effective mode is the stricter of this
    /// and the definition's own, so a policy can tighten a definition but a
    /// definition cannot loosen a policy.
    pub mode: AssuranceMode,
    pub required_contracts: Vec<Identifier>,
    pub required_verifiers: Vec<VerifierRequirement>,
    pub boundaries: BTreeMap<Identifier, BoundaryPolicy>,
    /// Who may waive an obligation. A waiver by anyone else is not a waiver.
    pub waiver_authorities: Vec<Identifier>,
    pub trust_routes: Vec<TrustRoute>,
}

impl AssurancePolicy {
    /// The mode actually in force for a definition.
    #[must_use]
    pub fn effective_mode(&self, declared: AssuranceMode) -> AssuranceMode {
        self.mode.strictest(declared)
    }

    /// The policy for a boundary, if this policy governs it.
    #[must_use]
    pub fn boundary(&self, id: &Identifier) -> Option<&BoundaryPolicy> {
        self.boundaries.get(id)
    }
}

/// Why a boundary may not be crossed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum DenialReason {
    /// Nothing checked this run.
    NoCertificate {
        required: AssuranceVerdict,
    },
    VerdictBelowMinimum {
        required: AssuranceVerdict,
        found: AssuranceVerdict,
    },
    /// The certificate does not account for every obligation the required
    /// contract declares. `missing` names the ones it says nothing about,
    /// computed from the definition rather than read off the list the
    /// certificate wrote about itself.
    ContractNotCovered {
        required: Identifier,
        missing: Vec<Identifier>,
    },
    /// The policy requires a contract the definition does not declare. Nothing
    /// can cover it, so nothing may cross on the strength of it.
    ContractNotInDefinition {
        required: Identifier,
    },
    /// A value's trust was established under a different contract than the
    /// destination requires. A passing check of the wrong property is not a
    /// pass.
    ContractMismatch {
        required: Identifier,
        established: Option<Identifier>,
    },
    /// No verifier of that name ran at all.
    MissingVerifier {
        identity: Identifier,
    },
    /// It ran, and did not reach the conclusion the policy required.
    VerifierDidNotPass {
        identity: Identifier,
        required: AssuranceVerdict,
        found: CheckerVerdict,
    },
    /// It ran, at a version the policy does not accept.
    VerifierVersionMismatch {
        identity: Identifier,
        required: String,
        found: Vec<String>,
    },
    /// It ran, somewhere the policy does not accept.
    VerifierEnvironmentMismatch {
        identity: Identifier,
        required: Digest,
        found: Vec<Digest>,
    },
    MissingApproval {
        obligation: Identifier,
    },
    /// Someone waived an obligation who was not authorised to.
    WaiverNotAuthorised {
        obligation: Identifier,
        authority: Identifier,
    },
    /// The evidence behind the certificate is older than the boundary allows.
    CertificateStale {
        age_ms: i64,
        max_age_ms: i64,
    },
    /// The boundary asks for freshness and the certificate carries no evidence,
    /// so there is nothing to date it by. Fails closed: undatable is not fresh.
    CertificateUndatable,
    /// The certificate has been withdrawn.
    CertificateRevoked {
        certificate: Digest,
    },
    /// The certificate is about a different definition than the one being run.
    CertificateNotForThisDefinition,
    /// The policy says nothing about this boundary, and a protected boundary
    /// with no policy is not implicitly open.
    BoundaryNotGoverned {
        boundary: Identifier,
    },
}

impl DenialReason {
    /// A short name for messages and metrics.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::NoCertificate { .. } => "no_certificate",
            Self::VerdictBelowMinimum { .. } => "verdict_below_minimum",
            Self::ContractNotCovered { .. } => "contract_not_covered",
            Self::ContractNotInDefinition { .. } => "contract_not_in_definition",
            Self::ContractMismatch { .. } => "contract_mismatch",
            Self::MissingVerifier { .. } => "missing_verifier",
            Self::VerifierDidNotPass { .. } => "verifier_did_not_pass",
            Self::VerifierVersionMismatch { .. } => "verifier_version_mismatch",
            Self::VerifierEnvironmentMismatch { .. } => "verifier_environment_mismatch",
            Self::MissingApproval { .. } => "missing_approval",
            Self::WaiverNotAuthorised { .. } => "waiver_not_authorised",
            Self::CertificateStale { .. } => "certificate_stale",
            Self::CertificateUndatable => "certificate_undatable",
            Self::CertificateRevoked { .. } => "certificate_revoked",
            Self::CertificateNotForThisDefinition => "certificate_not_for_this_definition",
            Self::BoundaryNotGoverned { .. } => "boundary_not_governed",
        }
    }
}

/// What a policy decided about a boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum BoundaryDecision {
    /// The boundary may be crossed, under the recorded verdict.
    Allowed {
        verdict: AssuranceVerdict,
    },
    /// The mode does not gate anything, so the crossing proceeds and the
    /// verdict is recorded for whoever reads the run later. This is a distinct
    /// answer from `Allowed`: nothing was enforced, and saying so is the point.
    NotEnforced {
        verdict: AssuranceVerdict,
        mode: AssuranceMode,
    },
    Denied {
        reason: DenialReason,
    },
}

impl BoundaryDecision {
    /// Whether the crossing may proceed.
    #[must_use]
    pub const fn permits_crossing(&self) -> bool {
        matches!(self, Self::Allowed { .. } | Self::NotEnforced { .. })
    }

    /// Whether a policy actually gated this crossing.
    #[must_use]
    pub const fn was_enforced(&self) -> bool {
        matches!(self, Self::Allowed { .. } | Self::Denied { .. })
    }
}

/// Decides whether a boundary may be crossed.
///
/// Pure and total: the same inputs give the same answer anywhere, which is what
/// lets the API, a worker, and the CLI share one rule instead of three.
///
/// `certificate` is an `Option` on purpose. The absent case is the one that
/// matters most, and making callers pass it explicitly keeps "we did not check"
/// from being indistinguishable from "we checked and it was fine".
///
/// `definition` is the whole definition rather than its digest, because a
/// contract's obligations live on it and coverage cannot be computed without
/// them. Passing only the digest is what forced the old gate to accept the
/// certificate's own word about which contracts it covered.
#[must_use]
pub fn decide_boundary(
    policy: &AssurancePolicy,
    declared_mode: AssuranceMode,
    definition: &Definition,
    certificate: Option<&Certificate>,
    boundary: &Identifier,
    context: DecisionContext<'_>,
) -> BoundaryDecision {
    let mode = policy.effective_mode(declared_mode);
    let verdict = certificate.map_or(AssuranceVerdict::Unverified, Certificate::verdict);

    if !mode.enforces_boundaries() {
        return BoundaryDecision::NotEnforced { verdict, mode };
    }

    let Some(required) = policy.boundary(boundary) else {
        return BoundaryDecision::Denied {
            reason: DenialReason::BoundaryNotGoverned {
                boundary: boundary.clone(),
            },
        };
    };

    let Some(certificate) = certificate else {
        return BoundaryDecision::Denied {
            reason: DenialReason::NoCertificate {
                required: required.minimum,
            },
        };
    };
    let body = certificate.body();

    // A definition that will not canonically encode cannot be shown to be the
    // one this certificate is about, so identity is not established and the
    // boundary stays shut. `admit` encodes first, so an admitted definition
    // never reaches the error arm.
    let Ok(digest) = crate::digest_of(definition) else {
        return BoundaryDecision::Denied {
            reason: DenialReason::CertificateNotForThisDefinition,
        };
    };
    if body.subject.definition != digest {
        return BoundaryDecision::Denied {
            reason: DenialReason::CertificateNotForThisDefinition,
        };
    }

    // Withdrawal outranks everything the certificate says about itself. A
    // revoked certificate may be perfectly well formed and still name evidence
    // nobody should be relying on any more, which is the case deleting it — the
    // only remedy there used to be — would have left no record of.
    if context.revoked.contains(certificate.replay_digest()) {
        return BoundaryDecision::Denied {
            reason: DenialReason::CertificateRevoked {
                certificate: *certificate.replay_digest(),
            },
        };
    }

    if let Some(max_age_ms) = required.max_age_ms {
        let Some(dated_at) = newest_evidence(body) else {
            return BoundaryDecision::Denied {
                reason: DenialReason::CertificateUndatable,
            };
        };
        let age_ms = context.now.epoch_millis() - dated_at.epoch_millis();
        if age_ms > max_age_ms {
            return BoundaryDecision::Denied {
                reason: DenialReason::CertificateStale { age_ms, max_age_ms },
            };
        }
    }

    // Every contract this crossing depends on: the one the boundary names, and
    // the ones the policy requires of every crossing it governs. The second was
    // declared and read by nothing at all.
    let required_contracts = required.contract.iter().chain(&policy.required_contracts);
    for contract in required_contracts {
        if let Some(reason) = uncovered_reason(definition, contract, body) {
            return BoundaryDecision::Denied { reason };
        }
    }

    for requirement in &policy.required_verifiers {
        if let Some(reason) = unmet_verifier(requirement, &body.verifiers) {
            return BoundaryDecision::Denied { reason };
        }
    }

    // A waiver is only a waiver if the policy said that authority may grant it.
    for obligation in &body.obligations {
        if let DischargeState::Waived { authority, .. } = &obligation.state
            && !policy.waiver_authorities.contains(authority)
        {
            return BoundaryDecision::Denied {
                reason: DenialReason::WaiverNotAuthorised {
                    obligation: obligation.statement.id.clone(),
                    authority: authority.clone(),
                },
            };
        }
    }

    if let Some(approval) = &required.requires_approval {
        let granted = body.obligations.iter().any(|obligation| {
            &obligation.statement.id == approval
                && matches!(obligation.state, DischargeState::Discharged { .. })
        });
        if !granted {
            return BoundaryDecision::Denied {
                reason: DenialReason::MissingApproval {
                    obligation: approval.clone(),
                },
            };
        }
    }

    // Judged on the contract this boundary is about, under the mode the
    // certificate was decided in — so the gate and the certificate cannot
    // disagree about the same run. A boundary naming no contract is judged on
    // the run as a whole, which is all there is to go on.
    let gated = match &required.contract {
        Some(contract) => AssuranceVerdict::for_contract(body.mode, contract, &body.obligations),
        None => verdict,
    };

    if gated.satisfies(required.minimum) {
        BoundaryDecision::Allowed { verdict: gated }
    } else {
        BoundaryDecision::Denied {
            reason: DenialReason::VerdictBelowMinimum {
                required: required.minimum,
                found: gated,
            },
        }
    }
}

/// When the certificate last looked at the world, if it can be dated.
///
/// The newest piece of evidence, because that is the most recent moment any of
/// this was observed. An older piece alongside it is not staleness — a contract
/// signed years ago is exactly as true as it was — so the oldest would refuse
/// perfectly good citations of durable documents.
fn newest_evidence(
    body: &crate::correctness::certificate::CertificateBody,
) -> Option<RecordedTime> {
    body.evidence
        .iter()
        .map(|evidence| evidence.captured_at)
        .max_by_key(|recorded| recorded.epoch_millis())
}

/// The reason a verifier requirement is not met, if it is not.
///
/// Selection then judgement: the name, then the pinned version, then the pinned
/// environment narrow the records down, and every record still standing has to
/// have reached the minimum. All of them, not one — a verifier that ran twice
/// and rejected once has rejected, and picking the run that agrees with you is
/// the whole failure mode.
fn unmet_verifier(
    requirement: &VerifierRequirement,
    records: &[VerifierRecord],
) -> Option<DenialReason> {
    let named: Vec<&VerifierRecord> = records
        .iter()
        .filter(|record| record.identity.name == requirement.name)
        .collect();
    if named.is_empty() {
        return Some(DenialReason::MissingVerifier {
            identity: requirement.name.clone(),
        });
    }

    let at_version = match &requirement.version {
        None => named,
        Some(version) => {
            let matching: Vec<&VerifierRecord> = named
                .iter()
                .copied()
                .filter(|record| &record.identity.version == version)
                .collect();
            if matching.is_empty() {
                return Some(DenialReason::VerifierVersionMismatch {
                    identity: requirement.name.clone(),
                    required: version.clone(),
                    found: named
                        .iter()
                        .map(|record| record.identity.version.clone())
                        .collect(),
                });
            }
            matching
        }
    };

    let selected = match &requirement.environment {
        None => at_version,
        Some(environment) => {
            let matching: Vec<&VerifierRecord> = at_version
                .iter()
                .copied()
                .filter(|record| &record.environment == environment)
                .collect();
            if matching.is_empty() {
                return Some(DenialReason::VerifierEnvironmentMismatch {
                    identity: requirement.name.clone(),
                    required: *environment,
                    found: at_version.iter().map(|record| record.environment).collect(),
                });
            }
            matching
        }
    };

    selected
        .iter()
        .find(|record| !AssuranceVerdict::from(record.verdict).satisfies(requirement.minimum))
        .map(|record| DenialReason::VerifierDidNotPass {
            identity: requirement.name.clone(),
            required: requirement.minimum,
            found: record.verdict,
        })
}

/// The reason a contract is not covered, if it is not.
fn uncovered_reason(
    definition: &Definition,
    contract: &Identifier,
    body: &crate::correctness::certificate::CertificateBody,
) -> Option<DenialReason> {
    match coverage(definition, contract, body) {
        Coverage::Complete => None,
        Coverage::ContractNotInDefinition => Some(DenialReason::ContractNotInDefinition {
            required: contract.clone(),
        }),
        Coverage::Missing(missing) => Some(DenialReason::ContractNotCovered {
            required: contract.clone(),
            missing,
        }),
    }
}

/// Decides whether a value of this trust class may reach a destination.
///
/// The same rule as a boundary, applied to a value rather than an effect: a
/// governed memory space or a protected downstream node states what it accepts,
/// and an unverified value does not become acceptable by arriving.
///
/// # Errors
///
/// Returns the [`DenialReason`] when the value may not be routed there.
pub fn check_trust_route(
    policy: &AssurancePolicy,
    into: &Identifier,
    class: &TrustClass,
) -> Result<(), DenialReason> {
    let Some(route) = policy.trust_routes.iter().find(|route| &route.into == into) else {
        // Nothing is protected here, so nothing is being bypassed.
        return Ok(());
    };

    if TrustLevel::of(class) < route.minimum {
        return Err(DenialReason::VerdictBelowMinimum {
            required: match route.minimum {
                TrustLevel::Verified => AssuranceVerdict::Accepted,
                TrustLevel::Conditional => AssuranceVerdict::Conditional,
                TrustLevel::Unverified => AssuranceVerdict::Unverified,
            },
            found: match TrustLevel::of(class) {
                TrustLevel::Verified => AssuranceVerdict::Accepted,
                TrustLevel::Conditional => AssuranceVerdict::Conditional,
                TrustLevel::Unverified => AssuranceVerdict::Unverified,
            },
        });
    }

    if let Some(required) = &route.contract {
        let covered = class
            .contract()
            .is_some_and(|contract| contract == required.as_str());
        if !covered {
            return Err(DenialReason::ContractMismatch {
                required: required.clone(),
                established: class
                    .contract()
                    .and_then(|contract| Identifier::parse(contract).ok()),
            });
        }
    }

    Ok(())
}
