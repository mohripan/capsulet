//! What kind of checker discharged an obligation, and what a model may do.
//!
//! A certificate used to say who discharged an obligation and not what kind of
//! checker that was, so a model's agreement would have read exactly like a
//! recomputed sum. These tests pin the rules ADR 0021 sets: a model endorses or
//! disputes and never discharges, a recorded basis names a checker of that kind,
//! a review is under the authority the obligation names, and no number of
//! endorsements makes a certificate `accepted`.

mod fixtures;

use std::collections::BTreeMap;

use capsulet_ir::assurance::{
    BoundaryDecision, BoundaryPolicy, DecisionContext, DenialReason, EndorsementRequirement,
};
use capsulet_ir::correctness::certificate::{Subject, VerifierRecord, VerifierTrust};
use capsulet_ir::correctness::evidence::EvidenceRef;
use capsulet_ir::correctness::obligation::{
    BasisKind, DischargeBasis, DischargeState, ObligationStatement, RepairOwner,
};
use capsulet_ir::correctness::proposal::{Producer, ProducerKind};
use capsulet_ir::correctness::{Certificate, CertificateBody};
use capsulet_ir::{
    AssuranceMode, AssurancePolicy, AssuranceVerdict, CertificateError, CheckerVerdict, Digest,
    Identity, Obligation, RecordedTime, admit, decide_boundary,
};

use fixtures::{definition_in, id};

const CAPTURED_AT: i64 = 1_772_000_000_000;
const CONTRACT: &str = "patch-compiles";

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

fn record(name: &str, trust: VerifierTrust) -> VerifierRecord {
    VerifierRecord {
        identity: Identity::new(id(name), "1"),
        environment: Digest::of(name.as_bytes()),
        inputs: vec![],
        outputs: vec![],
        trust,
        verdict: CheckerVerdict::Accepted,
    }
}

fn model(name: &str, independent: bool) -> VerifierRecord {
    record(
        name,
        VerifierTrust::ModelJudgment {
            family: if independent { "other" } else { "proposer" }.to_string(),
            independent_of_proposer: independent,
            calibration: Some(id("calibration-2026-11")),
        },
    )
}

/// Every kind of verifier these tests name.
fn verifiers() -> Vec<VerifierRecord> {
    vec![
        record("cargo-test", VerifierTrust::Deterministic),
        record(
            "compiler",
            VerifierTrust::DeclaredOracle {
                rationale: "rustc in a pinned image".to_string(),
            },
        ),
        model("second-reader", true),
        model("same-family", false),
    ]
}

fn statement(name: &str) -> ObligationStatement {
    ObligationStatement::new(id(name), format!("`{name}` holds"), RepairOwner::Verifier)
}

fn obligation(statement: ObligationStatement, state: DischargeState) -> Obligation {
    Obligation {
        statement,
        contract: id(CONTRACT),
        state,
    }
}

fn seal(obligations: Vec<Obligation>) -> Result<Certificate, CertificateError> {
    let mode = AssuranceMode::Enforce;
    let definition = definition_in(mode);
    let admission = admit(&definition).expect("admitted");
    let verdict = AssuranceVerdict::under_mode(mode, &obligations);
    Certificate::seal(CertificateBody {
        schema_version: Certificate::current_schema_version(),
        id: id("cert-basis"),
        admission: admission.clone(),
        mode,
        subject: Subject {
            definition: *admission.definition(),
            definition_version: "1".to_string(),
            run: Some(id("run-basis")),
            inputs: vec![],
            outputs: vec![],
        },
        policy_version: "release-policy/4".to_string(),
        kernel_version: "capsulet-kernel 0.1.0".to_string(),
        contracts: vec![id(CONTRACT)],
        verifiers: verifiers(),
        evidence: vec![evidence()],
        obligations,
        loops: vec![],
        verdict,
    })
}

fn discharged(by: &str, basis: DischargeBasis) -> DischargeState {
    DischargeState::Discharged {
        by: id(by),
        evidence: vec![evidence().content],
        basis,
    }
}

fn endorsed(by: &str) -> DischargeState {
    DischargeState::Endorsed {
        by: id(by),
        evidence: vec![],
    }
}

fn disputed(by: &str) -> DischargeState {
    DischargeState::Disputed {
        by: id(by),
        reason: "the clause it cites is qualified by section 12".to_string(),
        evidence: vec![],
    }
}

#[test]
fn no_combination_of_endorsements_makes_a_certificate_accepted() {
    // Exhaustive over three obligations and every state a model can leave: with
    // at least one endorsement or dispute among them, the verdict is never
    // `accepted`, whatever the others are.
    let states = || {
        vec![
            discharged("cargo-test", DischargeBasis::Mechanical),
            endorsed("second-reader"),
            endorsed("same-family"),
            disputed("second-reader"),
        ]
    };
    let mut cases = 0;
    for first in states() {
        for second in states() {
            for third in states() {
                let chosen = [first.clone(), second.clone(), third.clone()];
                let modelled = chosen.iter().any(|state| {
                    matches!(
                        state,
                        DischargeState::Endorsed { .. } | DischargeState::Disputed { .. }
                    )
                });
                let obligations = chosen
                    .into_iter()
                    .enumerate()
                    .map(|(index, state)| obligation(statement(&format!("step-{index}")), state))
                    .collect();
                let certificate = seal(obligations).expect("seals");
                if modelled {
                    assert_eq!(certificate.verdict(), AssuranceVerdict::Conditional);
                } else {
                    assert_eq!(certificate.verdict(), AssuranceVerdict::Accepted);
                }
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 64);
}

#[test]
fn a_model_cannot_discharge_and_an_endorsement_must_name_one() {
    for basis in [DischargeBasis::Unrecorded, DischargeBasis::Mechanical] {
        let error = seal(vec![obligation(
            statement("reading"),
            discharged("second-reader", basis),
        )])
        .expect_err("a model's discharge is refused");
        assert!(
            matches!(error, CertificateError::ModelCannotDischarge { .. }),
            "{error}"
        );
    }

    let error = seal(vec![obligation(
        statement("reading"),
        endorsed("cargo-test"),
    )])
    .expect_err("an endorsement by a deterministic checker is refused");
    assert!(
        matches!(error, CertificateError::EndorsementWithoutAModel { .. }),
        "{error}"
    );
    let error = seal(vec![obligation(statement("reading"), disputed("nobody"))])
        .expect_err("a dispute by nothing on record is refused");
    assert!(
        matches!(error, CertificateError::EndorsementWithoutAModel { .. }),
        "{error}"
    );
}

#[test]
fn a_recorded_basis_names_a_checker_of_that_kind() {
    assert!(
        seal(vec![obligation(
            statement("builds"),
            discharged("compiler", DischargeBasis::ReplayableTool),
        )])
        .is_ok()
    );
    for (by, basis) in [
        ("compiler", DischargeBasis::Mechanical),
        ("cargo-test", DischargeBasis::ReplayableTool),
        ("nobody", DischargeBasis::Mechanical),
    ] {
        let error = seal(vec![obligation(statement("builds"), discharged(by, basis))])
            .expect_err("a basis its checker does not have is refused");
        assert!(
            matches!(error, CertificateError::BasisWithoutAVerifier { .. }),
            "{error}"
        );
    }
}

#[test]
fn a_review_counts_only_under_the_authority_the_obligation_names() {
    let mut reading = statement("reading");
    reading.authority = Some(id("legal-review"));
    let review = |authority: &str| {
        discharged(
            "dana",
            DischargeBasis::HumanReview {
                authority: id(authority),
            },
        )
    };

    let certificate =
        seal(vec![obligation(reading.clone(), review("legal-review"))]).expect("seals");
    assert_eq!(certificate.verdict(), AssuranceVerdict::Accepted);

    let error = seal(vec![obligation(reading, review("anyone"))])
        .expect_err("another authority is refused");
    assert!(
        matches!(error, CertificateError::ReviewWithoutAuthority { .. }),
        "{error}"
    );
    let error = seal(vec![obligation(
        statement("unnamed"),
        review("legal-review"),
    )])
    .expect_err("an obligation naming no authority cannot be reviewed");
    assert!(
        matches!(error, CertificateError::ReviewWithoutAuthority { .. }),
        "{error}"
    );
}

#[test]
fn an_obligation_refuses_a_discharge_of_a_kind_it_does_not_accept() {
    let mut reading = statement("reading");
    reading.accepts = vec![BasisKind::HumanReview];
    let error = seal(vec![obligation(
        reading.clone(),
        discharged("cargo-test", DischargeBasis::Mechanical),
    )])
    .expect_err("a mechanical discharge of a reading that needs review is refused");
    assert!(
        matches!(error, CertificateError::BasisNotAccepted { .. }),
        "{error}"
    );
    // An unrecorded basis is a legacy discharge, and an obligation that names
    // what it accepts was written after bases existed, so it is refused too.
    let error = seal(vec![obligation(
        reading,
        discharged("cargo-test", DischargeBasis::Unrecorded),
    )])
    .expect_err("an unrecorded basis is not one a new obligation accepts");
    assert!(
        matches!(error, CertificateError::BasisNotAccepted { .. }),
        "{error}"
    );
}

#[test]
fn an_unrecorded_basis_and_a_plain_statement_encode_as_they_always_did() {
    let state = discharged("cargo-test", DischargeBasis::Unrecorded);
    let encoded = serde_json::to_value(&state).expect("encodes");
    assert!(encoded.get("basis").is_none(), "{encoded}");
    let encoded = serde_json::to_value(statement("builds")).expect("encodes");
    assert!(encoded.get("authority").is_none(), "{encoded}");
    assert!(encoded.get("accepts").is_none(), "{encoded}");

    let read: DischargeState = serde_json::from_value(serde_json::json!({
        "state": "discharged", "by": "cargo-test", "evidence": []
    }))
    .expect("a legacy discharge reads");
    assert!(matches!(
        read,
        DischargeState::Discharged {
            basis: DischargeBasis::Unrecorded,
            ..
        }
    ));
}

fn policy(boundary: BoundaryPolicy) -> AssurancePolicy {
    AssurancePolicy {
        id: id("release-policy"),
        version: "4".to_string(),
        mode: AssuranceMode::Enforce,
        required_contracts: vec![],
        required_verifiers: vec![],
        boundaries: BTreeMap::from([(id("publish-boundary"), boundary)]),
        waiver_authorities: vec![],
        trust_routes: vec![],
    }
}

fn conditional_boundary() -> BoundaryPolicy {
    BoundaryPolicy {
        minimum: AssuranceVerdict::Conditional,
        contract: Some(id(CONTRACT)),
        requires_approval: None,
        max_age_ms: None,
        refuse_disputed: false,
        endorsement: None,
    }
}

fn cross(boundary: BoundaryPolicy, state: DischargeState) -> BoundaryDecision {
    let certificate = seal(vec![obligation(statement("compiles"), state)]).expect("seals");
    decide_boundary(
        &policy(boundary),
        AssuranceMode::Enforce,
        &definition_in(AssuranceMode::Enforce),
        Some(&certificate),
        &id("publish-boundary"),
        DecisionContext::at(RecordedTime(CAPTURED_AT)),
    )
}

#[test]
fn a_boundary_may_refuse_to_cross_over_a_dispute() {
    assert!(cross(conditional_boundary(), disputed("second-reader")).permits_crossing());
    let refusing = BoundaryPolicy {
        refuse_disputed: true,
        ..conditional_boundary()
    };
    assert!(matches!(
        cross(refusing, disputed("second-reader")),
        BoundaryDecision::Denied {
            reason: DenialReason::Disputed { .. }
        }
    ));
}

#[test]
fn an_endorsement_requirement_counts_only_the_models_the_policy_accepts() {
    let independent = BoundaryPolicy {
        endorsement: Some(EndorsementRequirement { independent: true }),
        ..conditional_boundary()
    };
    let unread = DischargeState::Residual {
        rationale: "nobody has read this".to_string(),
        evidence: vec![],
    };
    for state in [unread, endorsed("same-family"), disputed("second-reader")] {
        assert!(matches!(
            cross(independent.clone(), state),
            BoundaryDecision::Denied {
                reason: DenialReason::NotEndorsed { .. }
            }
        ));
    }
    // Endorsed by a second reading: crosses, and the verdict it crosses with is
    // still `conditional`, because an endorsement certifies nothing.
    assert!(matches!(
        cross(independent, endorsed("second-reader")),
        BoundaryDecision::Allowed {
            verdict: AssuranceVerdict::Conditional
        }
    ));
    let any_model = BoundaryPolicy {
        endorsement: Some(EndorsementRequirement { independent: false }),
        ..conditional_boundary()
    };
    assert!(cross(any_model, endorsed("same-family")).permits_crossing());
}
