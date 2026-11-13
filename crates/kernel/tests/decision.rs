//! A run's certificate, assembled from what its verifier nodes decided.

#[path = "../../ir/tests/fixtures/mod.rs"]
mod fixtures;

use capsulet_ir::correctness::Identity;
use capsulet_ir::correctness::certificate::{
    CheckerVerdict, Subject, VerifierRecord, VerifierTrust,
};
use capsulet_ir::correctness::evidence::{EvidenceRef, RecordedTime};
use capsulet_ir::correctness::obligation::{
    DischargeBasis, DischargeState, Obligation, RepairOwner,
};
use capsulet_ir::correctness::proposal::{Producer, ProducerKind};
use capsulet_ir::{AssuranceMode, AssuranceVerdict, Digest, admit};
use capsulet_kernel::{
    BundleError, EvidenceMap, EvidenceSource, NodeDecision, RunRecord, assemble, certify,
};
use fixtures::{contract, definition_with_scan, id, scan_contract};

fn cited(name: &str, bytes: &[u8]) -> EvidenceRef {
    EvidenceRef {
        id: id(name),
        content: Digest::of(bytes),
        media_type: "text/plain".to_string(),
        byte_length: bytes.len() as u64,
        producer: Producer {
            kind: ProducerKind::Tool,
            identity: Identity::new(id("scanner"), "1"),
        },
        captured_at: RecordedTime(1_794_000_000_000),
    }
}

fn scanner(verdict: CheckerVerdict, outputs: Vec<Digest>) -> VerifierRecord {
    VerifierRecord {
        identity: Identity::new(id("scanner"), "1"),
        environment: Digest::of(b"scanner@sha256:pinned"),
        inputs: vec![],
        outputs,
        trust: VerifierTrust::DeclaredOracle {
            rationale: "a scanner in a pinned image".to_string(),
        },
        verdict,
    }
}

/// A decision of `compiles` by the scanner, resting on `log`.
fn decided(state: DischargeState, log: &[u8], verdict: CheckerVerdict) -> NodeDecision {
    let reference = cited("scanner/log", log);
    let mut bytes = EvidenceMap::new();
    bytes.insert(log.to_vec());
    NodeDecision::new(
        vec![Obligation {
            statement: contract().obligations[0].clone(),
            contract: contract().id,
            state,
        }],
        vec![scanner(verdict, vec![reference.content])],
        vec![reference],
        &bytes,
    )
    .expect("the decision carries what it cites")
}

fn record(decisions: &[NodeDecision]) -> (capsulet_kernel::Assembly, EvidenceMap) {
    let definition = definition_with_scan(AssuranceMode::Enforce);
    let admission = admit(&definition).expect("admitted");
    assemble(RunRecord {
        id: id("certificate-run"),
        definition: &definition,
        admission: admission.clone(),
        mode: AssuranceMode::Enforce,
        subject: Subject {
            definition: *admission.definition(),
            definition_version: "1".to_string(),
            run: Some(id("run")),
            inputs: vec![],
            outputs: vec![],
        },
        loops: vec![],
        decisions,
    })
    .expect("assembles")
}

#[test]
fn every_obligation_is_in_the_certificate_as_the_last_node_left_it_or_residual() {
    let failed = decided(
        DischargeState::Failed {
            reason: "it does not compile".to_string(),
            owner: RepairOwner::Proposer,
        },
        b"error: expected `;`",
        CheckerVerdict::Rejected,
    );
    let log = b"compiled clean";
    let held = decided(
        DischargeState::Discharged {
            by: id("scanner"),
            evidence: vec![Digest::of(log)],
            basis: DischargeBasis::ReplayableTool,
        },
        log,
        CheckerVerdict::Accepted,
    );
    let (assembly, bytes) = record(&[failed, held]);

    // The later decision is the one the run ended with; the obligation no node
    // decided is residual, and says so.
    let states: Vec<(&str, &DischargeState)> = assembly
        .obligations
        .iter()
        .map(|obligation| (obligation.statement.id.as_str(), &obligation.state))
        .collect();
    assert!(matches!(
        states[0],
        ("compiles", DischargeState::Discharged { .. })
    ));
    assert_eq!(states[1].0, scan_contract().obligations[0].id.as_str());
    assert!(matches!(
        states[1].1,
        DischargeState::Residual { rationale, .. } if rationale.contains("no verifier node")
    ));
    assert_eq!(assembly.policy_version, "security-remediation/1");

    // Both runs' records and logs are kept, each once, with their bytes.
    assert_eq!(assembly.verifiers.len(), 2);
    assert_eq!(assembly.evidence.len(), 2);
    for reference in &assembly.evidence {
        assert!(bytes.bytes(&reference.content).is_some());
    }
    let certificate = certify(assembly).expect("seals");
    assert_eq!(certificate.verdict(), AssuranceVerdict::Conditional);
}

#[test]
fn a_decision_whose_bytes_are_not_what_it_cites_does_not_assemble() {
    let decision = decided(
        DischargeState::Residual {
            rationale: "not yet".to_string(),
            evidence: vec![],
        },
        b"the log",
        CheckerVerdict::Conditional,
    );
    let mut value = serde_json::to_value(&decision).expect("encodes");
    let blobs = value["blobs"].as_object_mut().expect("blobs");
    let key = blobs.keys().next().expect("one").clone();
    blobs.insert(key, serde_json::Value::String("dGFtcGVyZWQ=".to_string()));
    let tampered: NodeDecision = serde_json::from_value(value).expect("reads");
    assert!(matches!(
        tampered.bytes(),
        Err(BundleError::BlobDigestMismatch { .. })
    ));

    let missing = NodeDecision::new(
        vec![],
        vec![],
        vec![cited("scanner/log", b"bytes nobody kept")],
        &EvidenceMap::new(),
    );
    assert!(matches!(missing, Err(BundleError::MissingEvidence { .. })));
}
