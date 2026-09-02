//! Observe reports, Verify reports a verdict, Enforce stops things.

mod fixtures;

use std::collections::BTreeMap;

use capsulet_ir::assurance::{BoundaryDecision, BoundaryPolicy, DenialReason, TrustRoute};
use capsulet_ir::correctness::certificate::{Subject, VerifierRecord, VerifierTrust};
use capsulet_ir::correctness::obligation::{DischargeState, ObligationStatement, RepairOwner};
use capsulet_ir::correctness::proposal::{Producer, ProducerKind};
use capsulet_ir::correctness::{Certificate, CertificateBody, EvidenceRef};
use capsulet_ir::trust::{RawVerificationRecord, RecordVerdict};
use capsulet_ir::{
    AssuranceMode, AssurancePolicy, AssuranceVerdict, CheckerVerdict, Digest, Identity, Obligation,
    RecordedTime, TrustClass, TrustLevel, VerificationRecord, admit, check_trust_route,
    decide_boundary,
};

use fixtures::{definition_in, id};

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
        captured_at: RecordedTime(1_772_000_000_000),
    }
}

fn obligation(name: &str, state: DischargeState) -> Obligation {
    Obligation {
        statement: ObligationStatement {
            id: id(name),
            statement: format!("`{name}` holds"),
            owner: RepairOwner::Verifier,
        },
        contract: id("patch-compiles"),
        state,
    }
}

fn discharged(name: &str) -> Obligation {
    obligation(
        name,
        DischargeState::Discharged {
            by: id("cargo-test"),
            evidence: vec![evidence().content],
        },
    )
}

fn certificate(mode: AssuranceMode, obligations: Vec<Obligation>) -> Certificate {
    let definition = definition_in(mode);
    let admission = admit(&definition).expect("the fixture definition is admitted");
    let verdict = AssuranceVerdict::under_mode(mode, &obligations);

    Certificate::seal(CertificateBody {
        schema_version: Certificate::current_schema_version(),
        id: id("cert-1"),
        admission: admission.clone(),
        mode,
        subject: Subject {
            definition: *admission.definition(),
            definition_version: "1".to_string(),
            run: Some(id("run-1")),
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
    .expect("the certificate seals")
}

fn policy(minimum: AssuranceVerdict, mode: AssuranceMode) -> AssurancePolicy {
    let mut boundaries = BTreeMap::new();
    boundaries.insert(
        id("publish-boundary"),
        BoundaryPolicy {
            minimum,
            contract: Some(id("patch-compiles")),
            requires_approval: None,
        },
    );
    AssurancePolicy {
        id: id("release-policy"),
        version: "3".to_string(),
        mode,
        required_contracts: vec![id("patch-compiles")],
        required_verifiers: vec![id("cargo-test")],
        boundaries,
        waiver_authorities: vec![id("platform-admin")],
        trust_routes: vec![],
    }
}

fn definition_digest(mode: AssuranceMode) -> Digest {
    *admit(&definition_in(mode))
        .expect("the fixture definition is admitted")
        .definition()
}

#[test]
fn observe_never_reaches_accepted_however_well_the_run_went() {
    let observed = certificate(AssuranceMode::Observe, vec![discharged("compiles")]);

    // The obligations were all discharged, and the verdict is still unverified,
    // because observe mode never required them to be checked.
    assert_eq!(observed.verdict(), AssuranceVerdict::Unverified);
    assert_eq!(
        AssuranceVerdict::under_mode(AssuranceMode::Verify, &[discharged("compiles")]),
        AssuranceVerdict::Accepted
    );
}

#[test]
fn verify_reports_a_verdict_and_blocks_nothing() {
    let verified = certificate(AssuranceMode::Verify, vec![discharged("compiles")]);
    let decision = decide_boundary(
        &policy(AssuranceVerdict::Accepted, AssuranceMode::Verify),
        AssuranceMode::Verify,
        &definition_digest(AssuranceMode::Verify),
        Some(&verified),
        &id("publish-boundary"),
    );

    assert!(decision.permits_crossing());
    assert!(
        !decision.was_enforced(),
        "verify records a verdict; it does not gate"
    );
    assert_eq!(
        decision,
        BoundaryDecision::NotEnforced {
            verdict: AssuranceVerdict::Accepted,
            mode: AssuranceMode::Verify,
        }
    );
}

#[test]
fn enforce_allows_a_crossing_that_meets_the_minimum() {
    let enforced = certificate(AssuranceMode::Enforce, vec![discharged("compiles")]);
    let decision = decide_boundary(
        &policy(AssuranceVerdict::Accepted, AssuranceMode::Enforce),
        AssuranceMode::Enforce,
        &definition_digest(AssuranceMode::Enforce),
        Some(&enforced),
        &id("publish-boundary"),
    );

    assert_eq!(
        decision,
        BoundaryDecision::Allowed {
            verdict: AssuranceVerdict::Accepted
        }
    );
    assert!(decision.was_enforced());
}

#[test]
fn enforce_denies_a_verdict_below_the_minimum() {
    let conditional = certificate(
        AssuranceMode::Enforce,
        vec![
            discharged("compiles"),
            obligation(
                "summary-is-faithful",
                DischargeState::Residual {
                    rationale: "nobody can decide this mechanically".to_string(),
                    evidence: vec![],
                },
            ),
        ],
    );
    assert_eq!(conditional.verdict(), AssuranceVerdict::Conditional);

    let decision = decide_boundary(
        &policy(AssuranceVerdict::Accepted, AssuranceMode::Enforce),
        AssuranceMode::Enforce,
        &definition_digest(AssuranceMode::Enforce),
        Some(&conditional),
        &id("publish-boundary"),
    );

    assert_eq!(
        decision,
        BoundaryDecision::Denied {
            reason: DenialReason::VerdictBelowMinimum {
                required: AssuranceVerdict::Accepted,
                found: AssuranceVerdict::Conditional,
            }
        }
    );
    assert!(!decision.permits_crossing());
}

#[test]
fn an_absent_certificate_is_unverified_and_never_satisfies_a_minimum() {
    for minimum in [AssuranceVerdict::Conditional, AssuranceVerdict::Accepted] {
        let decision = decide_boundary(
            &policy(minimum, AssuranceMode::Enforce),
            AssuranceMode::Enforce,
            &definition_digest(AssuranceMode::Enforce),
            None,
            &id("publish-boundary"),
        );
        assert_eq!(
            decision,
            BoundaryDecision::Denied {
                reason: DenialReason::NoCertificate { required: minimum }
            }
        );
    }

    // Unverified satisfies only a minimum of unverified, and a rejection does
    // not even do that.
    assert!(AssuranceVerdict::Unverified.satisfies(AssuranceVerdict::Unverified));
    assert!(!AssuranceVerdict::Unverified.satisfies(AssuranceVerdict::Conditional));
    assert!(!AssuranceVerdict::Rejected.satisfies(AssuranceVerdict::Unverified));
}

#[test]
fn a_waiver_by_an_unauthorised_party_is_not_a_waiver() {
    let waived = certificate(
        AssuranceMode::Enforce,
        vec![
            discharged("compiles"),
            obligation(
                "licence-review",
                DischargeState::Waived {
                    policy: id("release-policy"),
                    authority: id("a-passing-colleague"),
                },
            ),
        ],
    );

    let decision = decide_boundary(
        &policy(AssuranceVerdict::Conditional, AssuranceMode::Enforce),
        AssuranceMode::Enforce,
        &definition_digest(AssuranceMode::Enforce),
        Some(&waived),
        &id("publish-boundary"),
    );

    assert_eq!(
        decision,
        BoundaryDecision::Denied {
            reason: DenialReason::WaiverNotAuthorised {
                obligation: id("licence-review"),
                authority: id("a-passing-colleague"),
            }
        }
    );
}

#[test]
fn a_waiver_by_a_named_authority_stands() {
    let waived = certificate(
        AssuranceMode::Enforce,
        vec![
            discharged("compiles"),
            obligation(
                "licence-review",
                DischargeState::Waived {
                    policy: id("release-policy"),
                    authority: id("platform-admin"),
                },
            ),
        ],
    );

    let decision = decide_boundary(
        &policy(AssuranceVerdict::Conditional, AssuranceMode::Enforce),
        AssuranceMode::Enforce,
        &definition_digest(AssuranceMode::Enforce),
        Some(&waived),
        &id("publish-boundary"),
    );

    assert_eq!(
        decision,
        BoundaryDecision::Allowed {
            verdict: AssuranceVerdict::Conditional
        }
    );
}

#[test]
fn a_boundary_no_policy_governs_is_not_implicitly_open() {
    let enforced = certificate(AssuranceMode::Enforce, vec![discharged("compiles")]);
    let decision = decide_boundary(
        &policy(AssuranceVerdict::Accepted, AssuranceMode::Enforce),
        AssuranceMode::Enforce,
        &definition_digest(AssuranceMode::Enforce),
        Some(&enforced),
        &id("some-other-boundary"),
    );

    assert_eq!(
        decision,
        BoundaryDecision::Denied {
            reason: DenialReason::BoundaryNotGoverned {
                boundary: id("some-other-boundary")
            }
        }
    );
}

#[test]
fn a_certificate_for_a_different_definition_does_not_count() {
    let enforced = certificate(AssuranceMode::Enforce, vec![discharged("compiles")]);
    let decision = decide_boundary(
        &policy(AssuranceVerdict::Accepted, AssuranceMode::Enforce),
        AssuranceMode::Enforce,
        &Digest::of(b"a different definition entirely"),
        Some(&enforced),
        &id("publish-boundary"),
    );

    assert_eq!(
        decision,
        BoundaryDecision::Denied {
            reason: DenialReason::CertificateNotForThisDefinition
        }
    );
}

#[test]
fn a_required_verifier_that_did_not_run_denies_the_crossing() {
    let enforced = certificate(AssuranceMode::Enforce, vec![discharged("compiles")]);
    let mut demanding = policy(AssuranceVerdict::Accepted, AssuranceMode::Enforce);
    demanding.required_verifiers.push(id("cargo-audit"));

    let decision = decide_boundary(
        &demanding,
        AssuranceMode::Enforce,
        &definition_digest(AssuranceMode::Enforce),
        Some(&enforced),
        &id("publish-boundary"),
    );

    assert_eq!(
        decision,
        BoundaryDecision::Denied {
            reason: DenialReason::MissingVerifier {
                identity: id("cargo-audit")
            }
        }
    );
}

#[test]
fn a_required_approval_must_have_been_granted() {
    let enforced = certificate(AssuranceMode::Enforce, vec![discharged("compiles")]);
    let mut demanding = policy(AssuranceVerdict::Accepted, AssuranceMode::Enforce);
    demanding
        .boundaries
        .get_mut(&id("publish-boundary"))
        .expect("the boundary is governed")
        .requires_approval = Some(id("release-manager-approval"));

    let decision = decide_boundary(
        &demanding,
        AssuranceMode::Enforce,
        &definition_digest(AssuranceMode::Enforce),
        Some(&enforced),
        &id("publish-boundary"),
    );

    assert_eq!(
        decision,
        BoundaryDecision::Denied {
            reason: DenialReason::MissingApproval {
                obligation: id("release-manager-approval")
            }
        }
    );
}

#[test]
fn a_policy_may_tighten_a_definition_but_a_definition_may_not_loosen_a_policy() {
    let strict = policy(AssuranceVerdict::Accepted, AssuranceMode::Enforce);
    assert_eq!(
        strict.effective_mode(AssuranceMode::Observe),
        AssuranceMode::Enforce
    );

    let relaxed = policy(AssuranceVerdict::Accepted, AssuranceMode::Observe);
    assert_eq!(
        relaxed.effective_mode(AssuranceMode::Enforce),
        AssuranceMode::Enforce
    );
}

#[test]
fn a_protected_destination_refuses_a_value_that_did_not_earn_its_way_in() {
    let mut governed = policy(AssuranceVerdict::Accepted, AssuranceMode::Enforce);
    governed.trust_routes.push(TrustRoute {
        into: id("governed-memory"),
        minimum: TrustLevel::Verified,
        contract: Some(id("patch-compiles")),
    });

    assert_eq!(
        check_trust_route(&governed, &id("governed-memory"), &TrustClass::Unverified),
        Err(DenialReason::VerdictBelowMinimum {
            required: AssuranceVerdict::Accepted,
            found: AssuranceVerdict::Unverified,
        })
    );

    let record = VerificationRecord::admit(RawVerificationRecord {
        contract: "patch-compiles".to_string(),
        certificate: Digest::of(b"a certificate"),
        verdict: RecordVerdict::Accepted,
        residual_count: 0,
        provenance_complete: true,
    })
    .expect("the record is admitted");
    assert_eq!(
        check_trust_route(
            &governed,
            &id("governed-memory"),
            &TrustClass::from_record(&record)
        ),
        Ok(())
    );

    // Verified, but under a different contract: still not what this space asked
    // for.
    let elsewhere = VerificationRecord::admit(RawVerificationRecord {
        contract: "scanned-under-named-rules".to_string(),
        certificate: Digest::of(b"another certificate"),
        verdict: RecordVerdict::Accepted,
        residual_count: 0,
        provenance_complete: true,
    })
    .expect("the record is admitted");
    assert!(matches!(
        check_trust_route(
            &governed,
            &id("governed-memory"),
            &TrustClass::from_record(&elsewhere)
        ),
        Err(DenialReason::ContractNotCovered { .. })
    ));

    // A destination nothing protects lets anything through, and says so by
    // succeeding rather than by pretending a check happened.
    assert_eq!(
        check_trust_route(&governed, &id("scratch-space"), &TrustClass::Unverified),
        Ok(())
    );
}
