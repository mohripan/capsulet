//! The IR definition the IR suites store, admit and run.

use std::collections::BTreeMap;

use capsulet_ir::admission::AdmissionRecord;
use capsulet_ir::capability::CapabilitySet;
use capsulet_ir::definition::{AssuranceMode, Definition};
use capsulet_ir::value::LengthBounds;
use capsulet_ir::{
    Graph, GraphBuilder, Identifier, Node, NodeKind, OutputPort, ResourceBudget, ValueSchema, admit,
};

pub fn id(value: &str) -> Identifier {
    Identifier::parse(value).expect("the fixture identifier is valid")
}

/// A one-node, deterministic, enforced definition named `name`.
pub fn definition(name: &str) -> Definition {
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
            parameters: BTreeMap::new(),
        }],
        ..GraphBuilder::default()
    })
    .expect("the fixture graph is valid");

    Definition {
        schema_version: Definition::current_schema_version(),
        id: id(name),
        version: "1".to_string(),
        name: name.to_string(),
        assurance: AssuranceMode::Enforce,
        capabilities: CapabilitySet::empty(),
        budget: ResourceBudget::deterministic(600_000),
        graph,
        boundaries: vec![],
        contracts: vec![],
    }
}

pub fn admission(definition: &Definition) -> AdmissionRecord {
    admit(definition).expect("the fixture definition is admitted")
}
