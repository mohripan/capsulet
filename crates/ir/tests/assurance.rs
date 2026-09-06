//! Observe reports, Verify reports a verdict, Enforce stops things.

mod fixtures;

use std::collections::BTreeMap;

use capsulet_ir::assurance::{BoundaryDecision, BoundaryPolicy, DenialReason, TrustRoute};
use capsulet_ir::correctness::certificate::{Subject, VerifierRecord, VerifierTrust};
use capsulet_ir::correctness::obligation::{DischargeState, ObligationStatement, RepairOwner};
use capsulet_ir::correctness::proposal::{Producer, ProducerKind};
use capsulet_ir::correctness::{Certificate, CertificateBody, EvidenceRef};
use capsulet_ir::trust::{CertificateMap, Provenance, RawVerificationRecord};
use capsulet_ir::{
    AssuranceMode, AssurancePolicy, AssuranceVerdict, CheckerVerdict, Definition, Digest, Identity,
    Obligation, RecordedTime, TrustClass, TrustLevel, VerificationRecord, admit, check_trust_route,
    decide_boundary,
};

use fixtures::{definition_in, definition_with_scan, id};

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
    certificate_for(&definition_in(mode), mode, obligations)
}

/// An obligation of a contract other than the default.
fn obligation_of(name: &str, contract: &str) -> Obligation {
    Obligation {
        contract: id(contract),
        ..discharged(name)
    }
}

fn certificate_for(
    definition: &Definition,
    mode: AssuranceMode,
    obligations: Vec<Obligation>,
) -> Certificate {
    let admission = admit(definition).expect("the fixture definition is admitted");
    let contracts = definition
        .contracts
        .iter()
        .map(|contract| contract.id.clone())
        .collect();
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
        contracts,
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
        &definition_in(AssuranceMode::Verify),
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
        &definition_in(AssuranceMode::Enforce),
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
        &definition_in(AssuranceMode::Enforce),
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
            &definition_in(AssuranceMode::Enforce),
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
        &definition_in(AssuranceMode::Enforce),
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
        &definition_in(AssuranceMode::Enforce),
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
        &definition_in(AssuranceMode::Enforce),
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
        &{
            let mut other = definition_in(AssuranceMode::Enforce);
            other.version = "2".to_string();
            other
        },
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
        &definition_in(AssuranceMode::Enforce),
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
        &definition_in(AssuranceMode::Enforce),
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
fn a_boundary_is_denied_when_the_required_contracts_obligations_are_unaccounted_for() {
    // The run discharged something real, and nothing belonging to the contract
    // the boundary is about. Before coverage was computed, listing the contract
    // on the certificate was enough to cross here.
    let both = definition_with_scan(AssuranceMode::Enforce);
    let enforced = certificate_for(&both, AssuranceMode::Enforce, vec![discharged("compiles")]);

    let mut policy = policy(AssuranceVerdict::Accepted, AssuranceMode::Enforce);
    policy.boundaries.insert(
        id("publish-boundary"),
        BoundaryPolicy {
            minimum: AssuranceVerdict::Accepted,
            contract: Some(id("scanned-under-named-rules")),
            requires_approval: None,
        },
    );

    let decision = decide_boundary(
        &policy,
        AssuranceMode::Enforce,
        &both,
        Some(&enforced),
        &id("publish-boundary"),
    );

    assert_eq!(
        decision,
        BoundaryDecision::Denied {
            reason: DenialReason::ContractNotCovered {
                required: id("scanned-under-named-rules"),
                missing: vec![id("no-secrets-in-output")],
            }
        },
        "the denial should name the obligation nobody accounted for"
    );
}

#[test]
fn a_boundary_requiring_a_contract_the_definition_never_declared_is_denied() {
    let enforced = certificate(AssuranceMode::Enforce, vec![discharged("compiles")]);

    let mut policy = policy(AssuranceVerdict::Accepted, AssuranceMode::Enforce);
    policy.boundaries.insert(
        id("publish-boundary"),
        BoundaryPolicy {
            minimum: AssuranceVerdict::Accepted,
            contract: Some(id("no-secrets-leaked")),
            requires_approval: None,
        },
    );

    let decision = decide_boundary(
        &policy,
        AssuranceMode::Enforce,
        &definition_in(AssuranceMode::Enforce),
        Some(&enforced),
        &id("publish-boundary"),
    );

    assert_eq!(
        decision,
        BoundaryDecision::Denied {
            reason: DenialReason::ContractNotInDefinition {
                required: id("no-secrets-leaked"),
            }
        }
    );
}

#[test]
fn a_policy_wide_required_contract_is_enforced() {
    // `required_contracts` was declared, set by tests, and read by nothing.
    let both = definition_with_scan(AssuranceMode::Enforce);
    let enforced = certificate_for(&both, AssuranceMode::Enforce, vec![discharged("compiles")]);

    let mut policy = policy(AssuranceVerdict::Accepted, AssuranceMode::Enforce);
    policy.required_contracts = vec![id("scanned-under-named-rules")];

    let decision = decide_boundary(
        &policy,
        AssuranceMode::Enforce,
        &both,
        Some(&enforced),
        &id("publish-boundary"),
    );

    assert_eq!(
        decision,
        BoundaryDecision::Denied {
            reason: DenialReason::ContractNotCovered {
                required: id("scanned-under-named-rules"),
                missing: vec![id("no-secrets-in-output")],
            }
        }
    );
}

#[test]
fn obligations_beyond_the_contract_do_not_stop_a_crossing() {
    // Coverage asks whether every declared obligation is accounted for, not
    // whether the certificate confined itself to them.
    let both = definition_with_scan(AssuranceMode::Enforce);
    let enforced = certificate_for(
        &both,
        AssuranceMode::Enforce,
        vec![
            discharged("compiles"),
            obligation_of("no-secrets-in-output", "scanned-under-named-rules"),
        ],
    );

    let decision = decide_boundary(
        &policy(AssuranceVerdict::Accepted, AssuranceMode::Enforce),
        AssuranceMode::Enforce,
        &both,
        Some(&enforced),
        &id("publish-boundary"),
    );

    assert_eq!(
        decision,
        BoundaryDecision::Allowed {
            verdict: AssuranceVerdict::Accepted
        }
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

    let mut certificates = CertificateMap::new();
    let both = definition_with_scan(AssuranceMode::Enforce);
    let compiles = certificates.insert(certificate_for(
        &both,
        AssuranceMode::Enforce,
        vec![discharged("compiles")],
    ));
    let scanned = certificates.insert(certificate_for(
        &both,
        AssuranceMode::Enforce,
        vec![obligation_of(
            "no-secrets-in-output",
            "scanned-under-named-rules",
        )],
    ));

    let record = VerificationRecord::admit(
        RawVerificationRecord {
            contract: "patch-compiles".to_string(),
            certificate: compiles,
        },
        &certificates,
        &both,
        Provenance::Complete,
    )
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
    let elsewhere = VerificationRecord::admit(
        RawVerificationRecord {
            contract: "scanned-under-named-rules".to_string(),
            certificate: scanned,
        },
        &certificates,
        &both,
        Provenance::Complete,
    )
    .expect("the record is admitted");
    assert!(matches!(
        check_trust_route(
            &governed,
            &id("governed-memory"),
            &TrustClass::from_record(&elsewhere)
        ),
        Err(DenialReason::ContractMismatch { .. })
    ));

    // A destination nothing protects lets anything through, and says so by
    // succeeding rather than by pretending a check happened.
    assert_eq!(
        check_trust_route(&governed, &id("scratch-space"), &TrustClass::Unverified),
        Ok(())
    );
}
