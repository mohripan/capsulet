#![allow(
    dead_code,
    reason = "each test binary compiles this module and uses a different subset"
)]

//! One definition shared by the tests that need a real, admissible one.
//!
//! A security remediation shaped like the representative workflow in the
//! product design: something prepares a patch, and publishing it is a protected
//! boundary.

use std::collections::BTreeMap;

use capsulet_ir::capability::{Capability, Grant};
use capsulet_ir::correctness::obligation::{Contract, ObligationStatement, RepairOwner};
use capsulet_ir::effect::{Crossing, Idempotency, ProtectedBoundary, Reversibility};
use capsulet_ir::graph::{Combine, TrustDerivation};
use capsulet_ir::value::LengthBounds;
use capsulet_ir::{
    AssuranceMode, CapabilitySet, Definition, Effect, EffectKind, Endpoint, Graph, GraphBuilder,
    Hyperedge, Identifier, InputPort, Node, NodeKind, OutputPort, ResourceBudget, ValueSchema,
};

/// A validated identifier, for tests that know their inputs are well formed.
///
/// # Panics
///
/// Panics if the value is not a legal identifier.
#[must_use]
pub fn id(value: &str) -> Identifier {
    Identifier::parse(value).expect("test identifiers are well formed")
}

/// Length-bounded text.
#[must_use]
pub fn text() -> ValueSchema {
    ValueSchema::Text {
        length: LengthBounds::new(0, 1_024),
    }
}

/// A node with no capabilities and no effects.
#[must_use]
pub fn node(name: &str, kind: NodeKind, inputs: Vec<InputPort>, outputs: Vec<OutputPort>) -> Node {
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

/// The node that opens the pull request, with its declared effect.
#[must_use]
pub fn publish_node() -> Node {
    Node {
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
    }
}

/// `prepare -> publish`.
///
/// # Panics
///
/// Panics if the fixture identifiers collide, which would be a bug in the
/// fixture itself.
#[must_use]
pub fn graph() -> Graph {
    Graph::new(GraphBuilder {
        nodes: vec![
            node(
                "prepare",
                NodeKind::PureComputation,
                vec![],
                vec![OutputPort::new(id("patch"), text())],
            ),
            publish_node(),
        ],
        edges: vec![Hyperedge {
            id: id("prepare-to-publish"),
            sources: vec![Endpoint::Port {
                node: id("prepare"),
                port: id("patch"),
            }],
            targets: vec![Endpoint::Port {
                node: id("publish"),
                port: id("patch"),
            }],
            combine: Combine::Forward,
            trust: TrustDerivation::Weakest,
        }],
        ..GraphBuilder::default()
    })
    .expect("the fixture graph has distinct identifiers")
}

/// The contract the publish boundary is gated on.
#[must_use]
pub fn contract() -> Contract {
    Contract {
        id: id("patch-compiles"),
        version: "1".to_string(),
        inputs: BTreeMap::new(),
        outputs: BTreeMap::new(),
        allowed_effects: vec![EffectKind::Publication],
        obligations: vec![ObligationStatement {
            id: id("compiles"),
            statement: "the patch compiles against the pinned revision".to_string(),
            owner: RepairOwner::Verifier,
        }],
    }
}

/// The publish boundary.
#[must_use]
pub fn boundary() -> ProtectedBoundary {
    ProtectedBoundary {
        id: id("publish-boundary"),
        node: id("publish"),
        crossing: Crossing::Effect {
            effect: id("open-pull-request"),
        },
        description: "Opening a pull request against the customer repository".to_string(),
    }
}

/// The definition, in the given assurance mode.
///
/// # Panics
///
/// Panics if the fixture capabilities collide.
#[must_use]
pub fn definition_in(mode: AssuranceMode) -> Definition {
    Definition {
        schema_version: Definition::current_schema_version(),
        id: id("security-remediation"),
        version: "1".to_string(),
        name: "Security remediation".to_string(),
        assurance: mode,
        capabilities: CapabilitySet::new(vec![Capability {
            id: id("github"),
            grant: Grant::Network {
                hosts: vec!["api.github.com".to_string()],
            },
        }])
        .expect("the fixture grants are distinct"),
        budget: ResourceBudget {
            wall_ms: 600_000,
            tokens: 100_000,
            cost_micro_units: 50_000,
            effect_count: 4,
        },
        graph: graph(),
        boundaries: vec![boundary()],
        contracts: vec![contract()],
    }
}

/// The definition in `Enforce` mode, which is what most tests want.
#[must_use]
pub fn definition() -> Definition {
    definition_in(AssuranceMode::Enforce)
}
