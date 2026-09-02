//! The replay binary, exercised the way the gate exercises it: as a process,
//! with its environment scrubbed and nothing running to talk to.

use std::process::{Command, Output};

use capsulet_ir::admission::AdmissionRecord;
use capsulet_ir::correctness::certificate::{
    AssuranceVerdict, Certificate, Subject, VerifierRecord, VerifierTrust,
};
use capsulet_ir::correctness::evidence::EvidenceRef;
use capsulet_ir::correctness::obligation::{DischargeState, ObligationStatement, RepairOwner};
use capsulet_ir::correctness::proposal::{Producer, ProducerKind};
use capsulet_ir::definition::AssuranceMode;
use capsulet_ir::value::LengthBounds;
use capsulet_ir::{
    CapabilitySet, CheckerVerdict, Definition, Digest, Graph, GraphBuilder, Identifier, Identity,
    Node, NodeKind, Obligation, OutputPort, RecordedTime, ResourceBudget, ValueSchema, admit,
};
use capsulet_kernel::bundle::{Bundle, BundleError};
use capsulet_kernel::replay::EvidenceMap;
use capsulet_kernel::workflow::{Assembly, certify};

fn id(value: &str) -> Identifier {
    Identifier::parse(value).expect("test identifiers are well formed")
}

fn definition() -> Definition {
    let graph = Graph::new(GraphBuilder {
        nodes: vec![Node {
            id: id("prepare"),
            name: "Prepare".to_string(),
            kind: NodeKind::PureComputation,
            inputs: vec![],
            outputs: vec![OutputPort::new(
                id("patch"),
                ValueSchema::Text {
                    length: LengthBounds::new(0, 1_024),
                },
            )],
            capabilities: vec![],
            effects: vec![],
            budget: ResourceBudget::deterministic(1_000),
            provider: None,
            sub_workflow: None,
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
        capabilities: CapabilitySet::empty(),
        budget: ResourceBudget::deterministic(600_000),
        graph,
        boundaries: vec![],
        contracts: vec![],
    }
}

fn admission() -> AdmissionRecord {
    admit(&definition()).expect("the fixture definition is admitted")
}

fn certificate(content: &[u8]) -> Certificate {
    let admission = admission();
    let evidence = EvidenceRef {
        id: id("test-log"),
        content: Digest::of(content),
        media_type: "text/plain".to_string(),
        byte_length: content.len() as u64,
        producer: Producer {
            kind: ProducerKind::Deterministic,
            identity: Identity::new(id("cargo-test"), "1.96"),
        },
        captured_at: RecordedTime(1_772_000_000_000),
    };

    certify(Assembly {
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
        verifiers: vec![VerifierRecord {
            identity: Identity::new(id("cargo-test"), "1.96"),
            environment: Digest::of(b"an image"),
            inputs: vec![],
            outputs: vec![],
            trust: VerifierTrust::DeclaredOracle {
                rationale: "the test runner reports its own result".to_string(),
            },
            verdict: CheckerVerdict::Accepted,
        }],
        obligations: vec![Obligation {
            statement: ObligationStatement {
                id: id("compiles"),
                statement: "the patch compiles".to_string(),
                owner: RepairOwner::Verifier,
            },
            contract: id("patch-compiles"),
            state: DischargeState::Discharged {
                by: id("cargo-test"),
                evidence: vec![Digest::of(content)],
            },
        }],
        evidence: vec![evidence],
        loops: vec![],
    })
    .expect("the certificate seals")
}

fn bundle(content: &[u8]) -> Bundle {
    let mut evidence = EvidenceMap::new();
    evidence.insert(content.to_vec());
    Bundle::build(certificate(content), &evidence).expect("the bundle builds")
}

/// Runs the built binary with an empty environment and no working service.
fn run_replay(bundle: &Bundle, name: &str) -> Output {
    let path = std::env::temp_dir().join(format!("capsulet-replay-{name}.bundle.json"));
    std::fs::write(
        &path,
        bundle.to_canonical_bytes().expect("the bundle encodes"),
    )
    .expect("the bundle is writable");

    let output = Command::new(env!("CARGO_BIN_EXE_capsulet-replay"))
        .arg(&path)
        // Nothing inherited: no API URL, no token, no proxy, no database.
        .env_clear()
        .output()
        .expect("the replay binary runs");

    let _ = std::fs::remove_file(&path);
    output
}

#[test]
fn a_complete_bundle_replays_to_its_recorded_verdict() {
    let output = run_replay(&bundle(b"tests passed"), "accepted");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "expected success, got {:?}\n{stdout}",
        output.status
    );
    assert!(stdout.contains("reproduced: accepted"), "{stdout}");
    assert!(
        stdout.contains("was not re-run"),
        "the binary must say what it did not do: {stdout}"
    );
}

#[test]
fn one_tampered_byte_makes_the_binary_exit_non_zero_and_say_why() {
    let mut tampered = bundle(b"tests passed");
    tampered.tamper_with(&Digest::of(b"tests passed"), b"tests passed!");

    let output = run_replay(&tampered, "tampered");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(!output.status.success(), "tampering must fail: {stdout}");
    assert!(stdout.contains("diverged"), "{stdout}");
    assert!(stdout.contains("recorded:   accepted"), "{stdout}");
    assert!(stdout.contains("recomputed: rejected"), "{stdout}");
    assert!(
        stdout.contains(&Digest::of(b"tests passed").to_string()),
        "the failing digest must be named: {stdout}"
    );
}

#[test]
fn a_bundle_that_cannot_be_read_is_a_different_failure_from_a_wrong_verdict() {
    let path = std::env::temp_dir().join("capsulet-replay-garbage.bundle.json");
    std::fs::write(&path, b"not a bundle").expect("the file is writable");

    let output = Command::new(env!("CARGO_BIN_EXE_capsulet-replay"))
        .arg(&path)
        .env_clear()
        .output()
        .expect("the replay binary runs");
    let _ = std::fs::remove_file(&path);

    assert_eq!(
        output.status.code(),
        Some(2),
        "an unreadable bundle exits 2, not 1"
    );
}

#[test]
fn a_bundle_must_carry_every_cited_piece_of_evidence() {
    let empty = EvidenceMap::new();
    assert_eq!(
        Bundle::build(certificate(b"tests passed"), &empty),
        Err(BundleError::MissingEvidence {
            digest: Digest::of(b"tests passed")
        })
    );
}

#[test]
fn a_bundle_may_not_carry_anything_the_certificate_does_not_cite() {
    let mut extra = bundle(b"tests passed");
    extra.tamper_with(&Digest::of(b"something else"), b"something else");

    assert_eq!(
        extra.check(),
        Err(BundleError::UnreferencedBlob {
            digest: Digest::of(b"something else")
        })
    );
}

#[test]
fn a_tampered_blob_is_a_replay_finding_rather_than_a_parse_error() {
    let mut tampered = bundle(b"tests passed");
    tampered.tamper_with(&Digest::of(b"tests passed"), b"tests passed!");

    // A caller who asks explicitly gets told the container is not intact.
    assert!(matches!(
        tampered.check(),
        Err(BundleError::BlobDigestMismatch { .. })
    ));

    // Reading it still succeeds, so replay is what reports the tampering. A
    // container parse error would hide the one signal a reader most needs.
    let bytes = tampered.to_canonical_bytes().expect("the bundle encodes");
    assert!(Bundle::read(&bytes).is_ok());
}

#[test]
fn the_same_certificate_and_evidence_always_produce_the_same_bundle_bytes() {
    let first = bundle(b"tests passed")
        .to_canonical_bytes()
        .expect("the bundle encodes");
    let second = bundle(b"tests passed")
        .to_canonical_bytes()
        .expect("the bundle encodes");

    assert_eq!(first, second);
    assert_eq!(Digest::of(&first), Digest::of(&second));
}

#[test]
fn the_replay_binary_cannot_reach_a_network_or_a_database() {
    let output = Command::new(env!("CARGO"))
        .args([
            "tree",
            "--locked",
            "--package",
            "capsulet-replay",
            "--edges",
            "normal",
            "--prefix",
            "none",
            "--no-dedupe",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo tree runs");

    assert!(output.status.success(), "cargo tree failed");
    let tree = String::from_utf8(output.stdout).expect("cargo tree prints UTF-8");

    let forbidden = [
        "axum",
        "capsulet-api",
        "capsulet-postgres",
        "capsulet-storage",
        "hyper",
        "reqwest",
        "sqlx",
        "sqlx-core",
        "tokio",
        "ureq",
    ];
    let found: Vec<&str> = tree
        .lines()
        .map(|line| line.split_whitespace().next().unwrap_or_default())
        .filter(|name| forbidden.contains(name))
        .collect();

    assert!(
        found.is_empty(),
        "replay must not be able to reach anything but the bundle, but its closure has: {found:?}"
    );
    // The verdict comes from the kernel, so that had better be in there.
    assert!(tree.contains("capsulet-kernel"));
}

#[test]
fn the_bundle_records_its_own_schema_version() {
    let bundle = bundle(b"tests passed");
    assert_eq!(bundle.schema_version.to_string(), "capsulet.bundle/v1");
    assert_eq!(bundle.blob_count(), 1);
    assert_eq!(bundle.certificate.verdict(), AssuranceVerdict::Accepted);
}
