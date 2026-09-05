#![allow(
    dead_code,
    reason = "each test binary compiles this module and uses a different subset"
)]

//! Definitions and log helpers the runtime tests share.

use std::collections::{BTreeMap, BTreeSet};

use capsulet_ir::capability::{Capability, Grant};
use capsulet_ir::correctness::evidence::RecordedTime;
use capsulet_ir::effect::{Effect, EffectKind, Idempotency, Reversibility};
use capsulet_ir::graph::{Combine, TrustDerivation};
use capsulet_ir::loop_region::{Continuation, LoopBudget, LoopSpec};
use capsulet_ir::region::{Region, RegionKind};
use capsulet_ir::value::LengthBounds;
use capsulet_ir::{
    AssuranceMode, CapabilitySet, Definition, Digest, Endpoint, Graph, GraphBuilder, Hyperedge,
    Identifier, InputPort, Node, NodeKind, OutputPort, ResourceBudget, ValueSchema,
};
use capsulet_runtime::event::{Epoch, RecordedEvent, RunEvent};

/// A validated identifier.
///
/// # Panics
///
/// Panics when the value is not a legal identifier.
#[must_use]
pub fn id(value: &str) -> Identifier {
    Identifier::parse(value).expect("test identifiers are well formed")
}

/// A moment, as milliseconds since the epoch.
#[must_use]
pub const fn at(millis: i64) -> RecordedTime {
    RecordedTime(millis)
}

#[must_use]
fn text() -> ValueSchema {
    ValueSchema::Text {
        length: LengthBounds::new(0, 1_024),
    }
}

/// One recorded event at a position, under epoch 1.
#[must_use]
pub fn event(position: u64, event: RunEvent) -> RecordedEvent {
    RecordedEvent {
        position,
        epoch: Epoch(1),
        at: RecordedTime(1_772_000_000_000 + i64::try_from(position).unwrap_or(0)),
        event,
    }
}

/// A log containing only the admission event.
#[must_use]
pub fn admitted() -> Vec<RecordedEvent> {
    vec![event(
        0,
        RunEvent::Admitted {
            definition: Digest::of(b"a definition"),
            mode: AssuranceMode::Enforce,
        },
    )]
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

fn definition_with(graph: Graph, capabilities: CapabilitySet) -> Definition {
    Definition {
        schema_version: Definition::current_schema_version(),
        id: id("test-definition"),
        version: "1".to_string(),
        name: "Test definition".to_string(),
        assurance: AssuranceMode::Enforce,
        capabilities,
        budget: ResourceBudget {
            wall_ms: 600_000,
            tokens: 100_000,
            cost_micro_units: 100_000,
            effect_count: 4,
        },
        graph,
        boundaries: vec![],
        contracts: vec![],
    }
}

/// `normalize -> summarize`.
///
/// # Panics
///
/// Panics if the fixture identifiers collide.
#[must_use]
pub fn pipeline_definition() -> Definition {
    let graph = Graph::new(GraphBuilder {
        nodes: vec![
            node(
                "normalize",
                NodeKind::PureComputation,
                vec![],
                vec![OutputPort::new(id("clean"), text())],
            ),
            node(
                "summarize",
                NodeKind::PureComputation,
                vec![InputPort::new(id("body"), text())],
                vec![],
            ),
        ],
        edges: vec![Hyperedge {
            id: id("normalize-to-summarize"),
            sources: vec![Endpoint::Port {
                node: id("normalize"),
                port: id("clean"),
            }],
            targets: vec![Endpoint::Port {
                node: id("summarize"),
                port: id("body"),
            }],
            combine: Combine::Forward,
            trust: TrustDerivation::Weakest,
        }],
        ..GraphBuilder::default()
    })
    .expect("the fixture identifiers are distinct");

    definition_with(graph, CapabilitySet::empty())
}

/// A single effect node, with the idempotency the caller wants to exercise.
///
/// # Panics
///
/// Panics if the fixture identifiers collide.
#[must_use]
pub fn effect_definition(idempotency: Idempotency) -> Definition {
    let publish = Node {
        id: id("publish"),
        name: "Open the pull request".to_string(),
        kind: NodeKind::Effect,
        inputs: vec![],
        outputs: vec![],
        capabilities: vec![id("github")],
        effects: vec![Effect {
            id: id("open-pull-request"),
            kind: EffectKind::Publication,
            target: "github.com/mohripan/capsulet".to_string(),
            capability: id("github"),
            idempotency,
            reversibility: Reversibility::Irreversible,
        }],
        budget: ResourceBudget {
            wall_ms: 30_000,
            tokens: 0,
            cost_micro_units: 0,
            effect_count: 1,
        },
        provider: None,
        sub_workflow: None,
    };

    let graph = Graph::new(GraphBuilder {
        nodes: vec![publish],
        ..GraphBuilder::default()
    })
    .expect("the fixture identifiers are distinct");

    definition_with(
        graph,
        CapabilitySet::new(vec![Capability {
            id: id("github"),
            grant: Grant::Network {
                hosts: vec!["api.github.com".to_string()],
            },
        }])
        .expect("the fixture grants are distinct"),
    )
}

/// A loop region bounded at three iterations.
///
/// # Panics
///
/// Panics if the fixture identifiers collide.
#[must_use]
pub fn loop_definition() -> Definition {
    let mut parts = GraphBuilder {
        nodes: vec![
            node(
                "enter",
                NodeKind::RegionEntry,
                vec![InputPort::new(id("in"), text())],
                vec![OutputPort::new(id("out"), text())],
            ),
            node(
                "check",
                NodeKind::Verifier,
                vec![InputPort::new(id("candidate"), text())],
                vec![OutputPort::new(id("keep-going"), ValueSchema::Bool)],
            ),
            node(
                "leave",
                NodeKind::RegionExit,
                vec![InputPort::new(id("in"), text())],
                vec![OutputPort::new(id("out"), text())],
            ),
        ],
        ..GraphBuilder::default()
    };

    let mut members = BTreeSet::new();
    for member in ["enter", "check", "leave"] {
        members.insert(id(member));
    }
    parts.regions.push(Region {
        id: id("repair-loop"),
        kind: RegionKind::Loop {
            spec: Box::new(LoopSpec {
                state: BTreeMap::new(),
                exit: BTreeMap::new(),
                continuation: Continuation {
                    evaluated_by: id("check"),
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
                repairs: vec![],
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

    let graph = Graph::new(parts).expect("the fixture identifiers are distinct");
    definition_with(graph, CapabilitySet::empty())
}
