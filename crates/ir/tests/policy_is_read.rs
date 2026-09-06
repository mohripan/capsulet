//! Every field of an assurance policy, and what reads it.
//!
//! `required_contracts` sat on `AssurancePolicy` for a long time, was set by two
//! tests, and was read by no decision at all. Nothing failed. The tests that
//! mentioned it made it look covered, which is worse than the field not existing
//! — a policy author would reasonably believe they had required something.
//!
//! This file is the check that would have caught it. The destructuring below is
//! exhaustive on purpose: adding a field to `AssurancePolicy` stops this file
//! compiling, and the person adding it has to come here and say which decision
//! reads it, or state plainly that none does and why that is right.
//!
//! A field that changes no decision is not automatically a bug. An identifier is
//! there to be quoted back in a denial. But that has to be a decision somebody
//! made, not an omission nobody noticed.

mod fixtures;

use std::collections::BTreeMap;

use capsulet_ir::assurance::{
    BoundaryDecision, BoundaryPolicy, DecisionContext, DenialReason, TrustRoute,
    VerifierRequirement,
};
use capsulet_ir::correctness::certificate::{Subject, VerifierRecord, VerifierTrust};
use capsulet_ir::correctness::evidence::EvidenceRef;
use capsulet_ir::correctness::obligation::{DischargeState, ObligationStatement, RepairOwner};
use capsulet_ir::correctness::proposal::{Producer, ProducerKind};
use capsulet_ir::correctness::{Certificate, CertificateBody};
use capsulet_ir::port::TrustLevel;
use capsulet_ir::{
    AssuranceMode, AssurancePolicy, AssuranceVerdict, CheckerVerdict, Digest, Identity, Obligation,
    RecordedTime, TrustClass, admit, check_trust_route, decide_boundary,
};

use fixtures::{definition_in, id};

const CAPTURED_AT: i64 = 1_772_000_000_000;

fn now() -> DecisionContext<'static> {
    DecisionContext::at(RecordedTime(CAPTURED_AT))
}

fn evidence() -> EvidenceRef {
    let content = b"tests passed";
    EvidenceRef {
        id: id("test-log"),
        content: Digest::of(content),
        media_type: "text/plain".to_string(),
        byte_length: content.len() as u64,
        producer: Producer {
            kind: ProducerKind::Deterministic,
            identity: Identity::new(id("cargo-test"), "1.96"),
        },
        captured_at: RecordedTime(CAPTURED_AT),
    }
}

fn obligation(state: DischargeState) -> Obligation {
    Obligation {
        statement: ObligationStatement {
            id: id("compiles"),
            statement: "the patch compiles".to_string(),
            owner: RepairOwner::Verifier,
        },
        contract: id("patch-compiles"),
        state,
    }
}

fn certificate(obligations: Vec<Obligation>) -> Certificate {
    let mode = AssuranceMode::Enforce;
    let definition = definition_in(mode);
    let admission = admit(&definition).expect("admitted");
    let verdict = AssuranceVerdict::under_mode(mode, &obligations);

    Certificate::seal(CertificateBody {
        schema_version: Certificate::current_schema_version(),
        id: id("cert-policy"),
        admission: admission.clone(),
        mode,
        subject: Subject {
            definition: *admission.definition(),
            definition_version: "1".to_string(),
            run: Some(id("run-policy")),
            inputs: vec![],
            outputs: vec![],
        },
        policy_version: "release-policy/3".to_string(),
        kernel_version: "capsulet-kernel 0.1.0".to_string(),
        contracts: vec![id("patch-compiles")],
        verifiers: vec![VerifierRecord {
            identity: Identity::new(id("cargo-test"), "1.96"),
            environment: Digest::of(b"an image"),
            inputs: vec![],
            outputs: vec![],
            trust: VerifierTrust::Deterministic,
            verdict: CheckerVerdict::Accepted,
        }],
        evidence: vec![evidence()],
        obligations,
        loops: vec![],
        verdict,
    })
    .expect("seals")
}

fn discharged() -> Certificate {
    certificate(vec![obligation(DischargeState::Discharged {
        by: id("cargo-test"),
        evidence: vec![evidence().content],
    })])
}

fn governing() -> AssurancePolicy {
    AssurancePolicy {
        id: id("release-policy"),
        version: "3".to_string(),
        mode: AssuranceMode::Enforce,
        required_contracts: vec![],
        required_verifiers: vec![],
        boundaries: BTreeMap::from([(
            id("publish-boundary"),
            BoundaryPolicy {
                minimum: AssuranceVerdict::Accepted,
                contract: Some(id("patch-compiles")),
                requires_approval: None,
                max_age_ms: None,
            },
        )]),
        waiver_authorities: vec![],
        trust_routes: vec![],
    }
}

fn cross(policy: &AssurancePolicy, certificate: &Certificate) -> BoundaryDecision {
    cross_declaring(policy, AssuranceMode::Enforce, certificate)
}

fn cross_declaring(
    policy: &AssurancePolicy,
    declared: AssuranceMode,
    certificate: &Certificate,
) -> BoundaryDecision {
    decide_boundary(
        policy,
        declared,
        &definition_in(AssuranceMode::Enforce),
        Some(certificate),
        &id("publish-boundary"),
        now(),
    )
}

/// A certificate that covers the contract and reaches only `conditional`, for
/// isolating whatever a boundary asks about the verdict itself.
fn conditional() -> Certificate {
    certificate(vec![obligation(DischargeState::Residual {
        rationale: "nobody has checked this".to_string(),
        evidence: vec![],
    })])
}

#[test]
fn every_field_of_an_assurance_policy_is_accounted_for() {
    let policy = governing();

    // Exhaustive on purpose. If this stops compiling, a field was added: say
    // below which decision reads it, or why none should.
    let AssurancePolicy {
        id: _policy_id,
        version: _policy_version,
        mode,
        required_contracts,
        required_verifiers,
        boundaries,
        waiver_authorities,
        trust_routes,
    } = &policy;

    // --- Descriptive, by decision, not by omission -------------------------
    //
    // `id` names the policy so a denial can be traced back to the document that
    // caused it. Nothing about a crossing should turn on what a policy is
    // called.
    //
    // `version` is the honest gap. A certificate records the `policy_version` it
    // was decided under and the gate never compares the two, so a certificate
    // produced under an older policy crosses a boundary governed by a newer one
    // without anyone being told. That is `CAP-ASSURANCE-006` territory and is
    // tracked, not fixed here.

    // --- Read by a decision ------------------------------------------------

    // A policy's mode only ever *tightens* what a definition declared, so the
    // way to see it being read is a definition that declared observe: the
    // enforcing policy gates it, and an observing one leaves it alone.
    assert!(mode.enforces_boundaries());
    assert!(
        cross_declaring(&policy, AssuranceMode::Observe, &discharged()).was_enforced(),
        "mode is read: an enforcing policy tightens a definition that declared observe"
    );
    let mut observing = policy.clone();
    observing.mode = AssuranceMode::Observe;
    assert!(
        !cross_declaring(&observing, AssuranceMode::Observe, &discharged()).was_enforced(),
        "and an observing policy leaves it observing"
    );

    assert!(required_contracts.is_empty());
    let mut demanding_contract = policy.clone();
    demanding_contract.required_contracts = vec![id("scanned-under-named-rules")];
    assert!(
        matches!(
            cross(&demanding_contract, &discharged()),
            BoundaryDecision::Denied { .. }
        ),
        "required_contracts is read: this is the field that was dead"
    );

    assert!(required_verifiers.is_empty());
    let mut demanding_verifier = policy.clone();
    demanding_verifier.required_verifiers = vec![VerifierRequirement::named(id("cargo-audit"))];
    assert_eq!(
        cross(&demanding_verifier, &discharged()),
        BoundaryDecision::Denied {
            reason: DenialReason::MissingVerifier {
                identity: id("cargo-audit")
            }
        },
        "required_verifiers is read"
    );

    assert_eq!(boundaries.len(), 1);
    let mut ungoverned = policy.clone();
    ungoverned.boundaries.clear();
    assert!(
        matches!(
            cross(&ungoverned, &discharged()),
            BoundaryDecision::Denied {
                reason: DenialReason::BoundaryNotGoverned { .. }
            }
        ),
        "boundaries is read: an ungoverned boundary is not implicitly open"
    );

    assert!(waiver_authorities.is_empty());
    let waived = certificate(vec![obligation(DischargeState::Waived {
        policy: id("release-policy"),
        authority: id("platform-admin"),
    })]);
    assert!(
        matches!(
            cross(&policy, &waived),
            BoundaryDecision::Denied {
                reason: DenialReason::WaiverNotAuthorised { .. }
            }
        ),
        "waiver_authorities is read: an unlisted authority cannot waive"
    );
    let mut permitting = policy.clone();
    permitting.waiver_authorities = vec![id("platform-admin")];
    assert!(
        !matches!(
            cross(&permitting, &waived),
            BoundaryDecision::Denied {
                reason: DenialReason::WaiverNotAuthorised { .. }
            }
        ),
        "and a listed one can"
    );

    assert!(trust_routes.is_empty());
    let mut routed = policy.clone();
    routed.trust_routes = vec![TrustRoute {
        into: id("governed-memory"),
        minimum: TrustLevel::Verified,
        contract: Some(id("patch-compiles")),
    }];
    assert!(
        check_trust_route(&routed, &id("governed-memory"), &TrustClass::Unverified).is_err(),
        "trust_routes is read"
    );
}

#[test]
fn every_field_of_a_boundary_policy_is_accounted_for() {
    let policy = governing();
    let boundary = policy
        .boundary(&id("publish-boundary"))
        .expect("the fixture boundary");

    // Exhaustive, for the same reason as above.
    let BoundaryPolicy {
        minimum,
        contract,
        requires_approval,
        max_age_ms,
    } = boundary;

    // Isolated with a certificate that *covers* the contract and reaches only
    // `conditional`, so the only thing left to decide the crossing is the
    // minimum. An empty certificate would be denied for coverage first and would
    // say nothing about this field.
    assert_eq!(*minimum, AssuranceVerdict::Accepted);
    assert!(
        !cross(&policy, &conditional()).permits_crossing(),
        "accepted is not met by conditional"
    );
    let mut lax = policy.clone();
    lax.boundaries
        .get_mut(&id("publish-boundary"))
        .expect("boundary")
        .minimum = AssuranceVerdict::Conditional;
    assert!(
        cross(&lax, &conditional()).permits_crossing(),
        "minimum is read: lowering it lets the same certificate through"
    );

    assert_eq!(contract.as_ref(), Some(&id("patch-compiles")));
    let mut elsewhere = policy.clone();
    elsewhere
        .boundaries
        .get_mut(&id("publish-boundary"))
        .expect("boundary")
        .contract = Some(id("no-secrets-leaked"));
    assert!(
        matches!(
            cross(&elsewhere, &discharged()),
            BoundaryDecision::Denied {
                reason: DenialReason::ContractNotInDefinition { .. }
            }
        ),
        "contract is read"
    );

    assert!(requires_approval.is_none());
    let mut approving = policy.clone();
    approving
        .boundaries
        .get_mut(&id("publish-boundary"))
        .expect("boundary")
        .requires_approval = Some(id("a-human-said-yes"));
    assert!(
        matches!(
            cross(&approving, &discharged()),
            BoundaryDecision::Denied {
                reason: DenialReason::MissingApproval { .. }
            }
        ),
        "requires_approval is read"
    );

    assert!(max_age_ms.is_none());
    let mut hourly = policy.clone();
    hourly
        .boundaries
        .get_mut(&id("publish-boundary"))
        .expect("boundary")
        .max_age_ms = Some(3_600_000);
    assert!(
        matches!(
            decide_boundary(
                &hourly,
                AssuranceMode::Enforce,
                &definition_in(AssuranceMode::Enforce),
                Some(&discharged()),
                &id("publish-boundary"),
                DecisionContext::at(RecordedTime(CAPTURED_AT + 86_400_000)),
            ),
            BoundaryDecision::Denied {
                reason: DenialReason::CertificateStale { .. }
            }
        ),
        "max_age_ms is read"
    );
}
