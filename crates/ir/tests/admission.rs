//! Admission is mandatory, typed, and total.

mod fixtures;

use std::collections::BTreeSet;

use capsulet_ir::admission::{AdmissionCode, AdmissionRefusal};
use capsulet_ir::correctness::obligation::RepairOwner;
use capsulet_ir::effect::Crossing;
use capsulet_ir::graph::{Combine, TrustDerivation};
use capsulet_ir::{
    AssuranceMode, CapabilitySet, Definition, Endpoint, Graph, GraphBuilder, Hyperedge, InputPort,
    NodeKind, OutputPort, Region, RegionKind, ResourceBudget, admit,
};

use fixtures::{definition_in as definition, id, node, publish_node, text};

#[test]
fn a_well_formed_definition_is_admitted_and_the_record_says_what_ran() {
    let record = admit(&definition(AssuranceMode::Enforce)).expect("the definition is admitted");

    assert_eq!(
        record.definition(),
        &capsulet_ir::digest_of(&definition(AssuranceMode::Enforce)).expect("it digests")
    );
    assert_eq!(record.rules_applied().len(), AdmissionCode::all().len());
}

#[test]
fn observe_mode_does_not_switch_admission_off() {
    // The same broken definition, in the most permissive mode there is.
    let mut broken = definition(AssuranceMode::Observe);
    let mut parts = GraphBuilder {
        nodes: vec![node(
            "orphan",
            NodeKind::PureComputation,
            vec![InputPort::new(id("in"), text())],
            vec![],
        )],
        ..GraphBuilder::default()
    };
    parts.edges.push(Hyperedge {
        id: id("from-nowhere"),
        sources: vec![Endpoint::Port {
            node: id("missing"),
            port: id("out"),
        }],
        targets: vec![Endpoint::Port {
            node: id("orphan"),
            port: id("in"),
        }],
        combine: Combine::Forward,
        trust: TrustDerivation::Weakest,
    });
    broken.graph = Graph::new(parts).expect("identifiers are distinct");

    let refusal = admit(&broken).expect_err("observe does not lower the floor");
    assert_eq!(refusal.code, AdmissionCode::GraphInvalid);
    assert_eq!(refusal.owner, RepairOwner::Runtime);
}

#[test]
fn a_refused_definition_produces_no_record_at_all() {
    let mut broken = definition(AssuranceMode::Verify);
    // An effect node whose capability was never granted.
    let mut parts = GraphBuilder {
        nodes: vec![publish_node()],
        ..GraphBuilder::default()
    };
    parts.nodes[0].capabilities = vec![id("kubernetes-admin")];
    parts.nodes[0].effects[0].capability = id("kubernetes-admin");
    broken.graph = Graph::new(parts).expect("identifiers are distinct");

    let outcome = admit(&broken);
    assert!(outcome.is_err(), "the definition must not be admitted");
    // There is no partial record, no `unverified` certificate, nothing: a
    // definition nobody could read has no verdict to report.
    assert_eq!(
        outcome.expect_err("refused").code,
        AdmissionCode::CapabilityUngranted
    );
}

#[test]
fn each_rule_reports_its_own_code_and_owner() {
    // Capability: a grant that was never made.
    let mut ungranted = definition(AssuranceMode::Verify);
    let mut parts = GraphBuilder {
        nodes: vec![publish_node()],
        ..GraphBuilder::default()
    };
    parts.nodes[0].capabilities = vec![id("unknown")];
    parts.nodes[0].effects[0].capability = id("unknown");
    ungranted.graph = Graph::new(parts).expect("identifiers are distinct");
    assert_refusal(
        &ungranted,
        AdmissionCode::CapabilityUngranted,
        RepairOwner::Policy,
    );

    // Boundary over an effect the node does not declare.
    let mut phantom = definition(AssuranceMode::Verify);
    phantom.boundaries[0].crossing = Crossing::Effect {
        effect: id("delete-the-repository"),
    };
    assert_refusal(
        &phantom,
        AdmissionCode::EffectUndeclared,
        RepairOwner::Runtime,
    );

    // Boundary over a node that does not exist.
    let mut absent = definition(AssuranceMode::Verify);
    absent.boundaries[0].node = id("nobody");
    assert_refusal(
        &absent,
        AdmissionCode::BoundaryInvalid,
        RepairOwner::Runtime,
    );

    // Two contracts with one identifier.
    let mut duplicated = definition(AssuranceMode::Verify);
    let contract = duplicated.contracts[0].clone();
    duplicated.contracts.push(contract);
    assert_refusal(
        &duplicated,
        AdmissionCode::ContractInvalid,
        RepairOwner::Runtime,
    );

    // An undeclared cycle.
    let mut cyclic = definition(AssuranceMode::Verify);
    let mut parts = GraphBuilder {
        nodes: vec![
            node(
                "left",
                NodeKind::PureComputation,
                vec![InputPort::new(id("in"), text())],
                vec![OutputPort::new(id("out"), text())],
            ),
            node(
                "right",
                NodeKind::PureComputation,
                vec![InputPort::new(id("in"), text())],
                vec![OutputPort::new(id("out"), text())],
            ),
        ],
        ..GraphBuilder::default()
    };
    for (name, from, to) in [
        ("there", ("left", "out"), ("right", "in")),
        ("back", ("right", "out"), ("left", "in")),
    ] {
        parts.edges.push(Hyperedge {
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
        });
    }
    cyclic.graph = Graph::new(parts).expect("identifiers are distinct");
    cyclic.boundaries.clear();
    assert_refusal(
        &cyclic,
        AdmissionCode::RepetitionUnbounded,
        RepairOwner::Runtime,
    );
}

fn assert_refusal(definition: &Definition, code: AdmissionCode, owner: RepairOwner) {
    let refusal: AdmissionRefusal = admit(definition).expect_err("the definition is refused");
    assert_eq!(refusal.code, code, "detail was: {}", refusal.detail);
    assert_eq!(refusal.owner, owner);
    assert!(
        refusal.to_string().contains(code.as_str()),
        "the message should carry the code: {refusal}"
    );
}

#[test]
fn every_code_has_a_distinct_name_owner_and_description() {
    let names: BTreeSet<&str> = AdmissionCode::all()
        .iter()
        .map(|code| code.as_str())
        .collect();
    assert_eq!(names.len(), AdmissionCode::all().len());

    for code in AdmissionCode::all() {
        assert!(!code.description().is_empty());
        assert_ne!(code.owner().as_str(), "");
    }
}

/// A small deterministic generator. The crate refuses a randomness dependency,
/// and a property test that cannot be replayed is not much of a property test.
struct Noise(u64);

impl Noise {
    fn next(&mut self) -> u64 {
        // xorshift64*, fixed seed, entirely reproducible.
        let mut state = self.0;
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        self.0 = state;
        state.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn pick(&mut self, bound: usize) -> usize {
        let bound = u64::try_from(bound).unwrap_or(u64::MAX);
        usize::try_from(self.next() % bound).unwrap_or(0)
    }
}

#[test]
fn admission_always_reaches_a_decision() {
    let kinds = [
        NodeKind::PureComputation,
        NodeKind::Proposer,
        NodeKind::Verifier,
        NodeKind::Effect,
        NodeKind::HumanGate,
        NodeKind::MemoryRead,
        NodeKind::MemoryWrite,
        NodeKind::SubWorkflow,
        NodeKind::RegionEntry,
        NodeKind::RegionExit,
    ];

    let mut noise = Noise(0x5eed_1234_5678_9abc);
    for case in 0..400_u32 {
        let node_count = 1 + noise.pick(5);
        let mut parts = GraphBuilder::default();
        for index in 0..node_count {
            let mut declared = node(
                &format!("n{index}"),
                kinds[noise.pick(kinds.len())],
                vec![InputPort::new(id("in"), text())],
                vec![OutputPort::new(id("out"), text())],
            );
            if noise.pick(3) == 0 {
                declared.effects = publish_node().effects;
                declared.capabilities = vec![id("github")];
            }
            parts.nodes.push(declared);
        }

        let edge_count = noise.pick(4);
        for index in 0..edge_count {
            let from = noise.pick(node_count);
            let to = noise.pick(node_count);
            parts.edges.push(Hyperedge {
                id: id(&format!("e{index}")),
                sources: vec![Endpoint::Port {
                    node: id(&format!("n{from}")),
                    port: id("out"),
                }],
                targets: vec![Endpoint::Port {
                    node: id(&format!("n{to}")),
                    port: id("in"),
                }],
                combine: Combine::Forward,
                trust: TrustDerivation::Weakest,
            });
        }

        if noise.pick(4) == 0 {
            let mut members = BTreeSet::new();
            for index in 0..node_count {
                members.insert(id(&format!("n{index}")));
            }
            parts.regions.push(Region {
                id: id("r0"),
                kind: RegionKind::Plain,
                parent: None,
                entry: id("n0"),
                exit: id(&format!("n{}", node_count - 1)),
                nodes: members,
                capabilities: CapabilitySet::empty(),
                budget: ResourceBudget::deterministic(1_000),
            });
        }

        let Ok(graph) = Graph::new(parts) else {
            // A duplicate identifier is itself a decision, reached without
            // panicking, which is what this test is about.
            continue;
        };
        let mut candidate = definition(AssuranceMode::Observe);
        candidate.graph = graph;
        candidate.boundaries.clear();

        // The assertion is that this returns at all, for every case, with a
        // decision rather than a panic or a hang.
        let outcome = admit(&candidate);
        assert!(
            outcome.is_ok() || outcome.is_err(),
            "case {case} produced no decision"
        );
    }
}

#[test]
fn the_published_rule_table_matches_the_rules_that_run() {
    use std::fmt::Write as _;

    let mut expected = String::new();
    for code in AdmissionCode::all() {
        let _ = writeln!(
            expected,
            "| `{}` | {} | {} |",
            code.as_str(),
            code.owner().as_str(),
            code.description()
        );
    }

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/contracts/ir-admission-rules.md");

    if std::env::var_os("CAPSULET_UPDATE_GOLDEN").is_some() {
        let document = std::fs::read_to_string(&path).unwrap_or_default();
        let head = document
            .split_once("<!-- generated: admission rules -->")
            .map(|(head, _)| head.to_string())
            .unwrap_or_default();
        std::fs::write(
            &path,
            format!("{head}<!-- generated: admission rules -->\n\n| Code | Owner | Refuses |\n| --- | --- | --- |\n{expected}"),
        )
        .expect("the rule table is writable");
    }

    let published = std::fs::read_to_string(&path).expect("the rule table is readable");
    assert!(
        published.contains(&expected),
        "the published rule table has drifted from the rules that run"
    );
}
