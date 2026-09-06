//! Observe reports, Verify reports a verdict, Enforce stops things.

mod fixtures;

use std::collections::BTreeMap;

use capsulet_ir::assurance::{
    BoundaryDecision, BoundaryPolicy, DenialReason, TrustRoute, VerifierRequirement,
};
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

/// An obligation of a named contract, left open.
fn open_of(name: &str, contract: &str) -> Obligation {
    Obligation {
        contract: id(contract),
        ..obligation(
            name,
            DischargeState::Residual {
                rationale: "nobody has checked this".to_string(),
                evidence: vec![],
            },
        )
    }
}

/// An obligation of a named contract, checked and found not to hold.
fn failed_of(name: &str, contract: &str) -> Obligation {
    Obligation {
        contract: id(contract),
        ..obligation(
            name,
            DischargeState::Failed {
                reason: "it did not hold".to_string(),
                owner: RepairOwner::Verifier,
            },
        )
    }
}

/// One verifier record.
fn verifier(name: &str, version: &str, verdict: CheckerVerdict) -> VerifierRecord {
    VerifierRecord {
        identity: Identity::new(id(name), version),
        environment: Digest::of(b"an image"),
        inputs: vec![],
        outputs: vec![],
        trust: VerifierTrust::Deterministic,
        verdict,
    }
}

fn certificate_with_verifiers(
    definition: &Definition,
    mode: AssuranceMode,
    obligations: Vec<Obligation>,
    verifiers: Vec<VerifierRecord>,
) -> Certificate {
    let mut body = seal_body(definition, mode, obligations);
    body.verifiers = verifiers;
    Certificate::seal(body).expect("the certificate seals")
}

fn seal_body(
    definition: &Definition,
    mode: AssuranceMode,
    obligations: Vec<Obligation>,
) -> CertificateBody {
    let admission = admit(definition).expect("the fixture definition is admitted");
    let contracts = definition
        .contracts
        .iter()
        .map(|contract| contract.id.clone())
        .collect();
    let verdict = AssuranceVerdict::under_mode(mode, &obligations);

    CertificateBody {
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
        verifiers: vec![verifier("cargo-test", "1.96", CheckerVerdict::Accepted)],
        evidence: vec![evidence()],
        obligations,
        loops: vec![],
        verdict,
    }
}

fn certificate_for(
    definition: &Definition,
    mode: AssuranceMode,
    obligations: Vec<Obligation>,
) -> Certificate {
    Certificate::seal(seal_body(definition, mode, obligations)).expect("the certificate seals")
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
        required_verifiers: vec![VerifierRequirement::named(id("cargo-test"))],
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
    demanding
        .required_verifiers
        .push(VerifierRequirement::named(id("cargo-audit")));

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
fn a_required_verifier_that_ran_and_failed_does_not_satisfy_the_requirement() {
    // The gap this task closes: the requirement was a name match, so a scanner
    // that ran, concluded `rejected`, and said so out loud satisfied a policy
    // that required the scanner.
    let both = definition_in(AssuranceMode::Enforce);
    let enforced = certificate_with_verifiers(
        &both,
        AssuranceMode::Enforce,
        vec![discharged("compiles")],
        vec![verifier("cargo-test", "1.96", CheckerVerdict::Rejected)],
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
        BoundaryDecision::Denied {
            reason: DenialReason::VerifierDidNotPass {
                identity: id("cargo-test"),
                required: AssuranceVerdict::Conditional,
                found: CheckerVerdict::Rejected,
            }
        }
    );
}

#[test]
fn a_required_verifier_at_the_wrong_version_does_not_satisfy_the_requirement() {
    let both = definition_in(AssuranceMode::Enforce);
    let enforced = certificate_with_verifiers(
        &both,
        AssuranceMode::Enforce,
        vec![discharged("compiles")],
        vec![verifier("cargo-test", "1.90", CheckerVerdict::Accepted)],
    );

    let mut pinned = policy(AssuranceVerdict::Accepted, AssuranceMode::Enforce);
    pinned.required_verifiers = vec![VerifierRequirement {
        name: id("cargo-test"),
        version: Some("1.96".to_string()),
        environment: None,
        minimum: AssuranceVerdict::Accepted,
    }];

    let decision = decide_boundary(
        &pinned,
        AssuranceMode::Enforce,
        &both,
        Some(&enforced),
        &id("publish-boundary"),
    );

    assert_eq!(
        decision,
        BoundaryDecision::Denied {
            reason: DenialReason::VerifierVersionMismatch {
                identity: id("cargo-test"),
                required: "1.96".to_string(),
                found: vec!["1.90".to_string()],
            }
        }
    );
}

#[test]
fn a_required_verifier_from_the_wrong_environment_does_not_satisfy_the_requirement() {
    // Same tool, same version, different image. The environment is what makes a
    // deterministic verifier's word reproducible, so a policy that pins one is
    // not satisfied by a run somewhere else.
    let both = definition_in(AssuranceMode::Enforce);
    let enforced = certificate_with_verifiers(
        &both,
        AssuranceMode::Enforce,
        vec![discharged("compiles")],
        vec![verifier("cargo-test", "1.96", CheckerVerdict::Accepted)],
    );

    let mut pinned = policy(AssuranceVerdict::Accepted, AssuranceMode::Enforce);
    pinned.required_verifiers = vec![VerifierRequirement {
        name: id("cargo-test"),
        version: None,
        environment: Some(Digest::of(b"a different image")),
        minimum: AssuranceVerdict::Accepted,
    }];

    let decision = decide_boundary(
        &pinned,
        AssuranceMode::Enforce,
        &both,
        Some(&enforced),
        &id("publish-boundary"),
    );

    assert!(matches!(
        decision,
        BoundaryDecision::Denied {
            reason: DenialReason::VerifierEnvironmentMismatch { .. }
        }
    ));
}

#[test]
fn a_policy_may_demand_more_of_a_verifier_than_the_default() {
    // The default minimum lets a qualified result through, because the
    // certificate's verdict already carries that qualification and the
    // boundary's own minimum is where it is judged. A policy that wants this
    // particular verifier to have accepted outright says so.
    let both = definition_in(AssuranceMode::Enforce);
    let enforced = certificate_with_verifiers(
        &both,
        AssuranceMode::Enforce,
        vec![discharged("compiles")],
        vec![verifier("cargo-test", "1.96", CheckerVerdict::Conditional)],
    );

    let mut lenient = policy(AssuranceVerdict::Accepted, AssuranceMode::Enforce);
    lenient.required_verifiers = vec![VerifierRequirement {
        name: id("cargo-test"),
        version: None,
        environment: None,
        minimum: AssuranceVerdict::Conditional,
    }];

    assert!(
        decide_boundary(
            &lenient,
            AssuranceMode::Enforce,
            &both,
            Some(&enforced),
            &id("publish-boundary"),
        )
        .permits_crossing()
    );

    let mut strict = lenient.clone();
    strict.required_verifiers[0].minimum = AssuranceVerdict::Accepted;
    assert!(
        !decide_boundary(
            &strict,
            AssuranceMode::Enforce,
            &both,
            Some(&enforced),
            &id("publish-boundary"),
        )
        .permits_crossing()
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
fn a_residual_on_another_contract_does_not_deny_this_boundary() {
    // Everything the boundary's contract asked for is discharged. Something
    // else in the run was never decided. Before verdicts were per-contract the
    // global `conditional` denied this crossing — a false denial, and the kind
    // that pushes an operator to lower the boundary's minimum and lose the
    // precision entirely.
    let both = definition_with_scan(AssuranceMode::Enforce);
    let enforced = certificate_for(
        &both,
        AssuranceMode::Enforce,
        vec![
            discharged("compiles"),
            open_of("no-secrets-in-output", "scanned-under-named-rules"),
        ],
    );
    assert_eq!(
        enforced.verdict(),
        AssuranceVerdict::Conditional,
        "the run as a whole is conditional"
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
        },
        "the boundary is about `patch-compiles`, and `patch-compiles` was fully discharged"
    );
}

#[test]
fn a_residual_on_this_contract_still_denies_the_boundary() {
    let both = definition_with_scan(AssuranceMode::Enforce);
    let enforced = certificate_for(
        &both,
        AssuranceMode::Enforce,
        vec![
            open_of("compiles", "patch-compiles"),
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
        BoundaryDecision::Denied {
            reason: DenialReason::VerdictBelowMinimum {
                required: AssuranceVerdict::Accepted,
                found: AssuranceVerdict::Conditional,
            }
        }
    );
}

#[test]
fn a_failure_anywhere_denies_however_well_this_contract_went() {
    // The asymmetry is deliberate. An undecided obligation of another contract
    // says nothing about this one. An obligation that was checked and did not
    // hold says the run produced something known to be wrong, and letting an
    // effect out of it on the strength of an unrelated contract is the "passing
    // check of the wrong property" this gate exists to refuse.
    let both = definition_with_scan(AssuranceMode::Enforce);
    let enforced = certificate_for(
        &both,
        AssuranceMode::Enforce,
        vec![
            discharged("compiles"),
            failed_of("no-secrets-in-output", "scanned-under-named-rules"),
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
        BoundaryDecision::Denied {
            reason: DenialReason::VerdictBelowMinimum {
                required: AssuranceVerdict::Accepted,
                found: AssuranceVerdict::Rejected,
            }
        }
    );
}

#[test]
fn the_overall_verdict_is_the_weakest_of_the_per_contract_verdicts() {
    // The global verdict is not replaced by the per-contract one; it stays the
    // summary, and it is exactly the meet. A drift between the two would mean
    // the gate and the certificate disagreed about the same run.
    let obligations = vec![
        discharged("compiles"),
        open_of("no-secrets-in-output", "scanned-under-named-rules"),
    ];
    let compiles =
        AssuranceVerdict::for_contract(AssuranceMode::Enforce, &id("patch-compiles"), &obligations);
    let scanned = AssuranceVerdict::for_contract(
        AssuranceMode::Enforce,
        &id("scanned-under-named-rules"),
        &obligations,
    );

    assert_eq!(compiles, AssuranceVerdict::Accepted);
    assert_eq!(scanned, AssuranceVerdict::Conditional);
    assert_eq!(
        AssuranceVerdict::under_mode(AssuranceMode::Enforce, &obligations),
        AssuranceVerdict::Conditional,
        "the weaker of the two"
    );
}

#[test]
fn a_contract_the_certificate_says_nothing_about_is_unverified_not_accepted() {
    // Vacuous truth is the trap here: no obligations of a contract must not
    // read as "every obligation of it was discharged".
    let obligations = vec![discharged("compiles")];

    assert_eq!(
        AssuranceVerdict::for_contract(
            AssuranceMode::Enforce,
            &id("scanned-under-named-rules"),
            &obligations,
        ),
        AssuranceVerdict::Unverified
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
