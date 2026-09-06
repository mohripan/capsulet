//! Trust strengthens only through a record admitted against a certificate that
//! exists, is about this definition, and covers the contract claimed. These
//! tests are the statement of that rule.

mod fixtures;

use capsulet_ir::correctness::certificate::Subject;
use capsulet_ir::correctness::obligation::{DischargeState, ObligationStatement, RepairOwner};
use capsulet_ir::correctness::{Certificate, CertificateBody};
use capsulet_ir::trust::{CertificateMap, Provenance, RawVerificationRecord, TrustError};
use capsulet_ir::{
    AssuranceMode, AssuranceVerdict, Definition, Digest, Obligation, TrustClass,
    VerificationRecord, admit,
};

use fixtures::{definition_with_scan, id};

/// The contract the fixture definition declares, and the one obligation it
/// comprises. Coverage is computed against exactly these.
const CONTRACT: &str = "patch-compiles";
const STATEMENT: &str = "compiles";

const OTHER_CONTRACT: &str = "scanned-under-named-rules";
const OTHER_STATEMENT: &str = "no-secrets-in-output";

fn definition() -> Definition {
    definition_with_scan(AssuranceMode::Enforce)
}

fn obligation(statement: &str, contract: &str, state: DischargeState) -> Obligation {
    Obligation {
        statement: ObligationStatement {
            id: id(statement),
            statement: format!("`{statement}` holds"),
            owner: RepairOwner::Verifier,
        },
        contract: id(contract),
        state,
    }
}

fn discharged(statement: &str, contract: &str) -> Obligation {
    obligation(
        statement,
        contract,
        DischargeState::Discharged {
            by: id("the-test-runner"),
            evidence: vec![],
        },
    )
}

/// A sealed certificate about the fixture definition. The verdict is not passed
/// in: it is derived from the obligations, as sealing requires.
fn certificate(obligations: Vec<Obligation>) -> Certificate {
    let mode = AssuranceMode::Enforce;
    let definition = definition();
    let admission = admit(&definition).expect("admitted");
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
        contracts: vec![id(CONTRACT), id(OTHER_CONTRACT)],
        verifiers: vec![],
        evidence: vec![],
        obligations,
        loops: vec![],
        verdict,
    })
    .expect("the body seals")
}

fn source_for(certificate: Certificate) -> (CertificateMap, Digest) {
    let mut certificates = CertificateMap::new();
    let digest = certificates.insert(certificate);
    (certificates, digest)
}

/// A record for `CONTRACT`, against a certificate carrying `extra` beyond the
/// one obligation the contract declares.
fn admitted(extra: Vec<Obligation>, provenance: Provenance) -> VerificationRecord {
    let mut obligations = vec![discharged(STATEMENT, CONTRACT)];
    obligations.extend(extra);
    let (certificates, digest) = source_for(certificate(obligations));

    VerificationRecord::admit(
        RawVerificationRecord {
            contract: CONTRACT.to_string(),
            certificate: digest,
        },
        &certificates,
        &definition(),
        provenance,
    )
    .expect("record is admitted")
}

fn open(statement: &str, contract: &str) -> Obligation {
    obligation(
        statement,
        contract,
        DischargeState::Residual {
            rationale: "nobody has checked this".to_string(),
            evidence: vec![],
        },
    )
}

#[test]
fn a_clean_accepted_record_justifies_verified() {
    let record = admitted(vec![], Provenance::Complete);

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
    let with_residual = admitted(vec![open("still-open", CONTRACT)], Provenance::Complete);
    assert_eq!(with_residual.residual_count(), 1);
    assert_eq!(with_residual.verdict(), AssuranceVerdict::Conditional);
    assert!(matches!(
        TrustClass::from_record(&with_residual),
        TrustClass::Conditional { .. }
    ));

    let hop = admitted(vec![], Provenance::Lost);
    assert!(matches!(
        TrustClass::from_record(&hop),
        TrustClass::Conditional { .. }
    ));
}

#[test]
fn a_rejected_certificate_carries_no_trust() {
    let failed = obligation(
        STATEMENT,
        CONTRACT,
        DischargeState::Failed {
            reason: "the patch did not compile".to_string(),
            owner: RepairOwner::Verifier,
        },
    );
    let (certificates, digest) = source_for(certificate(vec![failed]));

    let record = VerificationRecord::admit(
        RawVerificationRecord {
            contract: CONTRACT.to_string(),
            certificate: digest,
        },
        &certificates,
        &definition(),
        Provenance::Complete,
    )
    .expect("a rejected certificate still covers the contract; it just carries nothing");

    assert_eq!(record.verdict(), AssuranceVerdict::Rejected);
    assert_eq!(TrustClass::from_record(&record), TrustClass::Unverified);
}

#[test]
fn a_record_without_a_contract_is_not_admitted() {
    let (certificates, digest) = source_for(certificate(vec![discharged(STATEMENT, CONTRACT)]));

    assert_eq!(
        VerificationRecord::admit(
            RawVerificationRecord {
                contract: "   ".to_string(),
                certificate: digest,
            },
            &certificates,
            &definition(),
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
            &definition(),
            Provenance::Complete,
        ),
        Err(TrustError::CertificateNotFound {
            certificate: nowhere
        })
    );
}

#[test]
fn a_record_is_refused_when_the_contracts_obligations_are_unaccounted_for() {
    // The certificate discharged something real — but nothing belonging to the
    // contract this record claims.
    let (certificates, digest) = source_for(certificate(vec![discharged(
        OTHER_STATEMENT,
        OTHER_CONTRACT,
    )]));

    assert_eq!(
        VerificationRecord::admit(
            RawVerificationRecord {
                contract: CONTRACT.to_string(),
                certificate: digest,
            },
            &certificates,
            &definition(),
            Provenance::Complete,
        ),
        Err(TrustError::ContractNotCovered {
            contract: CONTRACT.to_string(),
            certificate: digest,
            missing: vec![id(STATEMENT)],
        })
    );
}

#[test]
fn an_obligation_attributed_to_another_contract_does_not_cover_this_one() {
    // Same statement identifier, filed under a different contract. Counting it
    // would reintroduce the hole through the back door.
    let (certificates, digest) =
        source_for(certificate(vec![discharged(STATEMENT, OTHER_CONTRACT)]));

    assert!(matches!(
        VerificationRecord::admit(
            RawVerificationRecord {
                contract: CONTRACT.to_string(),
                certificate: digest,
            },
            &certificates,
            &definition(),
            Provenance::Complete,
        ),
        Err(TrustError::ContractNotCovered { .. })
    ));
}

#[test]
fn a_record_naming_a_contract_the_definition_never_declared_is_refused() {
    let (certificates, digest) = source_for(certificate(vec![discharged(STATEMENT, CONTRACT)]));

    assert_eq!(
        VerificationRecord::admit(
            RawVerificationRecord {
                contract: "no-secrets-leaked".to_string(),
                certificate: digest,
            },
            &certificates,
            &definition(),
            Provenance::Complete,
        ),
        Err(TrustError::ContractNotInDefinition {
            contract: "no-secrets-leaked".to_string(),
        })
    );
}

#[test]
fn the_verdict_and_residual_count_come_from_the_certificate() {
    // Two records built from the same document shape against certificates that
    // differ. Nothing in the document says a verdict, so the records can only
    // disagree because their certificates do.
    let clean = admitted(vec![], Provenance::Complete);
    let two_open = admitted(
        vec![
            open("still-open-a", CONTRACT),
            open("still-open-b", CONTRACT),
        ],
        Provenance::Complete,
    );

    assert_eq!(clean.verdict(), AssuranceVerdict::Accepted);
    assert_eq!(clean.residual_count(), 0);
    assert_eq!(two_open.verdict(), AssuranceVerdict::Conditional);
    assert_eq!(two_open.residual_count(), 2);
}

#[test]
fn holding_a_record_at_a_weaker_class_is_allowed() {
    // Weakening stays available; there is no matching way up.
    let verified = TrustClass::from_record(&admitted(vec![], Provenance::Complete));
    assert!(matches!(verified, TrustClass::Verified { .. }));

    let held_back = verified.weakened();
    assert!(matches!(held_back, TrustClass::Conditional { .. }));
    assert_eq!(held_back.weakened(), TrustClass::Unverified);
}

#[test]
fn combining_values_yields_the_weakest_relevant_trust() {
    let verified = TrustClass::from_record(&admitted(vec![], Provenance::Complete));
    let conditional = TrustClass::from_record(&admitted(
        vec![open("still-open", CONTRACT)],
        Provenance::Complete,
    ));

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
    let compiled = TrustClass::from_record(&admitted(vec![], Provenance::Complete));

    let (certificates, digest) = source_for(certificate(vec![discharged(
        OTHER_STATEMENT,
        OTHER_CONTRACT,
    )]));
    let scanned = TrustClass::from_record(
        &VerificationRecord::admit(
            RawVerificationRecord {
                contract: OTHER_CONTRACT.to_string(),
                certificate: digest,
            },
            &certificates,
            &definition(),
            Provenance::Complete,
        )
        .expect("record is admitted"),
    );

    assert_eq!(compiled.meet(&scanned), TrustClass::Unverified);
}
