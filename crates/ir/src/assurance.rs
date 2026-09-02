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

use crate::correctness::certificate::{AssuranceVerdict, Certificate};
use crate::correctness::obligation::{DischargeState, Obligation};
use crate::definition::AssuranceMode;
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
    pub required_verifiers: Vec<Identifier>,
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
    /// The verdict was reached under a different contract than the one this
    /// boundary is about. A passing check of the wrong property is not a pass.
    ContractNotCovered {
        required: Identifier,
        covered: Vec<Identifier>,
    },
    MissingVerifier {
        identity: Identifier,
    },
    MissingApproval {
        obligation: Identifier,
    },
    /// Someone waived an obligation who was not authorised to.
    WaiverNotAuthorised {
        obligation: Identifier,
        authority: Identifier,
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
            Self::MissingVerifier { .. } => "missing_verifier",
            Self::MissingApproval { .. } => "missing_approval",
            Self::WaiverNotAuthorised { .. } => "waiver_not_authorised",
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
#[must_use]
pub fn decide_boundary(
    policy: &AssurancePolicy,
    declared_mode: AssuranceMode,
    definition: &crate::digest::Digest,
    certificate: Option<&Certificate>,
    boundary: &Identifier,
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

    if &body.subject.definition != definition {
        return BoundaryDecision::Denied {
            reason: DenialReason::CertificateNotForThisDefinition,
        };
    }

    if let Some(contract) = &required.contract
        && !body.contracts.contains(contract)
    {
        return BoundaryDecision::Denied {
            reason: DenialReason::ContractNotCovered {
                required: contract.clone(),
                covered: body.contracts.clone(),
            },
        };
    }

    for identity in &policy.required_verifiers {
        if !body
            .verifiers
            .iter()
            .any(|record| &record.identity.name == identity)
        {
            return BoundaryDecision::Denied {
                reason: DenialReason::MissingVerifier {
                    identity: identity.clone(),
                },
            };
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

    if verdict.satisfies(required.minimum) {
        BoundaryDecision::Allowed { verdict }
    } else {
        BoundaryDecision::Denied {
            reason: DenialReason::VerdictBelowMinimum {
                required: required.minimum,
                found: verdict,
            },
        }
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
            return Err(DenialReason::ContractNotCovered {
                required: required.clone(),
                covered: class
                    .contract()
                    .and_then(|contract| Identifier::parse(contract).ok())
                    .into_iter()
                    .collect(),
            });
        }
    }

    Ok(())
}
