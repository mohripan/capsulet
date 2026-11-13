//! A validator run as a real process, and what its run means for a certificate.
//!
//! The fixture validator decides each obligation by the first word of its
//! statement (see `src/bin/fixture-validator.rs`), so these tests can ask for
//! every way a validator behaves and every way it can misbehave.

// The IR's own fixtures, which already allow what these tests leave unused.
#[path = "../../ir/tests/fixtures/mod.rs"]
mod fixtures;

use std::collections::BTreeMap;
use std::path::Path;

use capsulet_ir::correctness::certificate::{Subject, VerifierTrust};
use capsulet_ir::correctness::evidence::RecordedTime;
use capsulet_ir::correctness::obligation::{
    DischargeBasis, DischargeState, ObligationStatement, RepairOwner,
};
use capsulet_ir::correctness::{Certificate, CertificateBody};
use capsulet_ir::{AssuranceMode, AssuranceVerdict, CheckerVerdict, Digest, admit};
use capsulet_verifier::{
    Capability, Environment, Input, Limits, ProcessTransport, RawRun, RegisteredBasis,
    Registration, Requested, Transport, VerifierFailure, verify,
};

use fixtures::{definition_in, id};

const FIXTURE: &str = env!("CARGO_BIN_EXE_fixture-validator");
const IMAGE: &str =
    "capsulet/fixture@sha256:1111111111111111111111111111111111111111111111111111111111111111";

/// Stands in for a container transport. Isolation is the container's job, and
/// what is under test here is what the host does with an isolated run's answer,
/// so this one starts the fixture as a process and reports nothing else.
struct Isolated;

impl Transport for Isolated {
    fn run(
        &self,
        registration: &Registration,
        workdir: &Path,
        request: &[u8],
    ) -> Result<RawRun, String> {
        let mut local = registration.clone();
        local.environment = Environment::Process {
            program: FIXTURE.to_string(),
            args: Vec::new(),
        };
        ProcessTransport.run(&local, workdir, request)
    }
}

fn registration(basis: RegisteredBasis, environment: Environment) -> Registration {
    let capabilities = match basis {
        RegisteredBasis::ModelJudgment { .. } => vec![Capability::Model],
        RegisteredBasis::ReplayableTool => Vec::new(),
    };
    Registration {
        name: "fixture-validator".to_string(),
        version: "1".to_string(),
        basis,
        environment,
        capabilities,
        limits: Limits {
            timeout_ms: 5_000,
            memory_mb: 256,
            max_output_bytes: 64 * 1024,
        },
        parameters: BTreeMap::new(),
    }
}

fn tool() -> Registration {
    registration(
        RegisteredBasis::ReplayableTool,
        Environment::Container {
            image: IMAGE.to_string(),
            command: Vec::new(),
        },
    )
}

fn model() -> Registration {
    registration(
        RegisteredBasis::ModelJudgment {
            family: "other".to_string(),
            independent_of_proposer: true,
            calibration: Some("calibration-2026-11".to_string()),
        },
        Environment::Container {
            image: IMAGE.to_string(),
            command: Vec::new(),
        },
    )
}

fn local() -> Registration {
    registration(
        RegisteredBasis::ReplayableTool,
        Environment::Process {
            program: FIXTURE.to_string(),
            args: Vec::new(),
        },
    )
}

fn requested(statements: &[&str]) -> Vec<Requested> {
    statements
        .iter()
        .enumerate()
        .map(|(index, statement)| Requested {
            contract: id("patch-compiles"),
            statement: ObligationStatement::new(
                id(&format!("step-{index}")),
                (*statement).to_string(),
                RepairOwner::Proposer,
            ),
        })
        .collect()
}

fn inputs() -> Vec<Input> {
    vec![Input {
        name: "patch.diff".to_string(),
        bytes: b"--- a\n+++ b\n".to_vec(),
        media_type: "text/x-diff".to_string(),
    }]
}

fn seal(
    obligations: Vec<capsulet_ir::Obligation>,
    verification: &capsulet_verifier::Verification,
) -> Result<Certificate, capsulet_ir::CertificateError> {
    let mode = AssuranceMode::Enforce;
    let definition = definition_in(mode);
    let admission = admit(&definition).expect("admitted");
    let verdict = AssuranceVerdict::under_mode(mode, &obligations);
    Certificate::seal(CertificateBody {
        schema_version: Certificate::current_schema_version(),
        id: id("cert-verifier"),
        admission: admission.clone(),
        mode,
        subject: Subject {
            definition: *admission.definition(),
            definition_version: "1".to_string(),
            run: None,
            inputs: vec![],
            outputs: vec![],
        },
        policy_version: "release-policy/5".to_string(),
        kernel_version: "capsulet-kernel 0.1.0".to_string(),
        contracts: vec![id("patch-compiles")],
        verifiers: vec![verification.record.clone()],
        evidence: verification.evidence(RecordedTime(1_772_000_000_000)),
        obligations,
        loops: vec![],
        verdict,
    })
}

#[test]
fn an_isolated_tools_answers_discharge_fail_or_stay_residual_and_seal() {
    let wanted = requested(&["report holds", "fails to compile", "unsure about it"]);
    let verification = verify(&tool(), &Isolated, &wanted, &inputs()).expect("the run decides");
    assert!(verification.isolated);
    assert_eq!(verification.record.verdict, CheckerVerdict::Rejected);
    assert!(matches!(
        verification.record.trust,
        VerifierTrust::DeclaredOracle { .. }
    ));
    assert_eq!(verification.record.inputs, [Digest::of(b"--- a\n+++ b\n")]);
    assert_eq!(verification.outputs.len(), 1);
    assert_eq!(verification.outputs[0].name, "report.txt");

    let obligations = verification.obligations(&wanted);
    assert!(matches!(
        &obligations[0].state,
        DischargeState::Discharged { basis: DischargeBasis::ReplayableTool, evidence, .. }
            if evidence == &[verification.outputs[0].digest]
    ));
    assert!(matches!(
        &obligations[1].state,
        DischargeState::Failed {
            owner: RepairOwner::Proposer,
            ..
        }
    ));
    assert!(matches!(
        &obligations[2].state,
        DischargeState::Residual { .. }
    ));

    let certificate = seal(obligations, &verification).expect("a tool's discharge seals");
    assert_eq!(certificate.verdict(), AssuranceVerdict::Rejected);
}

#[test]
fn a_models_holds_endorses_and_its_does_not_hold_disputes_and_neither_decides() {
    let wanted = requested(&["holds as read", "fails as read"]);
    let verification = verify(&model(), &Isolated, &wanted, &inputs()).expect("the run decides");
    assert!(matches!(
        verification.record.trust,
        VerifierTrust::ModelJudgment { .. }
    ));
    let obligations = verification.obligations(&wanted);
    assert!(matches!(
        &obligations[0].state,
        DischargeState::Endorsed { .. }
    ));
    assert!(matches!(
        &obligations[1].state,
        DischargeState::Disputed { .. }
    ));
    let certificate = seal(obligations, &verification).expect("a model's record seals");
    assert_eq!(certificate.verdict(), AssuranceVerdict::Conditional);
}

#[test]
fn an_unisolated_development_run_decides_nothing_and_says_so() {
    let wanted = requested(&["holds", "fails"]);
    let verification =
        verify(&local(), &ProcessTransport, &wanted, &inputs()).expect("the run answers");
    assert!(!verification.isolated);
    let VerifierTrust::DeclaredOracle { rationale } = &verification.record.trust else {
        panic!("{:?}", verification.record.trust);
    };
    assert!(rationale.contains("unisolated"), "{rationale}");
    for obligation in verification.obligations(&wanted) {
        let DischargeState::Residual { rationale, .. } = &obligation.state else {
            panic!("an unisolated run decided {:?}", obligation.state);
        };
        assert!(
            rationale.contains("unisolated development run"),
            "{rationale}"
        );
    }
}

fn failure(statement: &str, registration: &Registration) -> VerifierFailure {
    verify(registration, &Isolated, &requested(&[statement]), &inputs())
        .expect_err("the run decides nothing")
}

#[test]
fn every_way_a_validator_misbehaves_is_a_typed_failure_owned_by_the_verifier() {
    let mut quick = tool();
    quick.limits.timeout_ms = 300;
    let mut small = tool();
    small.limits.max_output_bytes = 1_000;
    let cases: Vec<(&str, Registration, &str)> = vec![
        ("garbage", tool(), "malformed_response"),
        ("ancient", tool(), "protocol_mismatch"),
        ("crash", tool(), "crashed"),
        ("sleep", quick, "timed_out"),
        ("skip", tool(), "missing_result"),
        ("extra", tool(), "unexpected_result"),
        ("twice", tool(), "duplicate_result"),
        ("tamper", tool(), "inputs_modified"),
        ("flood", small, "outputs_too_large"),
    ];
    for (statement, registration, code) in cases {
        let failed = failure(statement, &registration);
        assert_eq!(failed.code(), code, "{statement}: {failed}");
        assert_eq!(failed.owner(), RepairOwner::Verifier);
    }
}

#[test]
fn a_registration_that_cannot_run_and_an_input_that_is_not_a_file_name_are_refused_first() {
    let mut networked = tool();
    networked.capabilities = vec![Capability::Network];
    assert_eq!(failure("holds", &networked).code(), "registration_refused");

    for name in ["../escape", "nested/file", ".hidden", ""] {
        let bad = vec![Input {
            name: name.to_string(),
            bytes: Vec::new(),
            media_type: "text/plain".to_string(),
        }];
        let refused =
            verify(&tool(), &Isolated, &requested(&["holds"]), &bad).expect_err("refused");
        assert_eq!(refused.code(), "bad_input", "{name:?}");
    }
}
