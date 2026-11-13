//! Typed agent graphs into the IR.
//!
//! Two things get carried across that the source model states only informally.
//!
//! Node kind becomes a rule rather than a label: a validator becomes a
//! verifier, and model, retrieval, and ranking nodes become proposers, which
//! means the IR will refuse to let a validator hold a model provider even
//! though the source graph had no opinion about that.
//!
//! An agent's budget and termination conditions become a loop region, whose
//! continuation a deterministic node decides from what the agent's validators
//! reported. See [`from_agent`] for the shape.

use std::collections::{BTreeMap, BTreeSet};

use capsulet_core::{
    AgentDefinition, GraphDefinition, GraphHyperedge, GraphNode, GraphTransitionMode,
    HyperedgeEndpoint, NodeKind as SourceNodeKind, PortDirection, TerminationCondition,
};
use capsulet_ir::agent::{
    AGENT_CONTINUE, AGENT_ENTER, AGENT_LEAVE, CHECK_PORT_PREFIX, DISTINCT_RESULTS, HUMAN_REVIEWER,
    KEEP_GOING, KIND_PARAMETER, RETRY_PARAMETER, SEEN_RESULTS, STATE_PORT_PREFIX,
};
use capsulet_ir::capability::{Capability, CapabilitySet, Grant};
use capsulet_ir::definition::{AssuranceMode, Definition};
use capsulet_ir::effect::{Effect, EffectKind, Idempotency, Reversibility};
use capsulet_ir::graph::{
    Combine, ControlEdge, Endpoint, Graph, GraphBuilder, Hyperedge, TrustDerivation,
};
use capsulet_ir::id::Identifier;
use capsulet_ir::loop_region::{
    Continuation, FailureKind, LoopBudget, LoopSpec, ProgressDirection, ProgressMeasure,
    RepairRoute, Route,
};
use capsulet_ir::node::{Node, NodeKind, ProviderBinding, ResourceBudget};
use capsulet_ir::port::{InputPort, OutputPort};
use capsulet_ir::region::{Region, RegionKind};
use capsulet_ir::value::{IntegerRange, ValueSchema, aliases};

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

/// Translates an agent into a definition a worker can run.
///
/// The graph becomes the body of one loop region, and the region is what the
/// agent's budget and termination policy bound:
///
/// ```text
/// inputs -> agent-enter -> (the graph) -> agent-continue -> agent-leave -> outputs
/// ```
///
/// A state field the graph reads comes out of `agent-enter`, and one it writes goes
/// into `agent-leave`, because a value may only cross a region's edge through its
/// boundary. A field both read and written is loop state: the next iteration starts
/// from what the last one left there, which is how a retry gets to see its own last
/// attempt.
///
/// # Errors
///
/// Returns [`AdapterError`] when a name is not a legal IR identifier, a port tag has
/// no structural schema, or the graph uses something this translation refuses rather
/// than approximates: planner transitions, cycles, a port fed twice, a typed input
/// nothing feeds, a model node naming no model, or a validator with no rule.
pub fn from_agent(agent: &AgentDefinition) -> Result<Adapted, AdapterError> {
    let graph = agent.graph();
    refuse_what_cannot_run(graph)?;

    let mut losses = Vec::new();
    let names = LoopNames::new()?;
    let fields = state_fields(graph);

    // The graph's own nodes, with what each one does carried as parameters so the
    // definition's digest covers it.
    let mut opaque_ports = 0_usize;
    let mut nodes = Vec::with_capacity(graph.nodes().len() + 3);
    for node in graph.nodes() {
        nodes.push(agent_node(node, &mut opaque_ports)?);
    }

    // Every edge of the graph, with state fields routed through the boundary and
    // written fields also shown to the node that notices a repeated result.
    let mut edges = Vec::with_capacity(graph.hyperedges().len());
    let mut fed: BTreeSet<(String, String)> = BTreeSet::new();
    for edge in graph.hyperedges() {
        edges.push(agent_edge(edge, &names, &mut fed)?);
    }
    refuse_unfed_inputs(graph, &fed)?;

    let mut decider_inputs = wire_validators(graph, &names, &mut edges)?;
    let boundary = wire_boundary(&fields, &names, &mut edges, &mut decider_inputs)?;

    nodes.push(boundary_node(
        names.entry.clone(),
        NodeKind::RegionEntry,
        AGENT_ENTER,
        boundary.enter.0,
        boundary.enter.1,
    ));
    nodes.push(boundary_node(
        names.exit.clone(),
        NodeKind::RegionExit,
        AGENT_LEAVE,
        boundary.leave.0,
        boundary.leave.1,
    ));
    nodes.push(decider_node(
        names.decider.clone(),
        decider_inputs,
        agent.termination_policy().accept_on_validator_pass(),
    )?);

    record_losses(opaque_ports, &fields, &mut losses);
    let region_budget = region_budget_of(agent);
    let spec = loop_spec(
        agent,
        &names,
        boundary.carried,
        boundary.handed_back,
        &mut losses,
    )?;
    let capabilities = grants_for(&nodes)?;
    let control = ordering(graph, &names)?;
    let members: BTreeSet<Identifier> = nodes.iter().map(|node| node.id.clone()).collect();

    let built = Graph::new(GraphBuilder {
        nodes,
        edges,
        control,
        regions: vec![Region {
            id: identifier("agent-loop")?,
            kind: RegionKind::Loop {
                spec: Box::new(spec),
            },
            parent: None,
            entry: names.entry,
            exit: names.exit,
            nodes: members,
            capabilities: capabilities.clone(),
            budget: region_budget,
        }],
        inputs: boundary.graph_inputs,
        outputs: boundary.graph_outputs,
        ..GraphBuilder::default()
    })
    .map_err(|error| AdapterError::Unsupported {
        construct: error.to_string(),
    })?;

    let definition = Definition {
        schema_version: Definition::current_schema_version(),
        packs: Vec::new(),
        id: identifier(agent.id().as_str())?,
        version: "1".to_string(),
        name: agent.name().to_string(),
        assurance: AssuranceMode::Observe,
        capabilities,
        budget: region_budget,
        graph: built,
        boundaries: vec![],
        contracts: vec![],
    };

    Ok(Adapted {
        definition,
        notes: losses,
    })
}

/// Says what the translation did not carry, so the loss travels with the result.
fn record_losses(opaque_ports: usize, fields: &StateFields, losses: &mut Vec<AdaptationNote>) {
    if opaque_ports > 0 {
        losses.push(AdaptationNote::new(
            "graph port type",
            format!("{opaque_ports} ports carry the `json` tag, which has no structure to carry"),
        ));
    }
    if !fields.read.is_empty() || !fields.written.is_empty() {
        losses.push(AdaptationNote::new(
            "graph state field endpoint",
            format!(
                "{} state fields read and {} written cross the loop's boundary as named values; \
                 state nothing declares is not carried",
                fields.read.len(),
                fields.written.len()
            ),
        ));
    }
    losses.push(AdaptationNote::new(
        "agent termination condition",
        "continuation is decided by a deterministic node that reads every validator's result, in \
         place of the placeholder a declared loop would otherwise need",
    ));
}

/// What the definition and its loop may spend, from the agent's budget.
fn region_budget_of(agent: &AgentDefinition) -> ResourceBudget {
    let budget = agent.budget();
    let effect_nodes = agent
        .graph()
        .nodes()
        .iter()
        .filter(|node| {
            matches!(
                node.kind(),
                SourceNodeKind::Job | SourceNodeKind::MemoryWrite
            )
        })
        .count();
    ResourceBudget {
        wall_ms: budget.max_seconds().saturating_mul(1_000),
        tokens: budget.max_tokens(),
        cost_micro_units: budget.max_cost_micros(),
        effect_count: u32::try_from(effect_nodes).unwrap_or(u32::MAX),
    }
}

/// The three nodes a translated agent gains.
struct LoopNames {
    entry: Identifier,
    exit: Identifier,
    decider: Identifier,
}

impl LoopNames {
    fn new() -> Result<Self, AdapterError> {
        Ok(Self {
            entry: identifier(AGENT_ENTER)?,
            exit: identifier(AGENT_LEAVE)?,
            decider: identifier(AGENT_CONTINUE)?,
        })
    }
}

/// Connects each validator's result to the node that reads it, and returns the
/// ports that node gains.
fn wire_validators(
    graph: &GraphDefinition,
    names: &LoopNames,
    edges: &mut Vec<Hyperedge>,
) -> Result<Vec<InputPort>, AdapterError> {
    let mut ports = Vec::new();
    for (node, port, name) in validator_outputs(graph)? {
        let check_port = identifier(&format!("{CHECK_PORT_PREFIX}{name}"))?;
        ports.push(InputPort::new(
            check_port.clone(),
            schema_for("validation_result")?,
        ));
        edges.push(Hyperedge {
            id: identifier(&format!("agent-reads-{name}"))?,
            sources: vec![Endpoint::Port { node, port }],
            targets: vec![Endpoint::Port {
                node: names.decider.clone(),
                port: check_port,
            }],
            combine: Combine::Forward,
            trust: TrustDerivation::Weakest,
        });
    }
    Ok(ports)
}

/// What crosses the loop's boundary.
struct Boundary {
    enter: (Vec<InputPort>, Vec<OutputPort>),
    leave: (Vec<InputPort>, Vec<OutputPort>),
    graph_inputs: Vec<OutputPort>,
    graph_outputs: Vec<InputPort>,
    /// Fields both read and written, which the next iteration starts from.
    carried: BTreeMap<String, ValueSchema>,
    handed_back: BTreeMap<String, ValueSchema>,
}

/// Connects state fields to the graph-level ports behind the boundary nodes.
fn wire_boundary(
    fields: &StateFields,
    names: &LoopNames,
    edges: &mut Vec<Hyperedge>,
    decider_inputs: &mut Vec<InputPort>,
) -> Result<Boundary, AdapterError> {
    let mut boundary = Boundary {
        enter: (Vec::new(), Vec::new()),
        leave: (Vec::new(), Vec::new()),
        graph_inputs: Vec::new(),
        graph_outputs: Vec::new(),
        carried: BTreeMap::new(),
        handed_back: BTreeMap::new(),
    };
    for (field, tag) in &fields.read {
        let id = identifier(field)?;
        let schema = schema_for(tag)?;
        boundary
            .enter
            .0
            .push(InputPort::new(id.clone(), schema.clone()));
        boundary
            .enter
            .1
            .push(OutputPort::new(id.clone(), schema.clone()));
        boundary
            .graph_inputs
            .push(OutputPort::new(id.clone(), schema));
        edges.push(Hyperedge {
            id: identifier(&format!("state-in-{field}"))?,
            sources: vec![Endpoint::GraphInput { name: id.clone() }],
            targets: vec![Endpoint::Port {
                node: names.entry.clone(),
                port: id,
            }],
            combine: Combine::Forward,
            trust: TrustDerivation::Weakest,
        });
    }
    for (field, tag) in &fields.written {
        let id = identifier(field)?;
        let schema = schema_for(tag)?;
        decider_inputs.push(InputPort::new(
            identifier(&format!("{STATE_PORT_PREFIX}{field}"))?,
            schema.clone(),
        ));
        boundary
            .leave
            .0
            .push(InputPort::new(id.clone(), schema.clone()));
        boundary
            .leave
            .1
            .push(OutputPort::new(id.clone(), schema.clone()));
        boundary
            .graph_outputs
            .push(InputPort::new(id.clone(), schema.clone()));
        edges.push(Hyperedge {
            id: identifier(&format!("state-out-{field}"))?,
            sources: vec![Endpoint::Port {
                node: names.exit.clone(),
                port: id.clone(),
            }],
            targets: vec![Endpoint::GraphOutput { name: id }],
            combine: Combine::Forward,
            trust: TrustDerivation::Weakest,
        });
        boundary.handed_back.insert(field.clone(), schema.clone());
        if fields.read.contains_key(field) {
            boundary.carried.insert(field.clone(), schema);
        }
    }
    Ok(boundary)
}

/// The deterministic node that says whether the loop goes round again.
fn decider_node(
    id: Identifier,
    inputs: Vec<InputPort>,
    retries: bool,
) -> Result<Node, AdapterError> {
    let mut parameters = BTreeMap::new();
    parameters.insert(KIND_PARAMETER.to_string(), AGENT_CONTINUE.to_string());
    parameters.insert(RETRY_PARAMETER.to_string(), retries.to_string());
    Ok(Node {
        id,
        name: "Continue?".to_string(),
        kind: NodeKind::PureComputation,
        inputs,
        outputs: vec![
            OutputPort::new(identifier(KEEP_GOING)?, ValueSchema::Bool),
            OutputPort::new(
                identifier(DISTINCT_RESULTS)?,
                ValueSchema::Integer {
                    range: IntegerRange::new(0, i128::from(u32::MAX)),
                },
            ),
            OutputPort::new(
                identifier(SEEN_RESULTS)?,
                aliases::opaque("the digests of the results this loop has already produced"),
            ),
        ],
        capabilities: vec![],
        effects: vec![],
        budget: ResourceBudget::deterministic(1_000),
        provider: None,
        sub_workflow: None,
        parameters,
    })
}

/// Enter first, leave last, and the decision just before leaving.
///
/// Without these a region's exit could run before its body, and the iteration would
/// close with the work undone.
fn ordering(graph: &GraphDefinition, names: &LoopNames) -> Result<Vec<ControlEdge>, AdapterError> {
    let mut control = Vec::with_capacity(graph.nodes().len() * 2 + 2);
    for node in graph.nodes() {
        let id = identifier(node.id().as_str())?;
        control.push(ControlEdge {
            from: names.entry.clone(),
            to: id.clone(),
        });
        control.push(ControlEdge {
            from: id,
            to: names.decider.clone(),
        });
    }
    control.push(ControlEdge {
        from: names.entry.clone(),
        to: names.decider.clone(),
    });
    control.push(ControlEdge {
        from: names.decider.clone(),
        to: names.exit.clone(),
    });
    Ok(control)
}

/// What the agent's budget and termination policy say about its loop.
fn loop_spec(
    agent: &AgentDefinition,
    names: &LoopNames,
    carried: BTreeMap<String, ValueSchema>,
    handed_back: BTreeMap<String, ValueSchema>,
    notes: &mut Vec<AdaptationNote>,
) -> Result<LoopSpec, AdapterError> {
    let budget = agent.budget();
    let progress = if agent.termination_policy().stop_on_no_progress() {
        Some(ProgressMeasure {
            id: identifier("results-still-changing")?,
            measured_by: names.decider.clone(),
            port: identifier(DISTINCT_RESULTS)?,
            direction: ProgressDirection::StrictlyIncreasing,
        })
    } else {
        None
    };
    Ok(LoopSpec {
        state: carried,
        exit: handed_back,
        continuation: Continuation {
            evaluated_by: names.decider.clone(),
            port: identifier(KEEP_GOING)?,
        },
        budget: LoopBudget {
            max_iterations: budget.max_steps(),
            wall_ms: budget.max_seconds().saturating_mul(1_000),
            tokens: budget.max_tokens(),
            cost_micro_units: budget.max_cost_micros(),
            // Unbounded here on purpose: reaching this bound stops a loop, and each
            // effect already happens at most once however many times the loop goes
            // round. The definition's budget is what admission checks them against.
            effect_count: 0,
        },
        invariants: vec![],
        progress,
        repairs: repairs_for(agent, notes)?,
    })
}

/// The state fields a graph reads and writes, with their port tags.
#[derive(Default)]
struct StateFields {
    read: BTreeMap<String, String>,
    written: BTreeMap<String, String>,
}

fn state_fields(graph: &GraphDefinition) -> StateFields {
    let mut fields = StateFields::default();
    for edge in graph.hyperedges() {
        for endpoint in edge.sources() {
            if let HyperedgeEndpoint::StateField { field, value_type } = endpoint {
                fields.read.insert(field.clone(), value_type.to_string());
            }
        }
        for endpoint in edge.targets() {
            if let HyperedgeEndpoint::StateField { field, value_type } = endpoint {
                fields.written.insert(field.clone(), value_type.to_string());
            }
        }
    }
    fields
}

/// Refuses a graph this translation could only run by pretending.
fn refuse_what_cannot_run(graph: &GraphDefinition) -> Result<(), AdapterError> {
    if matches!(
        graph.transition_policy().mode(),
        GraphTransitionMode::Planner { .. }
    ) {
        return Err(AdapterError::Unsupported {
            construct: "planner transitions, where a model chooses the next node, cannot run yet; \
                        running the graph in static order would look like it worked"
                .to_string(),
        });
    }
    if graph.has_cycles() {
        return Err(AdapterError::Unsupported {
            construct: "a cycle in an agent graph; an agent repeats by going round its loop, \
                        which is what its budget bounds"
                .to_string(),
        });
    }
    Ok(())
}

/// Every validator's result port, which is what the loop decides on.
fn validator_outputs(
    graph: &GraphDefinition,
) -> Result<Vec<(Identifier, Identifier, String)>, AdapterError> {
    let mut outputs = Vec::new();
    for node in graph.nodes() {
        if node.kind() != SourceNodeKind::Validator {
            continue;
        }
        let results: Vec<&capsulet_core::GraphPort> = node
            .ports()
            .iter()
            .filter(|port| {
                port.direction() == PortDirection::Output
                    && port.value_type() == capsulet_core::PortValueType::ValidationResult
            })
            .collect();
        let [result] = results.as_slice() else {
            return Err(AdapterError::Unsupported {
                construct: format!(
                    "validator `{}` must declare exactly one `validation_result` output, which is \
                     what the loop reads",
                    node.id()
                ),
            });
        };
        if !node.config().keys().any(|key| key.starts_with("rule.")) {
            return Err(AdapterError::Unsupported {
                construct: format!(
                    "validator `{}` declares no `rule.*` config, so it would check nothing",
                    node.id()
                ),
            });
        }
        outputs.push((
            identifier(node.id().as_str())?,
            identifier(result.id().as_str())?,
            node.id().as_str().to_string(),
        ));
    }
    Ok(outputs)
}

/// One edge of the graph, with state fields routed through the region's boundary.
fn agent_edge(
    edge: &GraphHyperedge,
    names: &LoopNames,
    fed: &mut BTreeSet<(String, String)>,
) -> Result<Hyperedge, AdapterError> {
    let mut sources = Vec::with_capacity(edge.sources().len());
    for endpoint in edge.sources() {
        sources.push(match endpoint {
            HyperedgeEndpoint::Port { node_id, port_id } => Endpoint::Port {
                node: identifier(node_id.as_str())?,
                port: identifier(port_id.as_str())?,
            },
            HyperedgeEndpoint::StateField { field, .. } => Endpoint::Port {
                node: names.entry.clone(),
                port: identifier(field)?,
            },
        });
    }

    let mut targets = Vec::with_capacity(edge.targets().len());
    for endpoint in edge.targets() {
        let (node, port) = match endpoint {
            HyperedgeEndpoint::Port { node_id, port_id } => {
                (node_id.as_str().to_string(), port_id.as_str().to_string())
            }
            HyperedgeEndpoint::StateField { field, .. } => (AGENT_LEAVE.to_string(), field.clone()),
        };
        if !fed.insert((node.clone(), port.clone())) {
            return Err(AdapterError::Unsupported {
                construct: format!(
                    "`{port}` on `{node}` is fed by more than one edge, so which value it runs on \
                     would depend on the order they arrived"
                ),
            });
        }
        match endpoint {
            HyperedgeEndpoint::Port { node_id, port_id } => targets.push(Endpoint::Port {
                node: identifier(node_id.as_str())?,
                port: identifier(port_id.as_str())?,
            }),
            HyperedgeEndpoint::StateField { field, .. } => {
                targets.push(Endpoint::Port {
                    node: names.exit.clone(),
                    port: identifier(field)?,
                });
                // The node that notices a repeated result has to see what was written.
                targets.push(Endpoint::Port {
                    node: names.decider.clone(),
                    port: identifier(&format!("{STATE_PORT_PREFIX}{field}"))?,
                });
            }
        }
    }

    let combine = if sources.len() == 1 {
        Combine::Forward
    } else {
        let mut fields = BTreeMap::new();
        for (index, source) in sources.iter().enumerate() {
            fields.insert(endpoint_name(source), index);
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

/// Refuses a typed input nothing feeds.
///
/// A `json` input may stay unconnected: it declares no structure, and a job taking no
/// data is an ordinary thing to run. A typed one is a node that cannot do what it says
/// it does, and finding that out at admission beats finding out half-way through a run.
fn refuse_unfed_inputs(
    graph: &GraphDefinition,
    fed: &BTreeSet<(String, String)>,
) -> Result<(), AdapterError> {
    for node in graph.nodes() {
        for port in node.ports() {
            if port.direction() == PortDirection::Input
                && port.value_type() != capsulet_core::PortValueType::Json
                && !fed.contains(&(
                    node.id().as_str().to_string(),
                    port.id().as_str().to_string(),
                ))
            {
                return Err(AdapterError::Unsupported {
                    construct: format!(
                        "input `{}` of node `{}` is not connected to anything; connect it to \
                         another node or to a state field",
                        port.id(),
                        node.id()
                    ),
                });
            }
        }
    }
    Ok(())
}

/// One node of the graph, carrying what it does and what it is allowed to reach.
fn agent_node(node: &GraphNode, opaque: &mut usize) -> Result<Node, AdapterError> {
    let (inputs, outputs) = ports_of(node, opaque)?;
    let id = identifier(node.id().as_str())?;
    let kind = node_kind(node.kind());

    let mut parameters = node.config().clone();
    parameters.insert(KIND_PARAMETER.to_string(), node.kind().to_string());

    let mut budget = budget_for(node.kind());
    if let Some(seconds) = node
        .config()
        .get("timeout_seconds")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
    {
        budget.wall_ms = seconds.saturating_mul(1_000);
    }

    let (capabilities, effects, provider) = match node.kind() {
        SourceNodeKind::Job => {
            if !node.config().contains_key("job_definition_id") {
                return Err(AdapterError::Unsupported {
                    construct: format!(
                        "job node `{}` declares no `job_definition_id` config, so it has nothing \
                         to run",
                        node.id()
                    ),
                });
            }
            (
                vec![identifier(JOB_EXECUTION)?],
                vec![Effect {
                    id: identifier("run-job")?,
                    kind: EffectKind::ExternalSideEffect,
                    target: node.name().to_string(),
                    capability: identifier(JOB_EXECUTION)?,
                    // The job run's id is derived from this key, so a retry after a
                    // crash finds the run it already created instead of starting
                    // another.
                    idempotency: Idempotency::Keyed {
                        key_source: "run_id+node_id".to_string(),
                    },
                    reversibility: Reversibility::Irreversible,
                }],
                None,
            )
        }
        SourceNodeKind::MemoryWrite => (
            vec![identifier(MEMORY_WRITE)?],
            vec![Effect {
                id: identifier("propose-claim")?,
                kind: EffectKind::MemoryWrite,
                target: "governed memory review queue".to_string(),
                capability: identifier(MEMORY_WRITE)?,
                idempotency: Idempotency::Keyed {
                    key_source: "run_id+node_id".to_string(),
                },
                reversibility: Reversibility::Irreversible,
            }],
            None,
        ),
        SourceNodeKind::MemoryRead => (vec![identifier(MEMORY_READ)?], vec![], None),
        SourceNodeKind::Llm | SourceNodeKind::Planner | SourceNodeKind::Embedding => {
            let Some(model) = node.config().get("model") else {
                return Err(AdapterError::Unsupported {
                    construct: format!(
                        "model node `{}` declares no `model` config, so the definition could not \
                         say what it ran",
                        node.id()
                    ),
                });
            };
            let capability = model_capability(node)?;
            (
                vec![capability.clone()],
                vec![],
                Some(ProviderBinding {
                    capability,
                    selection: model.clone(),
                }),
            )
        }
        _ => (vec![], vec![], None),
    };

    Ok(Node {
        id,
        name: node.name().to_string(),
        kind,
        inputs,
        outputs,
        capabilities,
        effects,
        budget,
        provider,
        sub_workflow: None,
        parameters,
    })
}

/// The capability a model node spends: one per provider, named after it.
fn model_capability(node: &GraphNode) -> Result<Identifier, AdapterError> {
    identifier(&format!("model:{}", model_provider(node)))
}

fn model_provider(node: &GraphNode) -> &str {
    node.config()
        .get("provider")
        .map_or("ollama", String::as_str)
}

/// Grants exactly what the translated nodes spend.
fn grants_for(nodes: &[Node]) -> Result<CapabilitySet, AdapterError> {
    let mut models: BTreeMap<Identifier, BTreeSet<String>> = BTreeMap::new();
    let mut grants = BTreeMap::new();
    for node in nodes {
        for capability in &node.capabilities {
            match capability.as_str() {
                JOB_EXECUTION => {
                    grants.insert(
                        capability.clone(),
                        Grant::Tool {
                            tool: identifier("capsulet/job-runner")?,
                        },
                    );
                }
                MEMORY_READ | MEMORY_WRITE => {
                    grants.insert(
                        capability.clone(),
                        Grant::MemorySpace {
                            space: identifier("project")?,
                            write: capability.as_str() == MEMORY_WRITE,
                        },
                    );
                }
                _ => {
                    if let Some(provider) = &node.provider {
                        models
                            .entry(capability.clone())
                            .or_default()
                            .insert(provider.selection.clone());
                    }
                }
            }
        }
    }
    for (capability, selected) in models {
        let provider = capability
            .as_str()
            .strip_prefix("model:")
            .unwrap_or(capability.as_str())
            .to_string();
        grants.insert(
            capability,
            Grant::ModelProvider {
                provider: identifier(&provider)?,
                models: selected.into_iter().collect(),
            },
        );
    }
    CapabilitySet::new(
        grants
            .into_iter()
            .map(|(id, grant)| Capability { id, grant })
            .collect(),
    )
    .map_err(|error| AdapterError::Unsupported {
        construct: error.to_string(),
    })
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
                to: identifier(HUMAN_REVIEWER)?,
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
            "this agent does not stop on a validator pass, so a failed check ends the run rather \
             than going round again",
        ));
    }
    Ok(repairs)
}

fn boundary_node(
    id: Identifier,
    kind: NodeKind,
    role: &str,
    inputs: Vec<InputPort>,
    outputs: Vec<OutputPort>,
) -> Node {
    let mut parameters = BTreeMap::new();
    parameters.insert(KIND_PARAMETER.to_string(), role.to_string());
    Node {
        name: role.to_string(),
        id,
        kind,
        inputs,
        outputs,
        capabilities: vec![],
        effects: vec![],
        budget: ResourceBudget::deterministic(1_000),
        provider: None,
        sub_workflow: None,
        parameters,
    }
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
            parameters: std::collections::BTreeMap::new(),
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
const MEMORY_READ: &str = "memory-read";
const MEMORY_WRITE: &str = "memory-write";

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
        SourceNodeKind::MemoryWrite => ResourceBudget {
            wall_ms: 120_000,
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
        packs: Vec::new(),
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
