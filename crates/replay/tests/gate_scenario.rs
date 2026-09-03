//! The M2 gate, end to end.
//!
//! One definition containing a proposer, a checker, a bounded loop that runs
//! out of budget, and a publication effect behind a protected boundary. It is
//! admitted, certified, bundled, replayed offline by the shipped binary,
//! tampered with, and gated under both Verify and Enforce.
//!
//! Run with `--nocapture` to see the transcript the completion report quotes.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

use capsulet_ir::assurance::{BoundaryDecision, BoundaryPolicy, DenialReason};
use capsulet_ir::capability::{Capability, Grant};
use capsulet_ir::correctness::certificate::{Subject, VerifierRecord, VerifierTrust};
use capsulet_ir::correctness::evidence::EvidenceRef;
use capsulet_ir::correctness::obligation::{
    Contract, DischargeState, ObligationStatement, RepairOwner,
};
use capsulet_ir::correctness::proposal::{Producer, ProducerKind};
use capsulet_ir::effect::{
    Crossing, Effect, EffectKind, Idempotency, ProtectedBoundary, Reversibility,
};
use capsulet_ir::graph::{Combine, TrustDerivation};
use capsulet_ir::loop_region::{
    BudgetKind, Continuation, FailureKind, LoopBudget, LoopOutcome, LoopSpec, RepairRoute, Route,
    StopReason,
};
use capsulet_ir::region::{Region, RegionKind};
use capsulet_ir::value::LengthBounds;
use capsulet_ir::{
    AssuranceMode, AssurancePolicy, AssuranceVerdict, CapabilitySet, CheckerVerdict, Definition,
    Digest, Endpoint, Graph, GraphBuilder, Hyperedge, Identifier, Identity, InputPort, Node,
    NodeKind, Obligation, OutputPort, ProviderBinding, RecordedTime, ResourceBudget, ValueSchema,
    admit, decide_boundary,
};
use capsulet_kernel::bundle::Bundle;
use capsulet_kernel::replay::EvidenceMap;
use capsulet_kernel::workflow::{Assembly, certify};

fn id(value: &str) -> Identifier {
    Identifier::parse(value).expect("the fixture identifier is valid")
}

fn text() -> ValueSchema {
    ValueSchema::Text {
        length: LengthBounds::new(0, 4_096),
    }
}

/// The fixture: propose a patch, check it, publish it behind a boundary.
fn definition() -> Definition {
    let mut parts = GraphBuilder::default();

    parts.nodes.push(Node {
        id: id("enter"),
        name: "Enter the repair loop".to_string(),
        kind: NodeKind::RegionEntry,
        inputs: vec![InputPort::new(id("in"), text())],
        outputs: vec![OutputPort::new(id("out"), text())],
        capabilities: vec![],
        effects: vec![],
        budget: ResourceBudget::deterministic(1_000),
        provider: None,
        sub_workflow: None,
    });
    parts.nodes.push(Node {
        id: id("propose-patch"),
        name: "Propose a patch".to_string(),
        kind: NodeKind::Proposer,
        inputs: vec![InputPort::new(id("findings"), text())],
        outputs: vec![OutputPort::new(id("patch"), text())],
        capabilities: vec![id("local-model")],
        effects: vec![],
        budget: ResourceBudget {
            wall_ms: 120_000,
            tokens: 32_000,
            cost_micro_units: 50_000,
            effect_count: 0,
        },
        provider: Some(ProviderBinding {
            capability: id("local-model"),
            selection: "qwen3:4b".to_string(),
        }),
        sub_workflow: None,
    });
    parts.nodes.push(Node {
        id: id("check-patch"),
        name: "Compile and run the named tests".to_string(),
        kind: NodeKind::Verifier,
        inputs: vec![InputPort::new(id("patch"), text())],
        outputs: vec![
            OutputPort::new(id("keep-going"), ValueSchema::Bool),
            OutputPort::new(id("accepted"), text()),
        ],
        capabilities: vec![],
        effects: vec![],
        budget: ResourceBudget::deterministic(600_000),
        provider: None,
        sub_workflow: None,
    });
    parts.nodes.push(Node {
        id: id("leave"),
        name: "Leave the repair loop".to_string(),
        kind: NodeKind::RegionExit,
        inputs: vec![InputPort::new(id("in"), text())],
        outputs: vec![OutputPort::new(id("out"), text())],
        capabilities: vec![],
        effects: vec![],
        budget: ResourceBudget::deterministic(1_000),
        provider: None,
        sub_workflow: None,
    });
    parts.nodes.push(Node {
        id: id("publish"),
        name: "Open the pull request".to_string(),
        kind: NodeKind::Effect,
        inputs: vec![InputPort::new(id("patch"), text())],
        outputs: vec![],
        capabilities: vec![id("github")],
        effects: vec![Effect {
            id: id("open-pull-request"),
            kind: EffectKind::Publication,
            target: "github.com/mohripan/capsulet".to_string(),
            capability: id("github"),
            idempotency: Idempotency::Keyed {
                key_source: "run_id".to_string(),
            },
            reversibility: Reversibility::Reversible {
                compensation: id("close-pull-request"),
            },
        }],
        budget: ResourceBudget {
            wall_ms: 30_000,
            tokens: 0,
            cost_micro_units: 0,
            effect_count: 1,
        },
        provider: None,
        sub_workflow: None,
    });

    let forward = |name: &str, from: (&str, &str), to: (&str, &str)| Hyperedge {
        id: id(name),
        sources: vec![Endpoint::Port {
            node: id(from.0),
            port: id(from.1),
        }],
        targets: vec![Endpoint::Port {
            node: id(to.0),
            port: id(to.1),
        }],
        combine: Combine::Forward,
        trust: TrustDerivation::Weakest,
    };
    parts.edges = vec![
        forward(
            "enter-to-propose",
            ("enter", "out"),
            ("propose-patch", "findings"),
        ),
        forward(
            "propose-to-check",
            ("propose-patch", "patch"),
            ("check-patch", "patch"),
        ),
        // The repair cycle the loop region authorises.
        forward(
            "check-to-propose",
            ("check-patch", "accepted"),
            ("propose-patch", "findings"),
        ),
        forward(
            "check-to-leave",
            ("check-patch", "accepted"),
            ("leave", "in"),
        ),
        forward("leave-to-publish", ("leave", "out"), ("publish", "patch")),
    ];

    let mut members = BTreeSet::new();
    for member in ["enter", "propose-patch", "check-patch", "leave"] {
        members.insert(id(member));
    }
    parts.regions.push(Region {
        id: id("repair-loop"),
        kind: RegionKind::Loop {
            spec: Box::new(LoopSpec {
                state: BTreeMap::new(),
                exit: BTreeMap::new(),
                continuation: Continuation {
                    evaluated_by: id("check-patch"),
                    port: id("keep-going"),
                },
                budget: LoopBudget {
                    max_iterations: 3,
                    wall_ms: 900_000,
                    tokens: 96_000,
                    cost_micro_units: 150_000,
                    effect_count: 0,
                },
                invariants: vec![],
                progress: None,
                repairs: vec![RepairRoute {
                    failure: FailureKind::InterpretationResidual,
                    route: Route::Escalate {
                        to: id("release-manager"),
                    },
                }],
            }),
        },
        parent: None,
        entry: id("enter"),
        exit: id("leave"),
        nodes: members,
        capabilities: CapabilitySet::empty(),
        budget: ResourceBudget {
            wall_ms: 900_000,
            tokens: 96_000,
            cost_micro_units: 150_000,
            effect_count: 0,
        },
    });

    Definition {
        schema_version: Definition::current_schema_version(),
        id: id("security-remediation"),
        version: "1".to_string(),
        name: "Security remediation".to_string(),
        assurance: AssuranceMode::Enforce,
        capabilities: CapabilitySet::new(vec![
            Capability {
                id: id("local-model"),
                grant: Grant::ModelProvider {
                    provider: id("ollama"),
                    models: vec!["qwen3:4b".to_string()],
                },
            },
            Capability {
                id: id("github"),
                grant: Grant::Network {
                    hosts: vec!["api.github.com".to_string()],
                },
            },
        ])
        .expect("the grants are distinct"),
        budget: ResourceBudget {
            wall_ms: 1_800_000,
            tokens: 128_000,
            cost_micro_units: 250_000,
            effect_count: 4,
        },
        graph: Graph::new(parts).expect("the fixture identifiers are distinct"),
        boundaries: vec![ProtectedBoundary {
            id: id("publish-boundary"),
            node: id("publish"),
            crossing: Crossing::Effect {
                effect: id("open-pull-request"),
            },
            description: "Opening a pull request against the customer repository".to_string(),
        }],
        contracts: vec![Contract {
            id: id("patch-compiles"),
            version: "1".to_string(),
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
            allowed_effects: vec![EffectKind::Publication],
            obligations: vec![ObligationStatement {
                id: id("compiles"),
                statement: "the patch compiles and the named tests pass".to_string(),
                owner: RepairOwner::Verifier,
            }],
        }],
    }
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
fn the_m2_gate_scenario_holds_end_to_end() {
    let definition = definition();

    // 1. Admission.
    let admission = admit(&definition).expect("the definition is structurally admitted");
    println!("admitted: {}", admission.definition());

    // 2. Certification: a residual obligation and a loop that ran out of budget.
    let log = b"2 of 3 named tests pass; coverage for the changed branch is absent";
    let evidence = EvidenceRef {
        id: id("test-log"),
        content: Digest::of(log),
        media_type: "text/plain".to_string(),
        byte_length: log.len() as u64,
        producer: Producer {
            kind: ProducerKind::Deterministic,
            identity: Identity::new(id("cargo-test"), "1.96"),
        },
        captured_at: RecordedTime(1_772_000_000_000),
    };

    let certificate = certify(Assembly {
        id: id("cert-gate"),
        subject: Subject {
            definition: *admission.definition(),
            definition_version: "1".to_string(),
            run: Some(id("run-gate")),
            inputs: vec![Digest::of(b"a pinned repository revision")],
            outputs: vec![Digest::of(b"a proposed patch")],
        },
        admission: admission.clone(),
        mode: AssuranceMode::Enforce,
        policy_version: "release-policy/3".to_string(),
        contracts: vec![id("patch-compiles")],
        verifiers: vec![VerifierRecord {
            identity: Identity::new(id("cargo-test"), "1.96"),
            environment: Digest::of(b"rust:1.96 toolchain image"),
            inputs: vec![Digest::of(b"a proposed patch")],
            outputs: vec![Digest::of(log)],
            trust: VerifierTrust::DeclaredOracle {
                rationale: "the test runner reports its own result".to_string(),
            },
            verdict: CheckerVerdict::Conditional,
        }],
        obligations: vec![
            Obligation {
                statement: ObligationStatement {
                    id: id("compiles"),
                    statement: "the patch compiles and the named tests pass".to_string(),
                    owner: RepairOwner::Verifier,
                },
                contract: id("patch-compiles"),
                state: DischargeState::Discharged {
                    by: id("cargo-test"),
                    evidence: vec![Digest::of(log)],
                },
            },
            Obligation {
                statement: ObligationStatement {
                    id: id("changed-branch-is-covered"),
                    statement: "the changed branch is covered by a test".to_string(),
                    owner: RepairOwner::Human,
                },
                contract: id("patch-compiles"),
                state: DischargeState::Residual {
                    rationale: "the loop ran out of iterations before coverage was added"
                        .to_string(),
                    evidence: vec![Digest::of(log)],
                },
            },
        ],
        evidence: vec![evidence],
        loops: vec![LoopOutcome {
            region: id("repair-loop"),
            iterations: vec![],
            stopped: StopReason::BudgetExhausted {
                budget: BudgetKind::Iterations,
            },
        }],
    })
    .expect("the certificate seals");

    assert_eq!(certificate.verdict(), AssuranceVerdict::Conditional);
    assert_eq!(
        certificate
            .body()
            .incomplete_loops()
            .map(StopReason::as_str)
            .collect::<Vec<_>>(),
        ["budget_exhausted"],
        "a loop that ran out of budget must not read as finished"
    );
    println!(
        "certified: {} ({} residual, stopped: budget_exhausted)",
        certificate.verdict().as_str(),
        certificate.body().residuals().count()
    );

    // 3. Bundling.
    let mut source = EvidenceMap::new();
    source.insert(log.to_vec());
    let bundle = Bundle::build(certificate.clone(), &source).expect("the bundle builds");
    let path = std::env::temp_dir().join("capsulet-m2-gate.bundle.json");
    std::fs::write(
        &path,
        bundle.to_canonical_bytes().expect("the bundle encodes"),
    )
    .expect("the bundle is writable");

    // 4. Offline replay, by the shipped binary, with nothing inherited.
    let output = Command::new(env!("CARGO_BIN_EXE_capsulet-replay"))
        .arg(&path)
        .env_clear()
        .output()
        .expect("the replay binary runs");
    let transcript = String::from_utf8_lossy(&output.stdout).to_string();
    println!("--- replay (clean) ---\n{transcript}");

    assert!(output.status.success(), "{transcript}");
    assert!(
        transcript.contains("reproduced: conditional"),
        "{transcript}"
    );
    assert!(transcript.contains("was not re-run"), "{transcript}");

    // 5. One tampered byte.
    let mut tampered = bundle;
    tampered.tamper_with(&Digest::of(log), b"3 of 3 named tests pass");
    let tampered_path = std::env::temp_dir().join("capsulet-m2-gate-tampered.bundle.json");
    std::fs::write(
        &tampered_path,
        tampered.to_canonical_bytes().expect("the bundle encodes"),
    )
    .expect("the bundle is writable");

    let tampered_output = Command::new(env!("CARGO_BIN_EXE_capsulet-replay"))
        .arg(&tampered_path)
        .env_clear()
        .output()
        .expect("the replay binary runs");
    let tampered_transcript = String::from_utf8_lossy(&tampered_output.stdout).to_string();
    println!("--- replay (tampered) ---\n{tampered_transcript}");

    assert!(!tampered_output.status.success());
    assert!(
        tampered_transcript.contains("recomputed: rejected"),
        "{tampered_transcript}"
    );
    assert!(
        tampered_transcript.contains(&Digest::of(log).to_string()),
        "the failing digest is named: {tampered_transcript}"
    );

    // 6. The same certificate at the same boundary, under two modes.
    let denied = decide_boundary(
        &policy(AssuranceVerdict::Accepted, AssuranceMode::Enforce),
        AssuranceMode::Enforce,
        admission.definition(),
        Some(&certificate),
        &id("publish-boundary"),
    );
    assert_eq!(
        denied,
        BoundaryDecision::Denied {
            reason: DenialReason::VerdictBelowMinimum {
                required: AssuranceVerdict::Accepted,
                found: AssuranceVerdict::Conditional,
            }
        }
    );
    println!("enforce: denied (conditional < accepted)");

    let observed = decide_boundary(
        &policy(AssuranceVerdict::Accepted, AssuranceMode::Verify),
        AssuranceMode::Verify,
        admission.definition(),
        Some(&certificate),
        &id("publish-boundary"),
    );
    assert!(observed.permits_crossing());
    assert!(
        !observed.was_enforced(),
        "verify records the verdict; it does not gate"
    );
    println!("verify: not enforced (verdict recorded, nothing gated)");

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&tampered_path);
}
