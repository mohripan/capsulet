//! The graph: nodes, the edges between them, and the rules that make an edge
//! meaningful.
//!
//! Two things here are not ordinary graph plumbing.
//!
//! The first is provenance. Every edge that combines values says *how*, and
//! every combination keeps the sources identifiable: fields name their source,
//! concatenation preserves order, and a selection records which arm won. There
//! is no merge policy that produces a value nobody can trace, because a value
//! nobody can trace cannot appear in a certificate that means anything.
//!
//! The second is trust derivation. By default a combined value carries the
//! weakest trust of its inputs. An edge may claim more only by naming a
//! verifier node that establishes a contract over the derivation itself, which
//! is the one honest way for assurance to increase.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::capability::CapabilitySet;
use crate::id::Identifier;
use crate::loop_region::{LoopError, LoopSpec};
use crate::node::{Node, NodeError, NodeKind, ResourceBudget};
use crate::port::{InputPort, OutputPort, TrustLevel, TrustRequirement};
use crate::region::{Region, RegionError, RegionKind};
use crate::trust::TrustClass;
use crate::value::{Field, SchemaMismatch, ValueSchema};

/// One end of an edge.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Endpoint {
    /// A port on a node.
    Port { node: Identifier, port: Identifier },
    /// A value supplied to the graph from outside.
    GraphInput { name: Identifier },
    /// A value the graph hands back.
    GraphOutput { name: Identifier },
}

impl Endpoint {
    /// The node this endpoint belongs to, if any.
    #[must_use]
    pub const fn node(&self) -> Option<&Identifier> {
        match self {
            Self::Port { node, .. } => Some(node),
            Self::GraphInput { .. } | Self::GraphOutput { .. } => None,
        }
    }
}

/// How the sources of an edge produce the value its targets receive.
///
/// Every case keeps the contributing sources identifiable. That is the point of
/// the enum: there is no "just merge them somehow".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Combine {
    /// One source, distributed unchanged to every target.
    Forward,
    /// Each source becomes a named field of a record.
    Record {
        /// Field name to source index. Every source must appear.
        fields: BTreeMap<String, usize>,
    },
    /// Sources concatenated in order into one list.
    Concat,
    /// Exactly one source supplies the value, and the edge records which one
    /// under `discriminant` so the choice is not lost.
    Select {
        discriminant: String,
        /// Variant name to source index. Every source must appear.
        arms: BTreeMap<String, usize>,
    },
}

/// How the trust of the produced value is decided.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrustDerivation {
    /// The weakest trust among the sources. The safe default, and the only one
    /// available without a checker.
    Weakest,
    /// A verifier established a contract over this derivation.
    Established {
        contract: Identifier,
        verifier: Identifier,
    },
}

/// What an edge promises about the trust of the value it delivers.
///
/// A promise is what can be known before the run: at admission time no
/// certificate exists yet, so the graph checks feasibility rather than fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustPromise {
    pub level: TrustLevel,
    pub contract: Option<Identifier>,
}

impl TrustPromise {
    /// Whether a requirement can be met by this promise.
    #[must_use]
    pub fn satisfies(&self, requirement: &TrustRequirement) -> bool {
        if self.level < requirement.minimum {
            return false;
        }
        match (&requirement.contract, &self.contract) {
            (None, _) => true,
            (Some(required), Some(promised)) => required == promised,
            (Some(_), None) => false,
        }
    }
}

/// An edge from one or more sources to one or more targets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hyperedge {
    pub id: Identifier,
    pub sources: Vec<Endpoint>,
    pub targets: Vec<Endpoint>,
    pub combine: Combine,
    pub trust: TrustDerivation,
}

/// An ordering constraint with no data attached.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ControlEdge {
    pub from: Identifier,
    pub to: Identifier,
}

/// A branch that selects one arm from a typed value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConditionalBranch {
    pub id: Identifier,
    /// The value being switched on.
    pub selector: Endpoint,
    /// Member or variant name to the node entered when it is selected.
    pub arms: BTreeMap<String, Identifier>,
}

/// Why a graph was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GraphError {
    #[error("`{what}` `{id}` is declared twice")]
    Duplicate { what: &'static str, id: Identifier },
    #[error("edge `{edge}` references node `{node}`, which the graph does not declare")]
    UnknownNode { edge: Identifier, node: Identifier },
    #[error("edge `{edge}` references `{port}` on node `{node}`, which has no such {direction}")]
    UnknownPort {
        edge: Identifier,
        node: Identifier,
        port: Identifier,
        direction: &'static str,
    },
    #[error("edge `{edge}` references graph {direction} `{name}`, which is not declared")]
    UnknownGraphPort {
        edge: Identifier,
        name: Identifier,
        direction: &'static str,
    },
    #[error(
        "edge `{edge}` uses a graph output as a source; values flow into an output, not out of it"
    )]
    OutputUsedAsSource { edge: Identifier },
    #[error(
        "edge `{edge}` uses a graph input as a target; values flow out of an input, not into it"
    )]
    InputUsedAsTarget { edge: Identifier },
    #[error("edge `{edge}` has no {end}")]
    Dangling { edge: Identifier, end: &'static str },
    #[error("edge `{edge}` forwards {count} sources; forwarding carries exactly one")]
    ForwardsMany { edge: Identifier, count: usize },
    #[error(
        "edge `{edge}` combines {sources} sources but accounts for only {named}; a value nobody \
         can trace back to its sources cannot be certified"
    )]
    ProvenanceLost {
        edge: Identifier,
        sources: usize,
        named: usize,
    },
    #[error("edge `{edge}` selects between sources without recording which one was chosen")]
    SelectionNotRecorded { edge: Identifier },
    #[error("edge `{edge}` names source index {index}, which does not exist")]
    SourceIndexOutOfRange { edge: Identifier, index: usize },
    #[error("edge `{edge}` concatenates values that are not lists of one item type")]
    ConcatNotLists { edge: Identifier },
    #[error("edge `{edge}`: {source}")]
    Schema {
        edge: Identifier,
        #[source]
        source: SchemaMismatch,
    },
    #[error(
        "edge `{edge}` delivers `{promised}` trust to `{target}`, which requires `{required}`{under}"
    )]
    TrustTooWeak {
        edge: Identifier,
        target: Identifier,
        promised: &'static str,
        required: &'static str,
        under: String,
    },
    #[error(
        "edge `{edge}` claims a contract established by `{verifier}`, which is not a verifier node"
    )]
    NotAVerifier {
        edge: Identifier,
        verifier: Identifier,
    },
    #[error("branch `{branch}` switches on a value that is neither an enumeration nor a union")]
    BranchNotSelectable { branch: Identifier },
    #[error("branch `{branch}` has no arm for `{member}`")]
    BranchNotExhaustive { branch: Identifier, member: String },
    #[error("branch `{branch}` has an arm for `{member}`, which the selector can never produce")]
    BranchArmUnreachable { branch: Identifier, member: String },
    #[error(
        "nodes {cycle} form a cycle outside any loop region; repetition must be declared, with its \
         bounds, rather than implied by the wiring"
    )]
    UndeclaredCycle { cycle: String },
    #[error("edge `{edge}` leaves region `{region}` without passing through its exit node")]
    ValueEscapesRegion {
        edge: Identifier,
        region: Identifier,
    },
    #[error("edge `{edge}` enters region `{region}` without passing through its entry node")]
    ValueBypassesRegionEntry {
        edge: Identifier,
        region: Identifier,
    },
    #[error(transparent)]
    Node(#[from] NodeError),
    #[error(transparent)]
    Region(#[from] RegionError),
}

/// A typed graph.
///
/// Collections are keyed maps rather than lists, so two authors who declare the
/// same graph in a different order produce the same canonical bytes and the
/// same digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Graph {
    nodes: BTreeMap<Identifier, Node>,
    edges: BTreeMap<Identifier, Hyperedge>,
    control: BTreeSet<ControlEdge>,
    branches: BTreeMap<Identifier, ConditionalBranch>,
    regions: BTreeMap<Identifier, Region>,
    /// Values supplied from outside. A graph input is a source, so it is
    /// described the way any other source is.
    inputs: BTreeMap<Identifier, OutputPort>,
    /// Values the graph hands back, described as the requirements they impose.
    outputs: BTreeMap<Identifier, InputPort>,
}

/// Everything needed to build a graph, before uniqueness is checked.
#[derive(Debug, Clone, Default)]
pub struct GraphBuilder {
    pub nodes: Vec<Node>,
    pub edges: Vec<Hyperedge>,
    pub control: Vec<ControlEdge>,
    pub branches: Vec<ConditionalBranch>,
    pub regions: Vec<Region>,
    pub inputs: Vec<OutputPort>,
    pub outputs: Vec<InputPort>,
}

impl Graph {
    /// Builds a graph, refusing duplicate identifiers.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::Duplicate`] when any identifier is declared twice.
    pub fn new(parts: GraphBuilder) -> Result<Self, GraphError> {
        Ok(Self {
            nodes: keyed("node", parts.nodes, |node| node.id.clone())?,
            edges: keyed("edge", parts.edges, |edge| edge.id.clone())?,
            control: parts.control.into_iter().collect(),
            branches: keyed("branch", parts.branches, |branch| branch.id.clone())?,
            regions: keyed("region", parts.regions, |region| region.id.clone())?,
            inputs: keyed("graph input", parts.inputs, |port| port.id.clone())?,
            outputs: keyed("graph output", parts.outputs, |port| port.id.clone())?,
        })
    }

    /// The nodes, in canonical order.
    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }

    /// The edges, in canonical order.
    pub fn edges(&self) -> impl Iterator<Item = &Hyperedge> {
        self.edges.values()
    }

    /// The regions, in canonical order.
    pub fn regions(&self) -> impl Iterator<Item = &Region> {
        self.regions.values()
    }

    /// A node by identifier.
    #[must_use]
    pub fn node(&self, id: &Identifier) -> Option<&Node> {
        self.nodes.get(id)
    }

    /// Checks everything about this graph that is decidable from the graph and
    /// the capabilities granted to it.
    ///
    /// # Errors
    ///
    /// Returns the first [`GraphError`] found.
    pub fn check(
        &self,
        granted: &CapabilitySet,
        budget: &ResourceBudget,
    ) -> Result<(), GraphError> {
        for node in self.nodes.values() {
            node.check(granted)?;
        }
        self.check_regions(granted, budget)?;
        for edge in self.edges.values() {
            self.check_edge(edge)?;
        }
        self.check_branches()?;
        self.check_cycles()?;
        Ok(())
    }

    fn check_regions(
        &self,
        granted: &CapabilitySet,
        budget: &ResourceBudget,
    ) -> Result<(), GraphError> {
        let mut owner: BTreeMap<&Identifier, &Identifier> = BTreeMap::new();
        for region in self.regions.values() {
            for node in &region.nodes {
                if !self.nodes.contains_key(node) {
                    return Err(RegionError::UnknownNode {
                        region: region.id.clone(),
                        node: node.clone(),
                    }
                    .into());
                }
                if let Some(other) = owner.insert(node, &region.id) {
                    return Err(RegionError::OverlappingMembership {
                        region: region.id.clone(),
                        other: other.clone(),
                        node: node.clone(),
                    }
                    .into());
                }
            }

            self.check_region_boundary(region, &region.entry, "entry", NodeKind::RegionEntry)?;
            self.check_region_boundary(region, &region.exit, "exit", NodeKind::RegionExit)?;

            let (parent_capabilities, parent_budget) = match &region.parent {
                None => (granted, budget),
                Some(parent) => {
                    let parent =
                        self.regions
                            .get(parent)
                            .ok_or_else(|| RegionError::UnknownParent {
                                region: region.id.clone(),
                                parent: parent.clone(),
                            })?;
                    (&parent.capabilities, &parent.budget)
                }
            };
            region.check_against_parent(parent_capabilities, parent_budget)?;
            self.check_no_cyclic_nesting(region)?;
            if let RegionKind::Loop { spec } = &region.kind {
                self.check_loop(region, spec)?;
            }
        }
        Ok(())
    }

    /// Checks a loop against the nodes it names.
    ///
    /// The declaration can say a condition is boolean; only the graph knows
    /// whether the node it names actually produces one, and an invariant
    /// evaluated by a node outside the loop is not evaluated every iteration.
    fn check_loop(&self, region: &Region, spec: &LoopSpec) -> Result<(), GraphError> {
        spec.check(&region.id).map_err(RegionError::from)?;

        let inside = |node: &Identifier, role: &'static str| -> Result<(), GraphError> {
            if region.contains(node) {
                Ok(())
            } else {
                Err(RegionError::from(LoopError::NodeOutsideLoop {
                    region: region.id.clone(),
                    node: node.clone(),
                    role,
                })
                .into())
            }
        };

        let output = |node: &Identifier, port: &Identifier| -> Option<&ValueSchema> {
            self.nodes.get(node).and_then(|declared| {
                declared
                    .outputs
                    .iter()
                    .find(|candidate| &candidate.id == port)
                    .map(|candidate| &candidate.schema)
            })
        };

        inside(&spec.continuation.evaluated_by, "continuation")?;
        if !matches!(
            output(&spec.continuation.evaluated_by, &spec.continuation.port),
            Some(ValueSchema::Bool)
        ) {
            return Err(RegionError::from(LoopError::ContinuationNotBoolean {
                region: region.id.clone(),
                node: spec.continuation.evaluated_by.clone(),
                port: spec.continuation.port.clone(),
            })
            .into());
        }

        for invariant in &spec.invariants {
            inside(&invariant.evaluator, "invariant evaluator")?;
            if !matches!(
                output(&invariant.evaluator, &invariant.port),
                Some(ValueSchema::Bool)
            ) {
                return Err(RegionError::from(LoopError::ContinuationNotBoolean {
                    region: region.id.clone(),
                    node: invariant.evaluator.clone(),
                    port: invariant.port.clone(),
                })
                .into());
            }
        }

        if let Some(progress) = &spec.progress {
            inside(&progress.measured_by, "progress measure")?;
            if !matches!(
                output(&progress.measured_by, &progress.port),
                Some(ValueSchema::Integer { .. })
            ) {
                return Err(RegionError::from(LoopError::ProgressNotOrdered {
                    region: region.id.clone(),
                    node: progress.measured_by.clone(),
                    port: progress.port.clone(),
                })
                .into());
            }
        }

        Ok(())
    }

    fn check_region_boundary(
        &self,
        region: &Region,
        node: &Identifier,
        role: &'static str,
        expected: NodeKind,
    ) -> Result<(), GraphError> {
        if !region.contains(node) {
            return Err(RegionError::BoundaryOutsideRegion {
                region: region.id.clone(),
                node: node.clone(),
                role,
            }
            .into());
        }
        let declared = self
            .nodes
            .get(node)
            .ok_or_else(|| RegionError::UnknownNode {
                region: region.id.clone(),
                node: node.clone(),
            })?;
        if declared.kind != expected {
            return Err(RegionError::WrongBoundaryKind {
                region: region.id.clone(),
                node: node.clone(),
                role,
                expected: expected.as_str(),
            }
            .into());
        }
        Ok(())
    }

    fn check_no_cyclic_nesting(&self, region: &Region) -> Result<(), GraphError> {
        let mut seen: BTreeSet<&Identifier> = BTreeSet::new();
        let mut current = region;
        while let Some(parent) = &current.parent {
            if !seen.insert(parent) {
                return Err(RegionError::CyclicNesting {
                    region: region.id.clone(),
                }
                .into());
            }
            if parent == &region.id {
                return Err(RegionError::CyclicNesting {
                    region: region.id.clone(),
                }
                .into());
            }
            current = self
                .regions
                .get(parent)
                .ok_or_else(|| RegionError::UnknownParent {
                    region: region.id.clone(),
                    parent: parent.clone(),
                })?;
        }
        Ok(())
    }

    fn region_of(&self, node: &Identifier) -> Option<&Region> {
        self.regions.values().find(|region| region.contains(node))
    }

    fn source_port(
        &self,
        edge: &Hyperedge,
        endpoint: &Endpoint,
    ) -> Result<&OutputPort, GraphError> {
        match endpoint {
            Endpoint::Port { node, port } => {
                let declared = self
                    .nodes
                    .get(node)
                    .ok_or_else(|| GraphError::UnknownNode {
                        edge: edge.id.clone(),
                        node: node.clone(),
                    })?;
                declared
                    .outputs
                    .iter()
                    .find(|candidate| &candidate.id == port)
                    .ok_or_else(|| GraphError::UnknownPort {
                        edge: edge.id.clone(),
                        node: node.clone(),
                        port: port.clone(),
                        direction: "output",
                    })
            }
            Endpoint::GraphInput { name } => {
                self.inputs
                    .get(name)
                    .ok_or_else(|| GraphError::UnknownGraphPort {
                        edge: edge.id.clone(),
                        name: name.clone(),
                        direction: "input",
                    })
            }
            Endpoint::GraphOutput { .. } => Err(GraphError::OutputUsedAsSource {
                edge: edge.id.clone(),
            }),
        }
    }

    fn target_port(&self, edge: &Hyperedge, endpoint: &Endpoint) -> Result<&InputPort, GraphError> {
        match endpoint {
            Endpoint::Port { node, port } => {
                let declared = self
                    .nodes
                    .get(node)
                    .ok_or_else(|| GraphError::UnknownNode {
                        edge: edge.id.clone(),
                        node: node.clone(),
                    })?;
                declared
                    .inputs
                    .iter()
                    .find(|candidate| &candidate.id == port)
                    .ok_or_else(|| GraphError::UnknownPort {
                        edge: edge.id.clone(),
                        node: node.clone(),
                        port: port.clone(),
                        direction: "input",
                    })
            }
            Endpoint::GraphOutput { name } => {
                self.outputs
                    .get(name)
                    .ok_or_else(|| GraphError::UnknownGraphPort {
                        edge: edge.id.clone(),
                        name: name.clone(),
                        direction: "output",
                    })
            }
            Endpoint::GraphInput { .. } => Err(GraphError::InputUsedAsTarget {
                edge: edge.id.clone(),
            }),
        }
    }

    fn check_edge(&self, edge: &Hyperedge) -> Result<(), GraphError> {
        if edge.sources.is_empty() {
            return Err(GraphError::Dangling {
                edge: edge.id.clone(),
                end: "sources",
            });
        }
        if edge.targets.is_empty() {
            return Err(GraphError::Dangling {
                edge: edge.id.clone(),
                end: "targets",
            });
        }

        let mut sources = Vec::with_capacity(edge.sources.len());
        for endpoint in &edge.sources {
            sources.push(self.source_port(edge, endpoint)?);
        }

        let delivered = Self::delivered_schema(edge, &sources)?;
        let promise = self.trust_promise(edge, &sources)?;

        for endpoint in &edge.targets {
            let target = self.target_port(edge, endpoint)?;
            delivered
                .check_satisfies(&target.schema)
                .map_err(|source| GraphError::Schema {
                    edge: edge.id.clone(),
                    source,
                })?;
            if !promise.satisfies(&target.requires) {
                return Err(GraphError::TrustTooWeak {
                    edge: edge.id.clone(),
                    target: target.id.clone(),
                    promised: promise.level.as_str(),
                    required: target.requires.minimum.as_str(),
                    under: target
                        .requires
                        .contract
                        .as_ref()
                        .map_or_else(String::new, |contract| {
                            format!(" under contract `{contract}`")
                        }),
                });
            }
        }

        self.check_edge_regions(edge)
    }

    fn delivered_schema(
        edge: &Hyperedge,
        sources: &[&OutputPort],
    ) -> Result<ValueSchema, GraphError> {
        let index = |position: &usize| -> Result<&&OutputPort, GraphError> {
            sources
                .get(*position)
                .ok_or_else(|| GraphError::SourceIndexOutOfRange {
                    edge: edge.id.clone(),
                    index: *position,
                })
        };

        match &edge.combine {
            Combine::Forward => {
                if sources.len() != 1 {
                    return Err(GraphError::ForwardsMany {
                        edge: edge.id.clone(),
                        count: sources.len(),
                    });
                }
                Ok(sources[0].schema.clone())
            }
            Combine::Record { fields } => {
                Self::check_covers_every_source(edge, sources.len(), fields.values().copied())?;
                let mut record = BTreeMap::new();
                for (name, position) in fields {
                    record.insert(
                        name.clone(),
                        Field::required(index(position)?.schema.clone()),
                    );
                }
                Ok(ValueSchema::Record { fields: record })
            }
            Combine::Concat => {
                let mut item: Option<ValueSchema> = None;
                let mut minimum = 0_u32;
                let mut maximum = 0_u32;
                for source in sources {
                    let ValueSchema::List {
                        item: source_item,
                        length,
                    } = &source.schema
                    else {
                        return Err(GraphError::ConcatNotLists {
                            edge: edge.id.clone(),
                        });
                    };
                    minimum = minimum.saturating_add(length.minimum);
                    maximum = maximum.saturating_add(length.maximum);
                    if let Some(expected) = &item {
                        source_item.check_satisfies(expected).map_err(|source| {
                            GraphError::Schema {
                                edge: edge.id.clone(),
                                source,
                            }
                        })?;
                    } else {
                        item = Some((**source_item).clone());
                    }
                }
                let item = item.ok_or_else(|| GraphError::Dangling {
                    edge: edge.id.clone(),
                    end: "sources",
                })?;
                Ok(ValueSchema::List {
                    item: Box::new(item),
                    length: crate::value::LengthBounds::new(minimum, maximum),
                })
            }
            Combine::Select { discriminant, arms } => {
                if discriminant.trim().is_empty() {
                    return Err(GraphError::SelectionNotRecorded {
                        edge: edge.id.clone(),
                    });
                }
                Self::check_covers_every_source(edge, sources.len(), arms.values().copied())?;
                let mut variants = BTreeMap::new();
                for (name, position) in arms {
                    variants.insert(name.clone(), index(position)?.schema.clone());
                }
                Ok(ValueSchema::Union {
                    discriminant: discriminant.clone(),
                    variants,
                })
            }
        }
    }

    fn check_covers_every_source(
        edge: &Hyperedge,
        sources: usize,
        named: impl Iterator<Item = usize>,
    ) -> Result<(), GraphError> {
        let named: BTreeSet<usize> = named.collect();
        for position in &named {
            if *position >= sources {
                return Err(GraphError::SourceIndexOutOfRange {
                    edge: edge.id.clone(),
                    index: *position,
                });
            }
        }
        if named.len() != sources {
            return Err(GraphError::ProvenanceLost {
                edge: edge.id.clone(),
                sources,
                named: named.len(),
            });
        }
        Ok(())
    }

    fn trust_promise(
        &self,
        edge: &Hyperedge,
        sources: &[&OutputPort],
    ) -> Result<TrustPromise, GraphError> {
        match &edge.trust {
            TrustDerivation::Weakest => {
                let classes: Vec<TrustClass> = sources
                    .iter()
                    .map(|source| source.produces().clone())
                    .collect();
                let combined = TrustClass::meet_all(classes.iter());
                Ok(TrustPromise {
                    level: TrustLevel::of(&combined),
                    contract: combined
                        .contract()
                        .and_then(|contract| Identifier::parse(contract).ok()),
                })
            }
            TrustDerivation::Established { contract, verifier } => {
                let node = self
                    .nodes
                    .get(verifier)
                    .ok_or_else(|| GraphError::UnknownNode {
                        edge: edge.id.clone(),
                        node: verifier.clone(),
                    })?;
                if node.kind != NodeKind::Verifier {
                    return Err(GraphError::NotAVerifier {
                        edge: edge.id.clone(),
                        verifier: verifier.clone(),
                    });
                }
                Ok(TrustPromise {
                    level: TrustLevel::Verified,
                    contract: Some(contract.clone()),
                })
            }
        }
    }

    fn check_edge_regions(&self, edge: &Hyperedge) -> Result<(), GraphError> {
        for source in &edge.sources {
            let Some(node) = source.node() else { continue };
            let Some(region) = self.region_of(node) else {
                continue;
            };
            for target in &edge.targets {
                let inside = target.node().is_some_and(|target| region.contains(target));
                if !inside && node != &region.exit {
                    return Err(GraphError::ValueEscapesRegion {
                        edge: edge.id.clone(),
                        region: region.id.clone(),
                    });
                }
            }
        }

        for target in &edge.targets {
            let Some(node) = target.node() else { continue };
            let Some(region) = self.region_of(node) else {
                continue;
            };
            for source in &edge.sources {
                let inside = source.node().is_some_and(|source| region.contains(source));
                if !inside && node != &region.entry {
                    return Err(GraphError::ValueBypassesRegionEntry {
                        edge: edge.id.clone(),
                        region: region.id.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    fn check_branches(&self) -> Result<(), GraphError> {
        for branch in self.branches.values() {
            let selector = match &branch.selector {
                Endpoint::Port { node, port } => self
                    .nodes
                    .get(node)
                    .and_then(|declared| {
                        declared
                            .outputs
                            .iter()
                            .find(|candidate| &candidate.id == port)
                    })
                    .map(|declared| declared.schema.clone()),
                Endpoint::GraphInput { name } => {
                    self.inputs.get(name).map(|port| port.schema.clone())
                }
                Endpoint::GraphOutput { .. } => None,
            }
            .ok_or_else(|| GraphError::BranchNotSelectable {
                branch: branch.id.clone(),
            })?;

            let members: BTreeSet<String> = match &selector {
                ValueSchema::Enumeration { members } => members.iter().cloned().collect(),
                ValueSchema::Union { variants, .. } => variants.keys().cloned().collect(),
                _ => {
                    return Err(GraphError::BranchNotSelectable {
                        branch: branch.id.clone(),
                    });
                }
            };

            for member in &members {
                if !branch.arms.contains_key(member) {
                    return Err(GraphError::BranchNotExhaustive {
                        branch: branch.id.clone(),
                        member: member.clone(),
                    });
                }
            }
            for (member, node) in &branch.arms {
                if !members.contains(member) {
                    return Err(GraphError::BranchArmUnreachable {
                        branch: branch.id.clone(),
                        member: member.clone(),
                    });
                }
                if !self.nodes.contains_key(node) {
                    return Err(GraphError::UnknownNode {
                        edge: branch.id.clone(),
                        node: node.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Every ordering constraint, from control edges and from data flow alike.
    fn adjacency(&self) -> BTreeMap<&Identifier, BTreeSet<&Identifier>> {
        let mut adjacency: BTreeMap<&Identifier, BTreeSet<&Identifier>> = BTreeMap::new();
        for edge in self.edges.values() {
            for source in edge.sources.iter().filter_map(Endpoint::node) {
                for target in edge.targets.iter().filter_map(Endpoint::node) {
                    adjacency.entry(source).or_default().insert(target);
                }
            }
        }
        for edge in &self.control {
            adjacency.entry(&edge.from).or_default().insert(&edge.to);
        }
        adjacency
    }

    fn check_cycles(&self) -> Result<(), GraphError> {
        let adjacency = self.adjacency();
        let mut visited: BTreeSet<&Identifier> = BTreeSet::new();
        let mut stack: Vec<&Identifier> = Vec::new();
        let mut on_stack: BTreeSet<&Identifier> = BTreeSet::new();

        for start in self.nodes.keys() {
            if visited.contains(start) {
                continue;
            }
            if let Some(cycle) =
                Self::find_cycle(start, &adjacency, &mut visited, &mut stack, &mut on_stack)
            {
                let inside_one_loop = self.regions.values().any(|region| {
                    region.kind.permits_cycles() && cycle.iter().all(|node| region.contains(node))
                });
                if !inside_one_loop {
                    let rendered: Vec<String> =
                        cycle.iter().map(|node| format!("`{node}`")).collect();
                    return Err(GraphError::UndeclaredCycle {
                        cycle: rendered.join(" -> "),
                    });
                }
            }
        }
        Ok(())
    }

    fn find_cycle<'a>(
        node: &'a Identifier,
        adjacency: &BTreeMap<&'a Identifier, BTreeSet<&'a Identifier>>,
        visited: &mut BTreeSet<&'a Identifier>,
        stack: &mut Vec<&'a Identifier>,
        on_stack: &mut BTreeSet<&'a Identifier>,
    ) -> Option<Vec<Identifier>> {
        visited.insert(node);
        stack.push(node);
        on_stack.insert(node);

        if let Some(next) = adjacency.get(node) {
            for target in next {
                if on_stack.contains(target) {
                    let start = stack.iter().position(|entry| entry == target).unwrap_or(0);
                    let cycle = stack[start..]
                        .iter()
                        .map(|entry| (*entry).clone())
                        .collect();
                    stack.pop();
                    on_stack.remove(node);
                    return Some(cycle);
                }
                if !visited.contains(target)
                    && let Some(cycle) =
                        Self::find_cycle(target, adjacency, visited, stack, on_stack)
                {
                    stack.pop();
                    on_stack.remove(node);
                    return Some(cycle);
                }
            }
        }

        stack.pop();
        on_stack.remove(node);
        None
    }
}

fn keyed<T>(
    what: &'static str,
    values: Vec<T>,
    key: impl Fn(&T) -> Identifier,
) -> Result<BTreeMap<Identifier, T>, GraphError> {
    let mut map = BTreeMap::new();
    for value in values {
        let id = key(&value);
        if map.contains_key(&id) {
            return Err(GraphError::Duplicate { what, id });
        }
        map.insert(id, value);
    }
    Ok(map)
}
