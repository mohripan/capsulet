//! What the worker delegates.
//!
//! The worker knows how to lease, fold, decide, and append. It does not know
//! how to run a verifier, call a model, or open a pull request, and that
//! separation is the point: everything above this trait is testable without a
//! network, and everything below it can fail in whatever way real work fails
//! without the durability story changing.
//!
//! The outcomes are typed rather than `Result<_, Box<dyn Error>>` because a
//! failure's *kind* is what a declared repair route is matched against. An
//! executor that could only say "it went wrong" would leave the routing table
//! in the IR with nothing to route on.

use std::collections::BTreeMap;

use async_trait::async_trait;
use capsulet_ir::definition::Definition;
use capsulet_ir::digest::Digest;
use capsulet_ir::effect::Effect;
use capsulet_ir::graph::{Combine, Endpoint};
use capsulet_ir::id::Identifier;
use capsulet_ir::loop_region::FailureKind;
use capsulet_ir::node::Node;
use capsulet_ir::region::RegionKind;
use capsulet_runtime::event::{ControlValue, NodeUsage};
use capsulet_runtime::state::RunState;

/// Where a value feeding a node's port comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSource {
    /// `node.port`, or the name of the graph input.
    pub name: String,
    /// `None` when the source has produced nothing. That is the executor's to
    /// report, because a node that runs on a value nobody supplied is a node that
    /// made one up.
    pub digest: Option<Digest>,
}

/// One input port, with the sources feeding it and how they combine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedInput {
    pub combine: Combine,
    pub sources: Vec<ResolvedSource>,
}

/// Works out what feeds each of a node's input ports, from the definition and the
/// log alone.
///
/// A region's entry node is where a loop's carried state comes back in: on any
/// iteration after the first, a port the loop declares as state reads what the
/// region's exit produced last time round, rather than the value the run started
/// with. Without that a loop that retries would retry on its original inputs and
/// could never improve on its last attempt.
#[must_use]
pub fn resolve_inputs(
    definition: &Definition,
    state: &RunState,
    node: &Node,
) -> BTreeMap<String, ResolvedInput> {
    let carried = definition
        .graph
        .region_of_node(&node.id)
        .filter(|region| region.entry == node.id)
        .and_then(|region| match &region.kind {
            RegionKind::Loop { spec } if state.loop_progress(&region.id).finished > 0 => {
                Some((spec, state.previous_outputs_of(&region.exit)))
            }
            _ => None,
        });

    let mut inputs = BTreeMap::new();
    for edge in definition.graph.edges() {
        for target in &edge.targets {
            let Endpoint::Port {
                node: target_node,
                port,
            } = target
            else {
                continue;
            };
            if target_node != &node.id {
                continue;
            }
            let sources = edge
                .sources
                .iter()
                .map(|source| match source {
                    Endpoint::Port { node, port } => ResolvedSource {
                        name: format!("{node}.{port}"),
                        digest: state.outputs_of(node).get(port.as_str()).copied(),
                    },
                    Endpoint::GraphInput { name } => {
                        let from_last_iteration = carried.as_ref().and_then(|(spec, outputs)| {
                            spec.state
                                .contains_key(port.as_str())
                                .then(|| outputs.get(port.as_str()).copied())
                                .flatten()
                        });
                        ResolvedSource {
                            name: name.to_string(),
                            digest: from_last_iteration.or_else(|| state.input(name.as_str())),
                        }
                    }
                    // A graph output is a target, and the graph refuses it as a
                    // source at admission. Reported as absent rather than trusted.
                    Endpoint::GraphOutput { name } => ResolvedSource {
                        name: name.to_string(),
                        digest: None,
                    },
                })
                .collect();
            inputs.insert(
                port.to_string(),
                ResolvedInput {
                    combine: edge.combine.clone(),
                    sources,
                },
            );
        }
    }
    inputs
}

/// Everything an executor is told about the work it has been asked to do.
#[derive(Debug, Clone, Copy)]
pub struct NodeRequest<'a> {
    pub tenant: &'a str,
    pub project: &'a str,
    pub run: &'a str,
    pub definition: &'a Definition,
    pub node: &'a Node,
    /// Which iteration of which loop region this is, when the node sits inside
    /// one. A node that runs several times needs to know which time it is.
    pub iteration: Option<(&'a Identifier, u32)>,
    /// What feeds each input port.
    pub inputs: &'a BTreeMap<String, ResolvedInput>,
    /// What this node produced the last time it ran, if it was reset by a loop.
    pub previous_outputs: &'a BTreeMap<String, Digest>,
    /// Who waived this node's failure at a gate, on the attempt after they did.
    pub approved_by: Option<&'a Identifier>,
}

/// How running a node came out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeOutcome {
    Finished {
        /// Output values by port, content-addressed. Digests rather than values
        /// so a large output does not travel through the log.
        outputs: BTreeMap<String, Digest>,
        /// Readings for the ports a loop declares as decision-relevant: a
        /// continuation, an invariant, a progress measure.
        ///
        /// Separate from `outputs` because nobody can evaluate a loop condition
        /// from a digest. An executor that reports none of these for a node a
        /// loop depends on stops the run rather than having a value assumed for
        /// it.
        control: BTreeMap<String, ControlValue>,
        /// What the node consumed. `None` when it consumes nothing worth
        /// reporting; a model call that could not say what it used reports a
        /// [`NodeUsage`] with its figures unknown.
        usage: Option<NodeUsage>,
    },
    Failed {
        failure: FailureKind,
        detail: String,
        /// A failed model call may still have been paid for.
        usage: Option<NodeUsage>,
    },
}

/// An effect the worker has already claimed and is asking to be performed.
#[derive(Debug, Clone, Copy)]
pub struct EffectRequest<'a> {
    pub tenant: &'a str,
    pub project: &'a str,
    pub run: &'a str,
    pub definition: &'a Definition,
    pub node: &'a Identifier,
    pub effect: &'a Effect,
    pub attempt: u32,
    /// The idempotency key to present, where the effect declared one. Presenting
    /// a different key than the last attempt would make a retry a second effect.
    pub key: Option<&'a str>,
    /// What feeds each input port of the node performing the effect.
    pub inputs: &'a BTreeMap<String, ResolvedInput>,
}

/// How performing an effect came out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectOutcome {
    /// It happened, and here is the proof, along with whatever the node produced
    /// by doing it.
    Performed {
        receipt: Digest,
        outputs: BTreeMap<String, Digest>,
        /// Set when the effect happened and the node failed anyway: a job that ran
        /// and came back failed. The claim is finalized, because it did happen, and
        /// the node's failure routes like any other.
        node_failure: Option<(FailureKind, String)>,
    },
    /// It did not happen, and the executor is sure of that.
    ///
    /// Sure is the operative word. Report this only when the far side rejected
    /// the request outright; a timeout is not this.
    Failed {
        failure: FailureKind,
        detail: String,
    },
    /// Nobody can say whether it happened — a timeout, a dropped connection, a
    /// response that never arrived.
    ///
    /// Saying so is far more useful than guessing, because what the runtime
    /// does next follows from the idempotency the IR declared.
    Uncertain { detail: String },
}

/// An effect that happened and now has to be taken back.
#[derive(Debug, Clone, Copy)]
pub struct CompensationRequest<'a> {
    pub tenant: &'a str,
    pub project: &'a str,
    pub run: &'a str,
    pub node: &'a Identifier,
    pub effect: &'a Effect,
    /// The route the IR declared for undoing it.
    pub route: &'a Identifier,
}

/// Performs the work the decision core asks for.
#[async_trait]
pub trait Executor: Send + Sync {
    /// Runs one node.
    async fn run_node(&self, request: NodeRequest<'_>) -> NodeOutcome;

    /// Performs one already-claimed effect.
    async fn perform_effect(&self, request: EffectRequest<'_>) -> EffectOutcome;

    /// Undoes an effect that happened.
    ///
    /// The same outcomes as performing one, and for the same reason: a
    /// compensation that might or might not have landed is exactly as awkward
    /// as an effect that might or might not have landed, and pretending
    /// otherwise would put the uncertainty somewhere nobody looks.
    async fn compensate(&self, request: CompensationRequest<'_>) -> EffectOutcome;
}

impl NodeOutcome {
    /// A node that finished with outputs and nothing a loop reads.
    #[must_use]
    pub fn finished(outputs: BTreeMap<String, Digest>) -> Self {
        Self::Finished {
            outputs,
            control: BTreeMap::new(),
            usage: None,
        }
    }
}

impl EffectOutcome {
    /// An effect that happened and produced nothing the graph reads.
    #[must_use]
    pub const fn performed(receipt: Digest) -> Self {
        Self::Performed {
            receipt,
            outputs: BTreeMap::new(),
            node_failure: None,
        }
    }
}
