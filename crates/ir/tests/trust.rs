//! Trust strengthens only through a record admitted against a certificate that
//! exists and supports it. These tests are the statement of that rule.

mod fixtures;

use capsulet_ir::correctness::certificate::Subject;
use capsulet_ir::correctness::obligation::{DischargeState, ObligationStatement, RepairOwner};
use capsulet_ir::correctness::{Certificate, CertificateBody};
use capsulet_ir::trust::{CertificateMap, Provenance, RawVerificationRecord, TrustError};
use capsulet_ir::{
    AssuranceMode, AssuranceVerdict, Digest, Obligation, TrustClass, VerificationRecord, admit,
};

use fixtures::{definition_in, id};

const CONTRACT: &str = "compiles-and-passes-named-tests";

/// A sealed certificate covering `CONTRACT`, with `residuals` obligations left
/// open. The verdict is not passed in: it is derived from the obligations, as
/// sealing requires.
fn certificate_with(residuals: usize, failed: bool) -> Certificate {
    let mode = AssuranceMode::Enforce;
    let admission = admit(&definition_in(mode)).expect("admitted");

    let mut obligations = vec![Obligation {
        statement: ObligationStatement {
            id: id("the-named-tests-pass"),
            statement: "the named tests pass".to_string(),
            owner: RepairOwner::Verifier,
        },
        contract: id(CONTRACT),
        state: if failed {
            DischargeState::Failed {
                reason: "a named test did not pass".to_string(),
                owner: RepairOwner::Verifier,
            }
        } else {
            DischargeState::Discharged {
                by: id("the-test-runner"),
                evidence: vec![],
            }
        },
    }];

    for index in 0..residuals {
        obligations.push(Obligation {
            statement: ObligationStatement {
                id: id(&format!("still-open-{index}")),
                statement: "not yet decided".to_string(),
                owner: RepairOwner::Verifier,
            },
            contract: id(CONTRACT),
            state: DischargeState::Residual {
                rationale: "nobody has checked this".to_string(),
                evidence: vec![],
            },
        });
    }

    let verdict = AssuranceVerdict::under_mode(mode, &obligations);

    Certificate::seal(CertificateBody {
        schema_version: Certificate::current_schema_version(),
        id: id("cert-trust-test"),
        admission: admission.clone(),
        mode,
        subject: Subject {
            definition: *admission.definition(),
            definition_version: "1".to_string(),
            run: Some(id("run-trust-test")),
            inputs: vec![],
            outputs: vec![],
        },
        policy_version: "release-policy/3".to_string(),
        kernel_version: "capsulet-kernel 0.1.0".to_string(),
        contracts: vec![id(CONTRACT)],
        verifiers: vec![],
        evidence: vec![],
        obligations,
        loops: vec![],
        verdict,
    })
    .expect("the body seals")
}

/// A source holding one certificate, and the digest that resolves it.
fn source_for(certificate: Certificate) -> (CertificateMap, Digest) {
    let mut certificates = CertificateMap::new();
    let digest = certificates.insert(certificate);
    (certificates, digest)
}

fn admitted(residuals: usize, provenance: Provenance) -> VerificationRecord {
    let (certificates, digest) = source_for(certificate_with(residuals, false));
    VerificationRecord::admit(
        RawVerificationRecord {
            contract: CONTRACT.to_string(),
            certificate: digest,
        },
        &certificates,
        provenance,
    )
    .expect("record is admitted")
}

#[test]
fn a_clean_accepted_record_justifies_verified() {
    let record = admitted(0, Provenance::Complete);

    assert_eq!(record.verdict(), AssuranceVerdict::Accepted);
    assert_eq!(record.residual_count(), 0);
    assert!(matches!(
        TrustClass::from_record(&record),
        TrustClass::Verified { .. }
    ));
}

#[test]
fn residuals_or_lost_provenance_cap_trust_at_conditional() {
    // The residual count is the certificate's, not a number anyone typed.
    let with_residual = admitted(1, Provenance::Complete);
    assert_eq!(with_residual.residual_count(), 1);
    assert_eq!(with_residual.verdict(), AssuranceVerdict::Conditional);
    assert!(matches!(
        TrustClass::from_record(&with_residual),
        TrustClass::Conditional { .. }
    ));

    let hop = admitted(0, Provenance::Lost);
    assert!(matches!(
        TrustClass::from_record(&hop),
        TrustClass::Conditional { .. }
    ));
}

#[test]
fn a_rejected_certificate_carries_no_trust() {
    let (certificates, digest) = source_for(certificate_with(0, true));
    let record = VerificationRecord::admit(
        RawVerificationRecord {
            contract: CONTRACT.to_string(),
            certificate: digest,
        },
        &certificates,
        Provenance::Complete,
    )
    .expect("a rejected certificate still admits a record; it just carries nothing");

    assert_eq!(record.verdict(), AssuranceVerdict::Rejected);
    assert_eq!(TrustClass::from_record(&record), TrustClass::Unverified);
}

#[test]
fn a_record_without_a_contract_is_not_admitted() {
    let (certificates, digest) = source_for(certificate_with(0, false));

    assert_eq!(
        VerificationRecord::admit(
            RawVerificationRecord {
                contract: "   ".to_string(),
                certificate: digest,
            },
            &certificates,
            Provenance::Complete,
        ),
        Err(TrustError::MissingContract)
    );
}

#[test]
fn a_record_naming_a_certificate_that_does_not_exist_is_refused() {
    // The forgery this module exists to refuse: a hand-written record whose
    // certificate digest resolves to nothing at all.
    let nowhere = Digest::of(b"not a certificate");

    assert_eq!(
        VerificationRecord::admit(
            RawVerificationRecord {
                contract: CONTRACT.to_string(),
                certificate: nowhere,
            },
            &CertificateMap::new(),
            Provenance::Complete,
        ),
        Err(TrustError::CertificateNotFound {
            certificate: nowhere
        })
    );
}

#[test]
fn a_record_naming_a_contract_the_certificate_does_not_cover_is_refused() {
    let (certificates, digest) = source_for(certificate_with(0, false));

    assert_eq!(
        VerificationRecord::admit(
            RawVerificationRecord {
                contract: "no-secrets-leaked".to_string(),
                certificate: digest,
            },
            &certificates,
            Provenance::Complete,
        ),
        Err(TrustError::ContractNotCovered {
            contract: "no-secrets-leaked".to_string(),
            certificate: digest,
        })
    );
}

#[test]
fn the_verdict_and_residual_count_come_from_the_certificate() {
    // Two records built from the same document shape against certificates that
    // differ. Nothing in the document says a verdict, so the records can only
    // disagree because their certificates do.
    let clean = admitted(0, Provenance::Complete);
    let open = admitted(2, Provenance::Complete);

    assert_eq!(clean.verdict(), AssuranceVerdict::Accepted);
    assert_eq!(clean.residual_count(), 0);
    assert_eq!(open.verdict(), AssuranceVerdict::Conditional);
    assert_eq!(open.residual_count(), 2);
}

#[test]
fn holding_a_record_at_a_weaker_class_is_allowed() {
    // Weakening stays available; there is no matching way up.
    let verified = TrustClass::from_record(&admitted(0, Provenance::Complete));
    assert!(matches!(verified, TrustClass::Verified { .. }));

    let held_back = verified.weakened();
    assert!(matches!(held_back, TrustClass::Conditional { .. }));
    assert_eq!(held_back.weakened(), TrustClass::Unverified);
}

#[test]
fn combining_values_yields_the_weakest_relevant_trust() {
    let verified = TrustClass::from_record(&admitted(0, Provenance::Complete));
    let conditional = TrustClass::from_record(&admitted(1, Provenance::Complete));

    assert_eq!(verified.meet(&verified), verified);
    assert_eq!(verified.meet(&conditional), conditional);
    assert_eq!(
        verified.meet(&TrustClass::Unverified),
        TrustClass::Unverified
    );
    assert_eq!(
        TrustClass::meet_all(&[verified.clone(), conditional.clone()]),
        conditional
    );
    assert_eq!(
        TrustClass::meet_all(std::iter::empty::<&TrustClass>()),
        TrustClass::Unverified
    );
}

#[test]
fn combining_different_contracts_establishes_neither() {
    let compiled = TrustClass::from_record(&admitted(0, Provenance::Complete));

    // A second certificate, covering a different contract.
    let mode = AssuranceMode::Enforce;
    let admission = admit(&definition_in(mode)).expect("admitted");
    let obligations = vec![Obligation {
        statement: ObligationStatement {
            id: id("no-secrets-in-output"),
            statement: "the output carries no secrets".to_string(),
            owner: RepairOwner::Verifier,
        },
        contract: id("scanned-under-named-rules"),
        state: DischargeState::Discharged {
            by: id("the-scanner"),
            evidence: vec![],
        },
    }];
    let other = Certificate::seal(CertificateBody {
        schema_version: Certificate::current_schema_version(),
        id: id("cert-scanned"),
        admission: admission.clone(),
        mode,
        subject: Subject {
            definition: *admission.definition(),
            definition_version: "1".to_string(),
            run: Some(id("run-scanned")),
            inputs: vec![],
            outputs: vec![],
        },
        policy_version: "release-policy/3".to_string(),
        kernel_version: "capsulet-kernel 0.1.0".to_string(),
        contracts: vec![id("scanned-under-named-rules")],
        verifiers: vec![],
        evidence: vec![],
        obligations: obligations.clone(),
        loops: vec![],
        verdict: AssuranceVerdict::under_mode(mode, &obligations),
    })
    .expect("the body seals");

    let (certificates, digest) = source_for(other);
    let scanned = TrustClass::from_record(
        &VerificationRecord::admit(
            RawVerificationRecord {
                contract: "scanned-under-named-rules".to_string(),
                certificate: digest,
            },
            &certificates,
            Provenance::Complete,
        )
        .expect("record is admitted"),
    );

    assert_eq!(compiled.meet(&scanned), TrustClass::Unverified);
}
