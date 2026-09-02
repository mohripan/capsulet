//! Graph structure: what wiring is meaningful, and what a rejection says.

use std::collections::{BTreeMap, BTreeSet};

use capsulet_ir::graph::{Combine, GraphError, TrustDerivation};
use capsulet_ir::port::TrustRequirement;
use capsulet_ir::value::LengthBounds;
use capsulet_ir::{
    CapabilitySet, ConditionalBranch, Endpoint, Graph, GraphBuilder, Hyperedge, Identifier,
    InputPort, Node, NodeKind, OutputPort, Region, RegionKind, ResourceBudget, TrustLevel,
    ValueSchema, digest_of,
};

fn id(value: &str) -> Identifier {
    Identifier::parse(value).expect("test identifiers are well formed")
}

fn text() -> ValueSchema {
    ValueSchema::Text {
        length: LengthBounds::new(0, 1_024),
    }
}

fn list_of_text() -> ValueSchema {
    ValueSchema::List {
        item: Box::new(text()),
        length: LengthBounds::new(0, 16),
    }
}

fn node(name: &str, kind: NodeKind, inputs: Vec<InputPort>, outputs: Vec<OutputPort>) -> Node {
    Node {
        id: id(name),
        name: name.to_string(),
        kind,
        inputs,
        outputs,
        capabilities: vec![],
        effects: vec![],
        budget: ResourceBudget::deterministic(1_000),
        provider: None,
        sub_workflow: None,
    }
}

fn edge(name: &str, sources: Vec<Endpoint>, targets: Vec<Endpoint>, combine: Combine) -> Hyperedge {
    Hyperedge {
        id: id(name),
        sources,
        targets,
        combine,
        trust: TrustDerivation::Weakest,
    }
}

fn port(node_name: &str, port_name: &str) -> Endpoint {
    Endpoint::Port {
        node: id(node_name),
        port: id(port_name),
    }
}

fn budget() -> ResourceBudget {
    ResourceBudget::deterministic(60_000)
}

/// A two-node pipeline: `normalize` feeds `summarize`.
fn pipeline() -> GraphBuilder {
    GraphBuilder {
        nodes: vec![
            node(
                "normalize",
                NodeKind::PureComputation,
                vec![InputPort::new(id("raw"), text())],
                vec![OutputPort::new(id("clean"), text())],
            ),
            node(
                "summarize",
                NodeKind::PureComputation,
                vec![InputPort::new(id("body"), text())],
                vec![OutputPort::new(id("summary"), text())],
            ),
        ],
        edges: vec![edge(
            "normalize-to-summarize",
            vec![port("normalize", "clean")],
            vec![port("summarize", "body")],
            Combine::Forward,
        )],
        ..GraphBuilder::default()
    }
}

#[test]
fn a_well_formed_pipeline_is_accepted() {
    let graph = Graph::new(pipeline()).expect("identifiers are distinct");
    assert_eq!(graph.check(&CapabilitySet::empty(), &budget()), Ok(()));
}

#[test]
fn authoring_order_does_not_change_the_digest() {
    let forwards = Graph::new(pipeline()).expect("identifiers are distinct");

    let mut reversed = pipeline();
    reversed.nodes.reverse();
    reversed.nodes[0].inputs.reverse();
    let reversed = Graph::new(reversed).expect("identifiers are distinct");

    assert_eq!(
        digest_of(&forwards).expect("the graph digests"),
        digest_of(&reversed).expect("the graph digests")
    );
}

#[test]
fn a_dangling_port_reference_is_refused() {
    let mut parts = pipeline();
    parts.edges[0].targets = vec![port("summarize", "not-a-port")];
    let graph = Graph::new(parts).expect("identifiers are distinct");

    assert_eq!(
        graph.check(&CapabilitySet::empty(), &budget()),
        Err(GraphError::UnknownPort {
            edge: id("normalize-to-summarize"),
            node: id("summarize"),
            port: id("not-a-port"),
            direction: "input",
        })
    );
}

#[test]
fn a_type_incompatible_edge_is_refused_with_the_reason() {
    let mut parts = pipeline();
    parts.nodes[1].inputs = vec![InputPort::new(id("body"), list_of_text())];
    let graph = Graph::new(parts).expect("identifiers are distinct");

    let error = graph
        .check(&CapabilitySet::empty(), &budget())
        .expect_err("text does not satisfy a list");
    assert!(matches!(error, GraphError::Schema { .. }), "found {error}");
    assert!(error.to_string().contains("list"));
}

#[test]
fn a_join_must_account_for_every_source() {
    let mut parts = pipeline();
    parts.nodes.push(node(
        "compare",
        NodeKind::PureComputation,
        vec![InputPort::new(
            id("both"),
            ValueSchema::Record {
                fields: BTreeMap::new(),
            },
        )],
        vec![OutputPort::new(id("verdict"), text())],
    ));

    // Two sources, one named field: the second source vanishes into the join.
    let mut fields = BTreeMap::new();
    fields.insert("left".to_string(), 0_usize);
    parts.edges.push(edge(
        "join",
        vec![port("normalize", "clean"), port("summarize", "summary")],
        vec![port("compare", "both")],
        Combine::Record { fields },
    ));
    let graph = Graph::new(parts).expect("identifiers are distinct");

    assert_eq!(
        graph.check(&CapabilitySet::empty(), &budget()),
        Err(GraphError::ProvenanceLost {
            edge: id("join"),
            sources: 2,
            named: 1,
        })
    );
}

#[test]
fn a_selection_must_record_which_source_won() {
    let mut parts = pipeline();
    let mut arms = BTreeMap::new();
    arms.insert("clean".to_string(), 0_usize);
    arms.insert("summary".to_string(), 1_usize);
    parts.edges.push(Hyperedge {
        id: id("select"),
        sources: vec![port("normalize", "clean"), port("summarize", "summary")],
        targets: vec![Endpoint::GraphOutput { name: id("result") }],
        combine: Combine::Select {
            discriminant: String::new(),
            arms,
        },
        trust: TrustDerivation::Weakest,
    });
    parts.outputs.push(InputPort::new(
        id("result"),
        ValueSchema::Json {
            reason: "the test accepts anything here".to_string(),
        },
    ));
    let graph = Graph::new(parts).expect("identifiers are distinct");

    assert_eq!(
        graph.check(&CapabilitySet::empty(), &budget()),
        Err(GraphError::SelectionNotRecorded { edge: id("select") })
    );
}

#[test]
fn a_forwarding_edge_carries_exactly_one_source() {
    let mut parts = pipeline();
    parts.edges[0].sources = vec![port("normalize", "clean"), port("summarize", "summary")];
    let graph = Graph::new(parts).expect("identifiers are distinct");

    assert_eq!(
        graph.check(&CapabilitySet::empty(), &budget()),
        Err(GraphError::ForwardsMany {
            edge: id("normalize-to-summarize"),
            count: 2,
        })
    );
}

#[test]
fn an_edge_may_not_claim_a_contract_from_a_node_that_is_not_a_verifier() {
    let mut parts = pipeline();
    parts.nodes[1].inputs = vec![InputPort::guarded(
        id("body"),
        text(),
        TrustRequirement::at_least(TrustLevel::Verified, id("reviewed")),
    )];
    parts.edges[0].trust = TrustDerivation::Established {
        contract: id("reviewed"),
        verifier: id("normalize"),
    };
    let graph = Graph::new(parts).expect("identifiers are distinct");

    assert_eq!(
        graph.check(&CapabilitySet::empty(), &budget()),
        Err(GraphError::NotAVerifier {
            edge: id("normalize-to-summarize"),
            verifier: id("normalize"),
        })
    );
}

#[test]
fn an_unverified_value_cannot_reach_a_guarded_input() {
    let mut parts = pipeline();
    parts.nodes[1].inputs = vec![InputPort::guarded(
        id("body"),
        text(),
        TrustRequirement::at_least(TrustLevel::Verified, id("reviewed")),
    )];
    let graph = Graph::new(parts).expect("identifiers are distinct");

    let error = graph
        .check(&CapabilitySet::empty(), &budget())
        .expect_err("unverified text cannot satisfy a verified requirement");
    assert!(
        matches!(error, GraphError::TrustTooWeak { .. }),
        "found {error}"
    );
    assert!(error.to_string().contains("reviewed"));
}

#[test]
fn a_verifier_established_contract_satisfies_a_guarded_input() {
    let mut parts = pipeline();
    parts.nodes[0].kind = NodeKind::Verifier;
    parts.nodes[1].inputs = vec![InputPort::guarded(
        id("body"),
        text(),
        TrustRequirement::at_least(TrustLevel::Verified, id("reviewed")),
    )];
    parts.edges[0].trust = TrustDerivation::Established {
        contract: id("reviewed"),
        verifier: id("normalize"),
    };
    let graph = Graph::new(parts).expect("identifiers are distinct");

    assert_eq!(graph.check(&CapabilitySet::empty(), &budget()), Ok(()));
}

#[test]
fn a_cycle_outside_a_loop_region_is_refused() {
    let mut parts = pipeline();
    parts.edges.push(edge(
        "summarize-back-to-normalize",
        vec![port("summarize", "summary")],
        vec![port("normalize", "raw")],
        Combine::Forward,
    ));
    let graph = Graph::new(parts).expect("identifiers are distinct");

    let error = graph
        .check(&CapabilitySet::empty(), &budget())
        .expect_err("an undeclared cycle is refused");
    assert!(
        matches!(error, GraphError::UndeclaredCycle { .. }),
        "found {error}"
    );
    assert!(error.to_string().contains("normalize"));
}

/// A region containing the whole pipeline, entered and left through its own
/// boundary nodes.
fn region_graph() -> GraphBuilder {
    let mut parts = pipeline();
    parts.nodes.push(node(
        "enter",
        NodeKind::RegionEntry,
        vec![InputPort::new(id("in"), text())],
        vec![OutputPort::new(id("out"), text())],
    ));
    parts.nodes.push(node(
        "leave",
        NodeKind::RegionExit,
        vec![InputPort::new(id("in"), text())],
        vec![OutputPort::new(id("out"), text())],
    ));
    parts.edges.push(edge(
        "enter-to-normalize",
        vec![port("enter", "out")],
        vec![port("normalize", "raw")],
        Combine::Forward,
    ));
    parts.edges.push(edge(
        "summarize-to-leave",
        vec![port("summarize", "summary")],
        vec![port("leave", "in")],
        Combine::Forward,
    ));

    let mut nodes = BTreeSet::new();
    for member in ["enter", "normalize", "summarize", "leave"] {
        nodes.insert(id(member));
    }
    parts.regions.push(Region {
        id: id("body"),
        kind: RegionKind::Plain,
        parent: None,
        entry: id("enter"),
        exit: id("leave"),
        nodes,
        capabilities: CapabilitySet::empty(),
        budget: ResourceBudget::deterministic(30_000),
    });
    parts
}

#[test]
fn a_region_with_proper_boundaries_is_accepted() {
    let graph = Graph::new(region_graph()).expect("identifiers are distinct");
    assert_eq!(graph.check(&CapabilitySet::empty(), &budget()), Ok(()));
}

#[test]
fn a_value_may_not_leave_a_region_except_through_its_exit() {
    let mut parts = region_graph();
    parts.nodes.push(node(
        "outside",
        NodeKind::PureComputation,
        vec![InputPort::new(id("in"), text())],
        vec![OutputPort::new(id("out"), text())],
    ));
    parts.edges.push(edge(
        "smuggle",
        vec![port("normalize", "clean")],
        vec![port("outside", "in")],
        Combine::Forward,
    ));
    let graph = Graph::new(parts).expect("identifiers are distinct");

    assert_eq!(
        graph.check(&CapabilitySet::empty(), &budget()),
        Err(GraphError::ValueEscapesRegion {
            edge: id("smuggle"),
            region: id("body"),
        })
    );
}

#[test]
fn a_region_may_not_widen_the_budget_it_was_given() {
    let mut parts = region_graph();
    parts.regions[0].budget = ResourceBudget::deterministic(600_000);
    let graph = Graph::new(parts).expect("identifiers are distinct");

    let error = graph
        .check(&CapabilitySet::empty(), &budget())
        .expect_err("a region cannot grant itself more than it was given");
    assert!(error.to_string().contains("larger than"), "found {error}");
}

#[test]
fn a_region_boundary_must_be_a_boundary_node() {
    let mut parts = region_graph();
    parts.regions[0].entry = id("normalize");
    let graph = Graph::new(parts).expect("identifiers are distinct");

    let error = graph
        .check(&CapabilitySet::empty(), &budget())
        .expect_err("a computation is not an entry point");
    assert!(error.to_string().contains("region entry"), "found {error}");
}

#[test]
fn a_branch_must_handle_every_case_its_selector_can_produce() {
    let mut parts = pipeline();
    parts.nodes[0].outputs = vec![OutputPort::new(
        id("clean"),
        ValueSchema::Enumeration {
            members: vec![
                "ok".to_string(),
                "retry".to_string(),
                "escalate".to_string(),
            ],
        },
    )];
    parts.nodes[1].inputs = vec![InputPort::new(
        id("body"),
        ValueSchema::Enumeration {
            members: vec![
                "ok".to_string(),
                "retry".to_string(),
                "escalate".to_string(),
            ],
        },
    )];

    let mut arms = BTreeMap::new();
    arms.insert("ok".to_string(), id("summarize"));
    arms.insert("retry".to_string(), id("normalize"));
    parts.branches.push(ConditionalBranch {
        id: id("route"),
        selector: port("normalize", "clean"),
        arms,
    });
    let graph = Graph::new(parts).expect("identifiers are distinct");

    assert_eq!(
        graph.check(&CapabilitySet::empty(), &budget()),
        Err(GraphError::BranchNotExhaustive {
            branch: id("route"),
            member: "escalate".to_string(),
        })
    );
}

#[test]
fn a_duplicate_identifier_is_refused_at_construction() {
    let mut parts = pipeline();
    let duplicate = parts.nodes[0].clone();
    parts.nodes.push(duplicate);

    assert_eq!(
        Graph::new(parts),
        Err(GraphError::Duplicate {
            what: "node",
            id: id("normalize"),
        })
    );
}

#[test]
fn a_graph_output_cannot_be_used_as_a_source() {
    let mut parts = pipeline();
    parts.edges[0].sources = vec![Endpoint::GraphOutput { name: id("result") }];
    let graph = Graph::new(parts).expect("identifiers are distinct");

    assert_eq!(
        graph.check(&CapabilitySet::empty(), &budget()),
        Err(GraphError::OutputUsedAsSource {
            edge: id("normalize-to-summarize"),
        })
    );
}

#[test]
fn concatenation_sums_the_bounds_of_its_sources() {
    let mut parts = GraphBuilder {
        nodes: vec![
            node(
                "first",
                NodeKind::PureComputation,
                vec![],
                vec![OutputPort::new(id("items"), list_of_text())],
            ),
            node(
                "second",
                NodeKind::PureComputation,
                vec![],
                vec![OutputPort::new(id("items"), list_of_text())],
            ),
            node(
                "sink",
                NodeKind::PureComputation,
                vec![InputPort::new(
                    id("all"),
                    ValueSchema::List {
                        item: Box::new(text()),
                        length: LengthBounds::new(0, 32),
                    },
                )],
                vec![],
            ),
        ],
        ..GraphBuilder::default()
    };
    parts.edges.push(edge(
        "concat",
        vec![port("first", "items"), port("second", "items")],
        vec![port("sink", "all")],
        Combine::Concat,
    ));
    let graph = Graph::new(parts).expect("identifiers are distinct");

    // 16 + 16 fits inside 32.
    assert_eq!(graph.check(&CapabilitySet::empty(), &budget()), Ok(()));
}
