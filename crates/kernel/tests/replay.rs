//! Offline replay: same verdict from the same evidence, and a loud failure
//! from anything else.

use capsulet_ir::admission::AdmissionRecord;
use capsulet_ir::capability::{Capability, Grant};
use capsulet_ir::correctness::certificate::{
    AssuranceVerdict, Certificate, CheckerVerdict, Subject, VerifierRecord, VerifierTrust,
};
use capsulet_ir::correctness::evidence::EvidenceRef;
use capsulet_ir::correctness::obligation::{DischargeState, ObligationStatement, RepairOwner};
use capsulet_ir::correctness::proposal::{Producer, ProducerKind};
use capsulet_ir::definition::AssuranceMode;
use capsulet_ir::graph::{Combine, TrustDerivation};
use capsulet_ir::value::LengthBounds;
use capsulet_ir::{
    CapabilitySet, Definition, Digest, Endpoint, Graph, GraphBuilder, Hyperedge, Identifier,
    Identity, InputPort, Node, NodeKind, Obligation, OutputPort, RecordedTime, ResourceBudget,
    ValueSchema, admit,
};
use capsulet_kernel::family::CLAIM_REASONING;
use capsulet_kernel::replay::{ReplayFinding, ReplayNote, ReplayOutcome, Unreadable};
use capsulet_kernel::workflow::{Assembly, KERNEL_VERSION, certify};
use capsulet_kernel::{EvidenceMap, replay};

fn id(value: &str) -> Identifier {
    Identifier::parse(value).expect("test identifiers are well formed")
}

fn definition() -> Definition {
    let text = ValueSchema::Text {
        length: LengthBounds::new(0, 1_024),
    };
    let graph = Graph::new(GraphBuilder {
        nodes: vec![
            Node {
                id: id("prepare"),
                name: "Prepare".to_string(),
                kind: NodeKind::PureComputation,
                inputs: vec![],
                outputs: vec![OutputPort::new(id("patch"), text.clone())],
                capabilities: vec![],
                effects: vec![],
                budget: ResourceBudget::deterministic(1_000),
                provider: None,
                sub_workflow: None,
            },
            Node {
                id: id("check"),
                name: "Check".to_string(),
                kind: NodeKind::Verifier,
                inputs: vec![InputPort::new(id("patch"), text)],
                outputs: vec![],
                capabilities: vec![],
                effects: vec![],
                budget: ResourceBudget::deterministic(60_000),
                provider: None,
                sub_workflow: None,
            },
        ],
        edges: vec![Hyperedge {
            id: id("prepare-to-check"),
            sources: vec![Endpoint::Port {
                node: id("prepare"),
                port: id("patch"),
            }],
            targets: vec![Endpoint::Port {
                node: id("check"),
                port: id("patch"),
            }],
            combine: Combine::Forward,
            trust: TrustDerivation::Weakest,
        }],
        ..GraphBuilder::default()
    })
    .expect("identifiers are distinct");

    Definition {
        schema_version: Definition::current_schema_version(),
        id: id("remediation"),
        version: "1".to_string(),
        name: "Remediation".to_string(),
        assurance: AssuranceMode::Enforce,
        capabilities: CapabilitySet::new(vec![Capability {
            id: id("scanner"),
            grant: Grant::Network {
                hosts: vec!["scanner.internal".to_string()],
            },
        }])
        .expect("grants are distinct"),
        budget: ResourceBudget {
            wall_ms: 600_000,
            tokens: 0,
            cost_micro_units: 0,
            effect_count: 1,
        },
        graph,
        boundaries: vec![],
        contracts: vec![],
    }
}

fn admission() -> AdmissionRecord {
    admit(&definition()).expect("the fixture definition is admitted")
}

fn evidence_ref(content: &[u8]) -> EvidenceRef {
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

fn assembly(evidence: EvidenceRef, verifiers: Vec<VerifierRecord>) -> Assembly {
    let admission = admission();
    Assembly {
        id: id("cert-1"),
        subject: Subject {
            definition: *admission.definition(),
            definition_version: "1".to_string(),
            run: Some(id("run-1")),
            inputs: vec![],
            outputs: vec![],
        },
        admission,
        mode: AssuranceMode::Enforce,
        policy_version: "release-policy/3".to_string(),
        contracts: vec![id("patch-compiles")],
        verifiers,
        obligations: vec![Obligation {
            statement: ObligationStatement {
                id: id("compiles"),
                statement: "the patch compiles".to_string(),
                owner: RepairOwner::Verifier,
            },
            contract: id("patch-compiles"),
            state: DischargeState::Discharged {
                by: id("cargo-test"),
                evidence: vec![evidence.content],
            },
        }],
        evidence: vec![evidence],
        loops: vec![],
    }
}

fn oracle() -> VerifierRecord {
    VerifierRecord {
        identity: Identity::new(id("cargo-test"), "1.96"),
        environment: Digest::of(b"an image"),
        inputs: vec![],
        outputs: vec![],
        trust: VerifierTrust::DeclaredOracle {
            rationale: "the test runner reports its own result".to_string(),
        },
        verdict: CheckerVerdict::Accepted,
    }
}

fn certificate_with(verifiers: Vec<VerifierRecord>, content: &[u8]) -> (Certificate, EvidenceMap) {
    let certificate =
        certify(assembly(evidence_ref(content), verifiers)).expect("the certificate seals");
    let mut bundle = EvidenceMap::new();
    bundle.insert(content.to_vec());
    (certificate, bundle)
}

#[test]
fn replaying_a_certificate_reaches_the_recorded_verdict() {
    let (certificate, bundle) = certificate_with(vec![oracle()], b"tests passed");

    let outcome = replay(&certificate, &bundle);
    assert!(
        outcome.reproduced(),
        "expected reproduction, got {outcome:?}"
    );
    assert_eq!(outcome.verdict(), Some(AssuranceVerdict::Accepted));
    assert_eq!(certificate.verdict(), AssuranceVerdict::Accepted);

    // Replaying twice gives the same answer, from the same bytes.
    assert_eq!(replay(&certificate, &bundle), outcome);
}

#[test]
fn replay_says_out_loud_that_it_did_not_re_run_the_tool() {
    let (certificate, bundle) = certificate_with(vec![oracle()], b"tests passed");

    let ReplayOutcome::Reproduced { notes, .. } = replay(&certificate, &bundle) else {
        panic!("expected reproduction");
    };
    assert!(
        notes.iter().any(|note| matches!(
            note,
            ReplayNote::OracleNotReExecuted { identity, .. } if identity == "cargo-test"
        )),
        "the outcome must state that the oracle was not re-executed: {notes:?}"
    );
}

#[test]
fn one_changed_byte_of_evidence_turns_the_verdict_to_rejected() {
    let (certificate, _) = certificate_with(vec![oracle()], b"tests passed");

    // The bundle carries different bytes under the digest the certificate cites.
    let mut tampered = EvidenceMap::new();
    tampered.insert_as(Digest::of(b"tests passed"), b"tests passed!".to_vec());

    let outcome = replay(&certificate, &tampered);
    assert_eq!(outcome.verdict(), Some(AssuranceVerdict::Rejected));
    let ReplayOutcome::Diverged { findings, .. } = outcome else {
        panic!("tampered evidence must diverge");
    };
    assert!(
        findings
            .iter()
            .any(|finding| matches!(finding, ReplayFinding::EvidenceTampered { .. })),
        "{findings:?}"
    );
}

#[test]
fn evidence_that_is_not_in_the_bundle_cannot_discharge_anything() {
    let (certificate, _) = certificate_with(vec![oracle()], b"tests passed");
    let empty = EvidenceMap::new();

    let outcome = replay(&certificate, &empty);
    assert_eq!(outcome.verdict(), Some(AssuranceVerdict::Rejected));
    let ReplayOutcome::Diverged { findings, .. } = outcome else {
        panic!("missing evidence must diverge");
    };
    assert!(
        findings
            .iter()
            .any(|finding| matches!(finding, ReplayFinding::EvidenceMissing { .. })),
        "{findings:?}"
    );
}

#[test]
fn an_unknown_deterministic_verifier_fails_closed() {
    let stranger = VerifierRecord {
        identity: Identity::new(id("some-checker-we-have-never-heard-of"), "9"),
        environment: Digest::of(b"an image"),
        inputs: vec![],
        outputs: vec![],
        trust: VerifierTrust::Deterministic,
        verdict: CheckerVerdict::Accepted,
    };
    let (certificate, bundle) = certificate_with(vec![stranger], b"tests passed");

    let outcome = replay(&certificate, &bundle);
    assert_eq!(
        outcome.verdict(),
        Some(AssuranceVerdict::Rejected),
        "an unknown checker claiming determinism is not trusted"
    );
    let ReplayOutcome::Diverged { findings, .. } = outcome else {
        panic!("an unknown deterministic verifier must diverge");
    };
    assert!(
        findings
            .iter()
            .any(|finding| matches!(finding, ReplayFinding::UnknownDeterministicVerifier { .. })),
        "{findings:?}"
    );
}

#[test]
fn a_kernel_version_difference_is_reported_rather_than_hidden() {
    let (certificate, bundle) = certificate_with(vec![oracle()], b"tests passed");
    assert_eq!(certificate.body().kernel_version, KERNEL_VERSION);

    // Rebuild the same body under a different kernel version.
    let mut body = certificate.body().clone();
    body.kernel_version = "capsulet-kernel 0.0.1".to_string();
    let older = Certificate::seal(body).expect("the certificate seals");

    let ReplayOutcome::Reproduced { notes, .. } = replay(&older, &bundle) else {
        panic!("a version difference is a note, not a divergence");
    };
    assert!(
        notes.iter().any(|note| matches!(
            note,
            ReplayNote::KernelVersionDiffers { recorded, .. } if recorded == "capsulet-kernel 0.0.1"
        )),
        "{notes:?}"
    );
}

#[test]
fn a_schema_version_this_build_cannot_read_fails_closed() {
    let (certificate, bundle) = certificate_with(vec![oracle()], b"tests passed");

    let mut body = certificate.body().clone();
    body.schema_version = "capsulet.certificate/v2"
        .parse()
        .expect("the version parses");
    let future = Certificate::seal(body).expect("the certificate seals");

    let outcome = replay(&future, &bundle);
    assert!(
        matches!(
            outcome,
            ReplayOutcome::Unreadable {
                reason: Unreadable::SchemaVersion { .. }
            }
        ),
        "a document from a later schema must not be interpreted: {outcome:?}"
    );
    assert_eq!(
        outcome.verdict(),
        None,
        "an unreadable certificate supports no verdict at all"
    );
}

#[test]
fn the_claim_reasoning_family_is_registered_and_re_decidable() {
    // The family is named in certificates and known to replay.
    assert_eq!(CLAIM_REASONING, "capsulet-kernel/claim-reasoning");
    assert!(
        capsulet_kernel::family::deterministic_families().contains(&CLAIM_REASONING),
        "the kernel's own rules must be re-decidable offline"
    );

    // Recorded as deterministic but with inputs the bundle does not carry:
    // replay says it could not re-decide rather than taking the word for it.
    let family = VerifierRecord {
        identity: Identity::new(id(CLAIM_REASONING), env!("CARGO_PKG_VERSION")),
        environment: Digest::of(b"the kernel"),
        inputs: vec![Digest::of(b"a proposal"), Digest::of(b"a snapshot")],
        outputs: vec![],
        trust: VerifierTrust::Deterministic,
        verdict: CheckerVerdict::Accepted,
    };
    let (certificate, bundle) = certificate_with(vec![family], b"tests passed");

    let ReplayOutcome::Diverged { findings, .. } = replay(&certificate, &bundle) else {
        panic!("a family whose inputs are absent cannot be confirmed");
    };
    assert!(
        findings
            .iter()
            .any(|finding| matches!(finding, ReplayFinding::FamilyInputsMissing { .. })),
        "{findings:?}"
    );
}
