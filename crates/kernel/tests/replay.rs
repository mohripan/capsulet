//! Offline replay: same verdict from the same evidence, and a loud failure
//! from anything else.

use capsulet_core::{SourceContent, SourceId};
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
use capsulet_kernel::family::{
    CLAIM_REASONING, CLAIM_REASONING_RULES, ClaimReasoning, ObligationFamily,
};
use capsulet_kernel::replay::{ReplayFinding, ReplayNote, ReplayOutcome, Unreadable};
use capsulet_kernel::snapshot_document::{
    EvidenceDocument, RelationDocument, SnapshotDocument, SourceDocument,
};
use capsulet_kernel::workflow::{Assembly, KERNEL_VERSION, certify};
use capsulet_kernel::{EvidenceMap, Proposal, Proposition, Rule, replay};

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
    assert_eq!(
        outcome.verdict(),
        None,
        "a divergent replay establishes no verdict"
    );
    assert_eq!(
        outcome.recomputed_verdict(),
        Some(AssuranceVerdict::Rejected),
        "asking what the bundle supports now is a separate, deliberate question"
    );
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
    assert_eq!(outcome.verdict(), None);
    assert_eq!(
        outcome.recomputed_verdict(),
        Some(AssuranceVerdict::Rejected)
    );
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
        None,
        "an unknown checker claiming determinism is not trusted"
    );
    assert_eq!(
        outcome.recomputed_verdict(),
        Some(AssuranceVerdict::Rejected)
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
        identity: Identity::new(id(CLAIM_REASONING), CLAIM_REASONING_RULES),
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

#[test]
fn a_bundle_with_nothing_in_it_says_so_rather_than_reproducing_quietly() {
    // A certificate that pins no evidence and names no verifier replays clean,
    // because there is nothing to contradict it. "Reproduced" is true and says
    // much less than it appears to, so the outcome says which one it is.
    let admission = admission();
    let certificate = certify(Assembly {
        id: id("cert-empty"),
        subject: Subject {
            definition: *admission.definition(),
            definition_version: "1".to_string(),
            run: Some(id("run-empty")),
            inputs: vec![],
            outputs: vec![],
        },
        admission,
        mode: AssuranceMode::Enforce,
        policy_version: "release-policy/3".to_string(),
        contracts: vec![id("patch-compiles")],
        verifiers: vec![],
        obligations: vec![Obligation {
            statement: ObligationStatement {
                id: id("compiles"),
                statement: "the patch compiles".to_string(),
                owner: RepairOwner::Verifier,
            },
            contract: id("patch-compiles"),
            state: DischargeState::Discharged {
                by: id("cargo-test"),
                evidence: vec![],
            },
        }],
        evidence: vec![],
        loops: vec![],
    })
    .expect("the certificate seals");

    let ReplayOutcome::Reproduced { notes, verdict } = replay(&certificate, &EvidenceMap::new())
    else {
        panic!("an empty bundle contradicts nothing, so it reproduces");
    };
    assert_eq!(verdict, AssuranceVerdict::Accepted);
    assert!(
        notes.contains(&ReplayNote::NothingWasReChecked),
        "the outcome must say it checked nothing: {notes:?}"
    );
}

#[test]
fn inputs_that_are_present_and_unreadable_are_not_reported_as_missing() {
    // Absent inputs mean the bundle is incomplete. Inputs that are there,
    // digest correctly, and cannot be parsed mean something else entirely, and
    // sending a reader to look for a file that is in front of them wastes the
    // one thing replay is for.
    let deterministic = VerifierRecord {
        identity: Identity::new(id(CLAIM_REASONING), "1.0"),
        environment: Digest::of(b"an image"),
        inputs: vec![Digest::of(b"not a proposal"), Digest::of(b"not a snapshot")],
        outputs: vec![],
        trust: VerifierTrust::Deterministic,
        verdict: CheckerVerdict::Accepted,
    };
    let (certificate, mut bundle) = certificate_with(vec![deterministic], b"tests passed");
    bundle.insert(b"not a proposal".to_vec());
    bundle.insert(b"not a snapshot".to_vec());

    let ReplayOutcome::Diverged { findings, .. } = replay(&certificate, &bundle) else {
        panic!("unreadable inputs must diverge");
    };
    assert!(
        findings.iter().any(|finding| matches!(
            finding,
            ReplayFinding::FamilyInputsMalformed { identity, .. } if identity == CLAIM_REASONING
        )),
        "expected a malformed-input finding, got {findings:?}"
    );
    assert!(
        !findings
            .iter()
            .any(|finding| matches!(finding, ReplayFinding::FamilyInputsMissing { .. })),
        "the bytes are present; nothing is missing: {findings:?}"
    );
}

// --- Re-deciding a real claim-reasoning decision -----------------------------

const GROUNDING_DOC: &str = "Acme renewed the Contoso contract.";

/// The snapshot a claim-reasoning decision was made against, as a bundle pins
/// it: one high-authority source, cited whole.
fn pinned_snapshot() -> SnapshotDocument {
    let hash = SourceContent::new(SourceId::new("src_1").expect("source id"), GROUNDING_DOC)
        .expect("content")
        .content_hash()
        .to_string();
    SnapshotDocument {
        sources: vec![SourceDocument {
            id: "src_1".to_string(),
            tenant_id: "acme".to_string(),
            project_id: "prod".to_string(),
            kind: "document".to_string(),
            uri: None,
            title: "Renewal memo".to_string(),
            authority: "high".to_string(),
            content: GROUNDING_DOC.to_string(),
        }],
        evidence: vec![EvidenceDocument {
            id: "ev_1".to_string(),
            tenant_id: "acme".to_string(),
            project_id: "prod".to_string(),
            source_id: "src_1".to_string(),
            locator: "para-1".to_string(),
            excerpt: GROUNDING_DOC.to_string(),
            observed_at: "2026-03-02T00:00:00Z".to_string(),
            span_start: Some(0),
            span_end: Some(GROUNDING_DOC.len()),
            source_content_hash: Some(hash),
        }],
        claims: vec![],
        authority_floor: None,
        relations: vec![RelationDocument {
            name: "renewed".to_string(),
            stated_by: vec!["{from} renewed {to}".to_string()],
        }],
        cues: vec![],
    }
}

/// A trusted citation of `object` from the pinned snapshot.
fn trusted_citation(object: &str) -> Proposal {
    let goal = Proposition::new("Acme", "renewed", object);
    Proposal {
        goal: goal.clone(),
        derivation: Rule::Trust {
            premise: Box::new(Rule::Cite {
                evidence_id: "ev_1".to_string(),
                proposition: goal,
            }),
            min_authority: "low".to_string(),
        },
    }
}

/// What this build decides about `proposal`, as the claim-reasoning family.
fn decided_here(proposal: &Proposal) -> CheckerVerdict {
    let snapshot = pinned_snapshot().rebuild().expect("the snapshot rebuilds");
    ClaimReasoning::decide(proposal, &snapshot).verdict
}

/// A certificate whose one obligation was discharged by the claim-reasoning
/// family, recorded as having reached `recorded` under rule version `rules`,
/// with the proposal and snapshot it was decided from carried as evidence.
fn claim_reasoning_certificate(
    proposal: &Proposal,
    recorded: CheckerVerdict,
    rules: &str,
) -> (Certificate, EvidenceMap) {
    let proposal_bytes = serde_json::to_vec(proposal).expect("the proposal encodes");
    let snapshot_bytes = serde_json::to_vec(&pinned_snapshot()).expect("the snapshot encodes");

    let mut bundle = EvidenceMap::new();
    let proposal_digest = bundle.insert(proposal_bytes.clone());
    let snapshot_digest = bundle.insert(snapshot_bytes.clone());

    let input = |name: &str, bytes: &[u8]| EvidenceRef {
        id: id(name),
        content: Digest::of(bytes),
        media_type: "application/json".to_string(),
        byte_length: bytes.len() as u64,
        producer: Producer {
            kind: ProducerKind::Deterministic,
            identity: Identity::new(id(CLAIM_REASONING), rules),
        },
        captured_at: RecordedTime(1_772_000_000_000),
    };

    let admission = admission();
    let certificate = certify(Assembly {
        id: id("cert-grounding"),
        subject: Subject {
            definition: *admission.definition(),
            definition_version: "1".to_string(),
            run: Some(id("run-grounding")),
            inputs: vec![],
            outputs: vec![],
        },
        admission,
        mode: AssuranceMode::Enforce,
        policy_version: "release-policy/3".to_string(),
        contracts: vec![id("grounded")],
        verifiers: vec![VerifierRecord {
            identity: Identity::new(id(CLAIM_REASONING), rules),
            environment: Digest::of(b"the kernel"),
            inputs: vec![proposal_digest, snapshot_digest],
            outputs: vec![],
            trust: VerifierTrust::Deterministic,
            verdict: recorded,
        }],
        obligations: vec![Obligation {
            statement: ObligationStatement {
                id: id("cited"),
                statement: "the answer is grounded in its citation".to_string(),
                owner: RepairOwner::Verifier,
            },
            contract: id("grounded"),
            state: DischargeState::Discharged {
                by: id(CLAIM_REASONING),
                evidence: vec![proposal_digest, snapshot_digest],
            },
        }],
        evidence: vec![
            input("proposal", &proposal_bytes),
            input("snapshot", &snapshot_bytes),
        ],
        loops: vec![],
    })
    .expect("the certificate seals");

    (certificate, bundle)
}

fn rules_differ(notes: &[ReplayNote]) -> bool {
    notes.iter().any(|note| {
        matches!(
            note,
            ReplayNote::FamilyRulesDiffer { recorded, replaying, .. }
                if recorded == "0.1.0" && replaying == CLAIM_REASONING_RULES
        )
    })
}

#[test]
fn the_claim_reasoning_family_names_its_rules_not_its_crate() {
    // A version that moves with the crate said nothing: it stayed `0.1.0`
    // through floating point becoming fixed point, NFC, and the span bound, so
    // a replay could not tell one rule set from the next.
    assert_eq!(ClaimReasoning.version(), CLAIM_REASONING_RULES);
    assert_ne!(ClaimReasoning.version(), env!("CARGO_PKG_VERSION"));
}

#[test]
fn a_pinned_decision_re_decides_and_reproduces() {
    // The path every rule change runs through, end to end: a real proposal and
    // a real snapshot, read back out of the bundle and decided again.
    let proposal = trusted_citation("Contoso");
    let (certificate, bundle) =
        claim_reasoning_certificate(&proposal, decided_here(&proposal), CLAIM_REASONING_RULES);

    let ReplayOutcome::Reproduced { notes, .. } = replay(&certificate, &bundle) else {
        panic!("a decision this build would reach again must reproduce");
    };
    assert!(
        notes.iter().any(|note| matches!(
            note,
            ReplayNote::FamilyReDecided { identity } if identity == CLAIM_REASONING
        )),
        "{notes:?}"
    );
    assert!(
        !notes
            .iter()
            .any(|note| matches!(note, ReplayNote::FamilyRulesDiffer { .. })),
        "the rules are the same, so nothing should say otherwise: {notes:?}"
    );
}

#[test]
fn a_decision_recorded_under_other_rules_says_so() {
    let proposal = trusted_citation("Contoso");
    let (certificate, bundle) =
        claim_reasoning_certificate(&proposal, decided_here(&proposal), "0.1.0");

    let ReplayOutcome::Reproduced { notes, .. } = replay(&certificate, &bundle) else {
        panic!("the verdicts agree, so a difference in rules is a note, not a divergence");
    };
    assert!(rules_differ(&notes), "{notes:?}");
}

#[test]
fn a_decision_these_rules_would_not_reach_does_not_reproduce() {
    // Recorded as accepted under older rules; this build rejects it. The note
    // explains the disagreement and does not excuse it: a certificate today's
    // rules would not accept is not reproduced on the strength of older ones.
    let proposal = trusted_citation("Fabrikam");
    assert_eq!(decided_here(&proposal), CheckerVerdict::Rejected);
    let (certificate, bundle) =
        claim_reasoning_certificate(&proposal, CheckerVerdict::Accepted, "0.1.0");

    let ReplayOutcome::Diverged {
        findings, notes, ..
    } = replay(&certificate, &bundle)
    else {
        panic!("a recorded verdict this build does not reach must diverge");
    };
    assert!(
        findings
            .iter()
            .any(|finding| matches!(finding, ReplayFinding::FamilyDisagrees { .. })),
        "{findings:?}"
    );
    assert!(rules_differ(&notes), "{notes:?}");
}
