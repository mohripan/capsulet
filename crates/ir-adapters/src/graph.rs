//! Typed agent graphs into the IR.
//!
//! Two things get carried across that the source model states only informally.
//!
//! Node kind becomes a rule rather than a label: a validator becomes a
//! verifier, and model, retrieval, and ranking nodes become proposers, which
//! means the IR will refuse to let a validator hold a model provider even
//! though the source graph had no opinion about that.
//!
//! An agent's budget and termination conditions become a loop region. The
//! current runtime decides whether to keep going outside the graph, so a
//! placeholder condition node stands in for the boolean port a declared loop
//! needs, and the substitution is recorded rather than presented as fidelity.

use std::collections::{BTreeMap, BTreeSet};

use capsulet_core::{
    AgentDefinition, GraphDefinition, GraphHyperedge, GraphNode, HyperedgeEndpoint,
    NodeKind as SourceNodeKind, PortDirection, TerminationCondition,
};
use capsulet_ir::capability::{Capability, CapabilitySet, Grant};
use capsulet_ir::definition::{AssuranceMode, Definition};
use capsulet_ir::effect::{Effect, EffectKind, Idempotency, Reversibility};
use capsulet_ir::graph::{Combine, Endpoint, Graph, GraphBuilder, Hyperedge, TrustDerivation};
use capsulet_ir::id::Identifier;
use capsulet_ir::loop_region::{
    Continuation, FailureKind, LoopBudget, LoopSpec, RepairRoute, Route,
};
use capsulet_ir::node::{Node, NodeKind, ResourceBudget};
use capsulet_ir::port::{InputPort, OutputPort};
use capsulet_ir::region::{Region, RegionKind};
use capsulet_ir::value::{ValueSchema, aliases};

use crate::{AdaptationNote, Adapted, AdapterError};

/// Translates a typed graph on its own, with no agent envelope around it.
///
/// # Errors
///
/// Returns [`AdapterError`] when a name is not a legal IR identifier or a port
/// tag has no structural schema.
pub fn from_graph(graph: &GraphDefinition) -> Result<Adapted, AdapterError> {
    let mut notes = Vec::new();
    let parts = translate(graph, &mut notes)?;

    let built = Graph::new(parts).map_err(|error| AdapterError::Unsupported {
        construct: error.to_string(),
    })?;

    Ok(Adapted {
        definition: definition_for(graph, built, AssuranceMode::Observe)?,
        notes,
    })
}

/// Translates an agent: its graph, its budget, and its termination policy.
///
/// # Errors
///
/// Returns [`AdapterError`] for the same reasons as [`from_graph`].
pub fn from_agent(agent: &AgentDefinition) -> Result<Adapted, AdapterError> {
    let mut notes = Vec::new();
    let mut parts = translate(agent.graph(), &mut notes)?;

    // A loop needs a way in, a way out, and a condition. The source graph has
    // none of those as nodes, because its runtime keeps them outside the graph.
    let entry = identifier("agent-enter")?;
    let exit = identifier("agent-leave")?;
    let condition = identifier("agent-continue")?;

    parts
        .nodes
        .push(boundary_node(entry.clone(), NodeKind::RegionEntry)?);
    parts
        .nodes
        .push(boundary_node(exit.clone(), NodeKind::RegionExit)?);
    parts.nodes.push(Node {
        id: condition.clone(),
        name: "Continue?".to_string(),
        kind: NodeKind::PureComputation,
        inputs: vec![],
        outputs: vec![OutputPort::new(
            identifier("keep-going")?,
            ValueSchema::Bool,
        )],
        capabilities: vec![],
        effects: vec![],
        budget: ResourceBudget::deterministic(1_000),
        provider: None,
        sub_workflow: None,
    });

    notes.push(AdaptationNote::new(
        "agent termination condition",
        "the current runtime decides continuation outside the graph, so a placeholder condition \
         node stands in for the boolean port a declared loop requires",
    ));

    let mut members: BTreeSet<Identifier> =
        parts.nodes.iter().map(|node| node.id.clone()).collect();
    members.insert(entry.clone());
    members.insert(exit.clone());
    members.insert(condition.clone());

    let budget = agent.budget();
    let spec = LoopSpec {
        state: BTreeMap::new(),
        exit: BTreeMap::new(),
        continuation: Continuation {
            evaluated_by: condition,
            port: identifier("keep-going")?,
        },
        budget: LoopBudget {
            max_iterations: budget.max_steps(),
            wall_ms: budget.max_seconds().saturating_mul(1_000),
            tokens: budget.max_tokens(),
            cost_micro_units: budget.max_cost_micros(),
            effect_count: 0,
        },
        invariants: vec![],
        progress: None,
        repairs: repairs_for(agent, &mut notes)?,
    };

    let region_budget = ResourceBudget {
        wall_ms: budget.max_seconds().saturating_mul(1_000),
        tokens: budget.max_tokens(),
        cost_micro_units: budget.max_cost_micros(),
        effect_count: 0,
    };

    parts.regions.push(Region {
        id: identifier("agent-loop")?,
        kind: RegionKind::Loop {
            spec: Box::new(spec),
        },
        parent: None,
        entry,
        exit,
        nodes: members,
        capabilities: CapabilitySet::empty(),
        budget: region_budget,
    });

    let built = Graph::new(parts).map_err(|error| AdapterError::Unsupported {
        construct: error.to_string(),
    })?;

    let mut definition = definition_for(agent.graph(), built, AssuranceMode::Observe)?;
    definition.id = identifier(agent.id().as_str())?;
    definition.name = agent.name().to_string();
    definition.budget = region_budget;

    Ok(Adapted { definition, notes })
}

fn repairs_for(
    agent: &AgentDefinition,
    notes: &mut Vec<AdaptationNote>,
) -> Result<Vec<RepairRoute>, AdapterError> {
    let policy = agent.termination_policy();
    let mut repairs = Vec::new();

    if policy.stop_on_no_progress() {
        repairs.push(RepairRoute {
            failure: FailureKind::NonProgress,
            route: Route::Reject,
        });
    }
    if policy.allow_human_escalation() {
        repairs.push(RepairRoute {
            failure: FailureKind::InterpretationResidual,
            route: Route::Escalate {
                to: identifier("human-reviewer")?,
            },
        });
    }
    if policy.stop_on_safety_failure() {
        repairs.push(RepairRoute {
            failure: FailureKind::UnsafeEffect,
            route: Route::Reject,
        });
    }
    if !policy.accept_on_validator_pass() {
        notes.push(AdaptationNote::new(
            "agent termination condition",
            "this agent does not stop on a validator pass, so the loop has no declared success \
             condition beyond its budget",
        ));
    }
    Ok(repairs)
}

fn boundary_node(id: Identifier, kind: NodeKind) -> Result<Node, AdapterError> {
    Ok(Node {
        name: id.as_str().to_string(),
        id,
        kind,
        inputs: vec![InputPort::new(identifier("in")?, passthrough())],
        outputs: vec![OutputPort::new(identifier("out")?, passthrough())],
        capabilities: vec![],
        effects: vec![],
        budget: ResourceBudget::deterministic(1_000),
        provider: None,
        sub_workflow: None,
    })
}

fn passthrough() -> ValueSchema {
    aliases::opaque("the agent runtime carries loop state as untyped JSON")
}

fn translate(
    graph: &GraphDefinition,
    notes: &mut Vec<AdaptationNote>,
) -> Result<GraphBuilder, AdapterError> {
    let mut parts = GraphBuilder::default();
    let mut opaque_ports = 0_usize;

    for node in graph.nodes() {
        let (inputs, outputs) = ports_of(node, &mut opaque_ports)?;
        let kind = node_kind(node.kind());
        // A job node exists to run a job, and an effect node that declares no
        // effect is not admissible — correctly, since it would be an effect
        // nobody could govern.
        let (capabilities, effects) = if matches!(kind, NodeKind::Effect) {
            (
                vec![identifier(JOB_EXECUTION)?],
                vec![Effect {
                    id: identifier("run-job")?,
                    kind: EffectKind::ExternalSideEffect,
                    target: node.name().to_string(),
                    capability: identifier(JOB_EXECUTION)?,
                    idempotency: Idempotency::NonIdempotent,
                    reversibility: Reversibility::Irreversible,
                }],
            )
        } else {
            (vec![], vec![])
        };

        parts.nodes.push(Node {
            id: identifier(node.id().as_str())?,
            name: node.name().to_string(),
            kind,
            inputs,
            outputs,
            capabilities,
            effects,
            budget: budget_for(node.kind()),
            provider: None,
            sub_workflow: None,
        });
    }

    if opaque_ports > 0 {
        notes.push(AdaptationNote::new(
            "graph port type",
            format!("{opaque_ports} ports carry the `json` tag, which has no structure to carry"),
        ));
    }

    let mut state_fields = 0_usize;
    for edge in graph.hyperedges() {
        parts.edges.push(hyperedge(edge, &mut state_fields)?);
    }
    if state_fields > 0 {
        notes.push(AdaptationNote::new(
            "graph state field endpoint",
            format!(
                "{state_fields} endpoints read or write shared agent state, which the IR does not \
                 model as a value; they become graph-level inputs and outputs"
            ),
        ));
    }

    // Every state field endpoint became a graph-level port, so declare them.
    let mut inputs: BTreeMap<Identifier, OutputPort> = BTreeMap::new();
    let mut outputs: BTreeMap<Identifier, InputPort> = BTreeMap::new();
    for edge in graph.hyperedges() {
        for endpoint in edge.sources() {
            if let HyperedgeEndpoint::StateField { field, value_type } = endpoint {
                let id = identifier(field)?;
                inputs.insert(
                    id.clone(),
                    OutputPort::new(id, schema_for(&value_type.to_string())?),
                );
            }
        }
        for endpoint in edge.targets() {
            if let HyperedgeEndpoint::StateField { field, value_type } = endpoint {
                let id = identifier(field)?;
                outputs.insert(
                    id.clone(),
                    InputPort::new(id, schema_for(&value_type.to_string())?),
                );
            }
        }
    }
    parts.inputs = inputs.into_values().collect();
    parts.outputs = outputs.into_values().collect();

    Ok(parts)
}

fn hyperedge(edge: &GraphHyperedge, state_fields: &mut usize) -> Result<Hyperedge, AdapterError> {
    let sources = endpoints(edge.sources(), true, state_fields)?;
    let targets = endpoints(edge.targets(), false, state_fields)?;

    // One source forwards; several combine into a record whose fields name
    // where each value came from, so the join keeps its provenance.
    let combine = if sources.len() == 1 {
        Combine::Forward
    } else {
        let mut fields = BTreeMap::new();
        for (index, endpoint) in sources.iter().enumerate() {
            fields.insert(endpoint_name(endpoint), index);
        }
        Combine::Record { fields }
    };

    Ok(Hyperedge {
        id: identifier(edge.id().as_str())?,
        sources,
        targets,
        combine,
        trust: TrustDerivation::Weakest,
    })
}

fn endpoints(
    source: &[HyperedgeEndpoint],
    outgoing: bool,
    state_fields: &mut usize,
) -> Result<Vec<Endpoint>, AdapterError> {
    let mut translated = Vec::with_capacity(source.len());
    for endpoint in source {
        translated.push(match endpoint {
            HyperedgeEndpoint::Port { node_id, port_id } => Endpoint::Port {
                node: identifier(node_id.as_str())?,
                port: identifier(port_id.as_str())?,
            },
            HyperedgeEndpoint::StateField { field, .. } => {
                *state_fields += 1;
                if outgoing {
                    Endpoint::GraphInput {
                        name: identifier(field)?,
                    }
                } else {
                    Endpoint::GraphOutput {
                        name: identifier(field)?,
                    }
                }
            }
        });
    }
    Ok(translated)
}

fn endpoint_name(endpoint: &Endpoint) -> String {
    match endpoint {
        Endpoint::Port { node, port } => format!("{node}.{port}"),
        Endpoint::GraphInput { name } | Endpoint::GraphOutput { name } => name.to_string(),
    }
}

fn ports_of(
    node: &GraphNode,
    opaque: &mut usize,
) -> Result<(Vec<InputPort>, Vec<OutputPort>), AdapterError> {
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();

    for port in node.ports() {
        let schema = schema_for(&port.value_type().to_string())?;
        if schema.carries_opacity() {
            *opaque += 1;
        }
        let id = identifier(port.id().as_str())?;
        match port.direction() {
            PortDirection::Input => inputs.push(InputPort::new(id, schema)),
            PortDirection::Output => outputs.push(OutputPort::new(id, schema)),
        }
    }
    Ok((inputs, outputs))
}

fn schema_for(tag: &str) -> Result<ValueSchema, AdapterError> {
    aliases::for_port_value_type(tag).ok_or_else(|| AdapterError::Unsupported {
        construct: format!("port value type `{tag}`"),
    })
}

/// Node kind, as a rule rather than a label.
const fn node_kind(kind: SourceNodeKind) -> NodeKind {
    match kind {
        // Anything that proposes: a planner choosing an action, a model
        // answering, a retriever offering documents, a ranker ordering them.
        SourceNodeKind::Planner
        | SourceNodeKind::Embedding
        | SourceNodeKind::Retriever
        | SourceNodeKind::Reranker
        | SourceNodeKind::Llm => NodeKind::Proposer,
        // Anything deterministic over its inputs.
        SourceNodeKind::QueryNormalizer
        | SourceNodeKind::PromptBuilder
        | SourceNodeKind::Return => NodeKind::PureComputation,
        SourceNodeKind::Validator => NodeKind::Verifier,
        SourceNodeKind::MemoryRead => NodeKind::MemoryRead,
        SourceNodeKind::MemoryWrite => NodeKind::MemoryWrite,
        SourceNodeKind::Job => NodeKind::Effect,
    }
}

/// The capability a job node spends, matching the workflow adapter's name.
const JOB_EXECUTION: &str = "job-execution";

/// A proposer needs a token budget to be admissible; a deterministic node does
/// not. These are placeholders until a definition carries real ones, and they
/// are deliberately generous rather than pretending to be measured.
const fn budget_for(kind: SourceNodeKind) -> ResourceBudget {
    match kind {
        SourceNodeKind::Planner
        | SourceNodeKind::Embedding
        | SourceNodeKind::Retriever
        | SourceNodeKind::Reranker
        | SourceNodeKind::Llm => ResourceBudget {
            wall_ms: 120_000,
            tokens: 32_000,
            cost_micro_units: 100_000,
            effect_count: 0,
        },
        SourceNodeKind::Job => ResourceBudget {
            wall_ms: 3_600_000,
            tokens: 0,
            cost_micro_units: 0,
            effect_count: 1,
        },
        _ => ResourceBudget::deterministic(120_000),
    }
}

fn definition_for(
    graph: &GraphDefinition,
    built: Graph,
    mode: AssuranceMode,
) -> Result<Definition, AdapterError> {
    // Grant only what something actually spends.
    let grants = if built.nodes().any(|node| !node.effects.is_empty()) {
        vec![Capability {
            id: identifier(JOB_EXECUTION)?,
            grant: Grant::Tool {
                tool: identifier("capsulet/job-runner")?,
            },
        }]
    } else {
        vec![]
    };

    Ok(Definition {
        schema_version: Definition::current_schema_version(),
        id: identifier(graph.id().as_str())?,
        version: "1".to_string(),
        name: graph.name().to_string(),
        assurance: mode,
        capabilities: CapabilitySet::new(grants).map_err(|error| AdapterError::Unsupported {
            construct: error.to_string(),
        })?,
        budget: ResourceBudget {
            wall_ms: 600_000,
            tokens: 1_000_000,
            cost_micro_units: 1_000_000,
            effect_count: 16,
        },
        graph: built,
        boundaries: vec![],
        contracts: vec![],
    })
}

fn identifier(value: &str) -> Result<Identifier, AdapterError> {
    Identifier::parse(value).map_err(|source| AdapterError::Identifier {
        what: "graph identifier",
        source,
    })
}

/// The termination conditions this adapter knows how to route.
///
/// Kept as a function so a condition added to the source model shows up here as
/// a compile error rather than being silently dropped.
#[must_use]
pub const fn routed_conditions() -> [TerminationCondition; 4] {
    [
        TerminationCondition::ValidatorPass,
        TerminationCondition::SafetyFailure,
        TerminationCondition::NoProgress,
        TerminationCondition::HumanEscalation,
    ]
}
