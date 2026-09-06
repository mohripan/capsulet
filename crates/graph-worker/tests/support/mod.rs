#![allow(
    dead_code,
    reason = "each test binary compiles this module and uses a different subset"
)]

//! A store, a scripted executor, and a clock the test moves.

use std::collections::{BTreeMap, BTreeSet};
use std::env::VarError;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

use async_trait::async_trait;
use capsulet_graph_worker::{
    Clock, CompensationRequest, EffectOutcome, EffectRequest, Executor, NodeOutcome, NodeRequest,
};
use capsulet_ir::admission::AdmissionRecord;
use capsulet_ir::capability::{Capability, CapabilitySet, Grant};
use capsulet_ir::correctness::evidence::RecordedTime;
use capsulet_ir::definition::{AssuranceMode, Definition};
use capsulet_ir::effect::{Effect, EffectKind, Idempotency, Reversibility};
use capsulet_ir::graph::{Combine, TrustDerivation};
use capsulet_ir::loop_region::{Continuation, IterationRecord, LoopBudget, LoopSpec};
use capsulet_ir::region::{Region, RegionKind};
use capsulet_ir::value::LengthBounds;
use capsulet_ir::{
    Endpoint, Graph, GraphBuilder, Hyperedge, Identifier, InputPort, Node, NodeKind, OutputPort,
    ResourceBudget, ValueSchema, admit,
};
use capsulet_postgres::PostgresStore;

static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(1);

/// A fixed moment the tests measure from.
pub const NOW: i64 = 1_772_000_000_000;

/// A name no other test binary can collide with.
///
/// The gates run several suites against one database, and a per-binary counter
/// alone produced `tenant_1` in two of them. A test that leases another suite's
/// run fails in a way that looks like a worker bug and is not one.
pub fn fixture_id(prefix: &str) -> String {
    let sequence = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
    let process = std::process::id();
    format!("{prefix}_gw{process}_{sequence}")
}

pub fn required_database_url() -> String {
    std::env::var("CAPSULET_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap_or_else(|_: VarError| {
            panic!(
                "CAPSULET_TEST_DATABASE_URL is required; run \
                 `cargo run -p capsulet-xtask --locked -- verify --gate postgres`"
            )
        })
}

pub fn id(value: &str) -> Identifier {
    Identifier::parse(value).expect("the fixture identifier is valid")
}

pub async fn store() -> PostgresStore {
    let store = PostgresStore::connect(&required_database_url())
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");
    store
}

fn text() -> ValueSchema {
    ValueSchema::Text {
        length: LengthBounds::new(0, 1_024),
    }
}

fn definition_with(name: &str, graph: Graph, capabilities: CapabilitySet) -> Definition {
    Definition {
        schema_version: Definition::current_schema_version(),
        id: id(name),
        version: "1".to_string(),
        name: name.to_string(),
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

fn pure(name: &str, inputs: Vec<InputPort>, outputs: Vec<OutputPort>) -> Node {
    Node {
        id: id(name),
        name: name.to_string(),
        kind: NodeKind::PureComputation,
        inputs,
        outputs,
        capabilities: vec![],
        effects: vec![],
        budget: ResourceBudget::deterministic(1_000),
        provider: None,
        sub_workflow: None,
    }
}

/// `normalize -> summarize`.
pub fn pipeline_definition(name: &str) -> Definition {
    let graph = Graph::new(GraphBuilder {
        nodes: vec![
            pure(
                "normalize",
                vec![],
                vec![OutputPort::new(id("clean"), text())],
            ),
            pure(
                "summarize",
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

    definition_with(name, graph, CapabilitySet::empty())
}

/// One node whose whole purpose is a declared effect.
pub fn effect_definition(name: &str, idempotency: Idempotency) -> Definition {
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
        name,
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

/// An effect node whose effect declares how to undo itself.
pub fn reversible_effect_definition(name: &str) -> Definition {
    let mut definition = effect_definition(name, Idempotency::Idempotent);
    definition.graph = {
        let mut publish = definition
            .graph
            .node(&id("publish"))
            .expect("the fixture has the node")
            .clone();
        publish.effects[0].reversibility = Reversibility::Reversible {
            compensation: id("close-pull-request"),
        };
        Graph::new(GraphBuilder {
            nodes: vec![publish],
            ..GraphBuilder::default()
        })
        .expect("the fixture identifiers are distinct")
    };
    definition
}

/// A workflow that is nothing but a bounded loop: `enter -> check -> leave`.
///
/// `check` is the node the loop asks whether to go round again.
pub fn loop_definition(name: &str, max_iterations: u32) -> Definition {
    let member = |id_of: &str, kind: NodeKind, outputs: Vec<OutputPort>| Node {
        id: id(id_of),
        name: id_of.to_string(),
        kind,
        inputs: vec![InputPort::new(id("in"), text())],
        outputs,
        capabilities: vec![],
        effects: vec![],
        budget: ResourceBudget::deterministic(60_000),
        provider: None,
        sub_workflow: None,
    };

    let edge = |id_of: &str, from: &str, from_port: &str, to: &str| Hyperedge {
        id: id(id_of),
        sources: vec![Endpoint::Port {
            node: id(from),
            port: id(from_port),
        }],
        targets: vec![Endpoint::Port {
            node: id(to),
            port: id("in"),
        }],
        combine: Combine::Forward,
        trust: TrustDerivation::Weakest,
    };

    let mut parts = GraphBuilder {
        nodes: vec![
            member(
                "enter",
                NodeKind::RegionEntry,
                vec![OutputPort::new(id("out"), text())],
            ),
            member(
                "check",
                NodeKind::Verifier,
                vec![
                    OutputPort::new(id("keep-going"), ValueSchema::Bool),
                    OutputPort::new(id("out"), text()),
                ],
            ),
            member(
                "leave",
                NodeKind::RegionExit,
                vec![OutputPort::new(id("out"), text())],
            ),
        ],
        edges: vec![
            edge("enter-to-check", "enter", "out", "check"),
            edge("check-to-leave", "check", "out", "leave"),
        ],
        ..GraphBuilder::default()
    };

    let mut members = BTreeSet::new();
    for each in ["enter", "check", "leave"] {
        members.insert(id(each));
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
                    max_iterations,
                    wall_ms: 300_000,
                    tokens: 50_000,
                    cost_micro_units: 50_000,
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
            wall_ms: 300_000,
            tokens: 50_000,
            cost_micro_units: 50_000,
            effect_count: 0,
        },
    });

    let graph = Graph::new(parts).expect("the fixture identifiers are distinct");
    definition_with(name, graph, CapabilitySet::empty())
}

/// The same loop, with a node after it that depends on the region's exit.
///
/// The node exists to prove it does not run until the loop has stopped: acting
/// on one iteration's output would be acting on a value the loop was still
/// working on.
pub fn loop_definition_with_successor(name: &str, max_iterations: u32) -> Definition {
    let looping = loop_definition(name, max_iterations);
    let mut parts = GraphBuilder {
        nodes: looping.graph.nodes().cloned().collect(),
        edges: looping.graph.edges().cloned().collect(),
        regions: looping.graph.regions().cloned().collect(),
        ..GraphBuilder::default()
    };
    parts.nodes.push(Node {
        id: id("summarise"),
        name: "summarise".to_string(),
        kind: NodeKind::PureComputation,
        inputs: vec![InputPort::new(id("in"), text())],
        outputs: vec![],
        capabilities: vec![],
        effects: vec![],
        budget: ResourceBudget::deterministic(60_000),
        provider: None,
        sub_workflow: None,
    });
    parts.edges.push(Hyperedge {
        id: id("leave-to-summarise"),
        sources: vec![Endpoint::Port {
            node: id("leave"),
            port: id("out"),
        }],
        targets: vec![Endpoint::Port {
            node: id("summarise"),
            port: id("in"),
        }],
        combine: Combine::Forward,
        trust: TrustDerivation::Weakest,
    });

    let graph = Graph::new(parts).expect("the fixture identifiers are distinct");
    definition_with(name, graph, CapabilitySet::empty())
}

/// A workflow with a bounded loop and a protected effect.
///
/// The two things a crash can damage in different ways: a loop can be handed
/// its budget back, and an effect can happen twice.
pub fn chaos_definition(name: &str, idempotency: Idempotency) -> Definition {
    let graph = Graph::new(chaos_graph(idempotency)).expect("the fixture identifiers are distinct");
    let mut definition = definition_with(
        name,
        graph,
        CapabilitySet::new(vec![Capability {
            id: id("github"),
            grant: Grant::Network {
                hosts: vec!["api.github.com".to_string()],
            },
        }])
        .expect("the fixture grants are distinct"),
    );
    definition.budget = ResourceBudget {
        wall_ms: 600_000,
        tokens: 100_000,
        cost_micro_units: 100_000,
        effect_count: 4,
    };
    definition
}

/// The effect node the chaos fixture publishes with.
fn publish_node(idempotency: Idempotency) -> Node {
    Node {
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
    }
}

/// The nodes and the loop region the chaos fixture executes.
fn chaos_graph(idempotency: Idempotency) -> GraphBuilder {
    let publish = publish_node(idempotency);
    let region_node = |name: &str, kind: NodeKind, outputs: Vec<OutputPort>| Node {
        id: id(name),
        name: name.to_string(),
        kind,
        inputs: vec![InputPort::new(id("in"), text())],
        outputs,
        capabilities: vec![],
        effects: vec![],
        budget: ResourceBudget::deterministic(60_000),
        provider: None,
        sub_workflow: None,
    };

    let edge = |id_of: &str, from: &str, from_port: &str, to: &str| Hyperedge {
        id: id(id_of),
        sources: vec![Endpoint::Port {
            node: id(from),
            port: id(from_port),
        }],
        targets: vec![Endpoint::Port {
            node: id(to),
            port: id("in"),
        }],
        combine: Combine::Forward,
        trust: TrustDerivation::Weakest,
    };

    let mut parts = GraphBuilder {
        nodes: vec![
            region_node(
                "enter",
                NodeKind::RegionEntry,
                vec![OutputPort::new(id("out"), text())],
            ),
            region_node(
                "check",
                NodeKind::Verifier,
                vec![
                    OutputPort::new(id("keep-going"), ValueSchema::Bool),
                    OutputPort::new(id("out"), text()),
                ],
            ),
            region_node(
                "leave",
                NodeKind::RegionExit,
                vec![OutputPort::new(id("out"), text())],
            ),
            publish,
        ],
        edges: vec![
            edge("enter-to-check", "enter", "out", "check"),
            edge("check-to-leave", "check", "out", "leave"),
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
                    wall_ms: 300_000,
                    tokens: 50_000,
                    cost_micro_units: 50_000,
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
            wall_ms: 300_000,
            tokens: 50_000,
            cost_micro_units: 50_000,
            effect_count: 0,
        },
    });

    parts
}

/// An iteration that spent a little and read the given progress measure.
pub fn iteration(index: u32, progress: Option<i128>) -> IterationRecord {
    IterationRecord {
        index,
        state_in: capsulet_ir::Digest::of(b"before"),
        state_out: capsulet_ir::Digest::of(b"after"),
        invariants: vec![],
        progress,
        spent: LoopBudget {
            max_iterations: 1,
            wall_ms: 10,
            tokens: 0,
            cost_micro_units: 0,
            effect_count: 0,
        },
    }
}

pub fn admission(definition: &Definition) -> AdmissionRecord {
    admit(definition).expect("the fixture definition is admitted")
}

/// Registers a definition and creates a run of it.
pub async fn seeded_run(
    store: &PostgresStore,
    definition: &Definition,
) -> (String, String, String) {
    let tenant = fixture_id("tenant");
    let project = fixture_id("project");
    let digest = store
        .insert_ir_definition_version(&tenant, &project, definition, &admission(definition))
        .await
        .expect("register the definition");
    let run_id = fixture_id("run");
    store
        .create_ir_run(
            &tenant,
            &project,
            &run_id,
            &digest,
            AssuranceMode::Enforce,
            RecordedTime(NOW),
        )
        .await
        .expect("create the run");
    (tenant, project, run_id)
}

/// A clock the test moves by hand.
#[derive(Debug, Default)]
pub struct TestClock {
    millis: AtomicI64,
}

impl TestClock {
    #[must_use]
    pub fn at(millis: i64) -> Self {
        Self {
            millis: AtomicI64::new(millis),
        }
    }

    pub fn advance_to(&self, millis: i64) {
        self.millis.store(millis, Ordering::Relaxed);
    }
}

impl Clock for TestClock {
    fn now(&self) -> RecordedTime {
        RecordedTime(self.millis.load(Ordering::Relaxed))
    }
}

/// An executor that does what the test tells it to, and records what it was
/// asked for.
#[derive(Debug, Default)]
pub struct ScriptedExecutor {
    node: Mutex<BTreeMap<String, NodeOutcome>>,
    node_sequences: Mutex<BTreeMap<String, Vec<NodeOutcome>>>,
    effect: Mutex<Vec<EffectOutcome>>,
    compensation: Mutex<Vec<EffectOutcome>>,
    steal: Mutex<Option<(PostgresStore, String)>>,
    pub node_calls: Mutex<Vec<String>>,
    pub effect_calls: Mutex<Vec<(String, u32, Option<String>)>>,
    pub compensation_calls: Mutex<Vec<(String, String)>>,
}

impl ScriptedExecutor {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// What a node should do. Anything unscripted finishes with no outputs.
    pub fn node(&self, name: &str, outcome: NodeOutcome) {
        self.node
            .lock()
            .expect("the scripted outcomes are not poisoned")
            .insert(name.to_string(), outcome);
    }

    /// What a node should do on each successive run, in order.
    ///
    /// A node inside a loop runs once per iteration, and a loop is only
    /// interesting when the answers change.
    pub fn node_sequence(&self, name: &str, outcomes: Vec<NodeOutcome>) {
        self.node_sequences
            .lock()
            .expect("the scripted outcomes are not poisoned")
            .insert(name.to_string(), outcomes);
    }

    /// Take the run over while the worker is inside the executor.
    ///
    /// This is the only moment a real handover is interesting: between the
    /// worker deciding and the worker writing down what it did.
    pub fn steal_lease_during_work(&self, store: &PostgresStore, run_id: &str) {
        *self.steal.lock().expect("the record is not poisoned") =
            Some((store.clone(), run_id.to_string()));
    }

    async fn steal_if_asked(&self) {
        let stolen = self
            .steal
            .lock()
            .expect("the record is not poisoned")
            .take();
        if let Some((store, run_id)) = stolen {
            sqlx::query(
                "UPDATE ir_runs SET epoch = epoch + 1, lease_owner = 'thief' WHERE id = $1",
            )
            .bind(&run_id)
            .execute(store.pool())
            .await
            .expect("take the run over");
        }
    }

    /// What successive effect attempts should do, in order.
    pub fn effects(&self, outcomes: Vec<EffectOutcome>) {
        *self
            .effect
            .lock()
            .expect("the scripted outcomes are not poisoned") = outcomes;
    }

    /// What successive compensations should do, in order.
    pub fn compensations(&self, outcomes: Vec<EffectOutcome>) {
        *self
            .compensation
            .lock()
            .expect("the scripted outcomes are not poisoned") = outcomes;
    }

    #[must_use]
    pub fn compensations_performed(&self) -> Vec<(String, String)> {
        self.compensation_calls
            .lock()
            .expect("the record is not poisoned")
            .clone()
    }

    #[must_use]
    pub fn nodes_run(&self) -> Vec<String> {
        self.node_calls
            .lock()
            .expect("the record is not poisoned")
            .clone()
    }

    #[must_use]
    pub fn effects_performed(&self) -> Vec<(String, u32, Option<String>)> {
        self.effect_calls
            .lock()
            .expect("the record is not poisoned")
            .clone()
    }
}

#[async_trait]
impl Executor for ScriptedExecutor {
    async fn run_node(&self, request: NodeRequest<'_>) -> NodeOutcome {
        let name = request.node.id.as_str().to_string();
        self.node_calls
            .lock()
            .expect("the record is not poisoned")
            .push(name.clone());
        self.steal_if_asked().await;
        if let Some(outcome) = self
            .node_sequences
            .lock()
            .expect("the scripted outcomes are not poisoned")
            .get_mut(&name)
            .filter(|remaining| !remaining.is_empty())
            .map(|remaining| remaining.remove(0))
        {
            return outcome;
        }
        self.node
            .lock()
            .expect("the scripted outcomes are not poisoned")
            .get(&name)
            .cloned()
            .unwrap_or_else(|| NodeOutcome::finished(BTreeMap::new()))
    }

    async fn perform_effect(&self, request: EffectRequest<'_>) -> EffectOutcome {
        self.effect_calls
            .lock()
            .expect("the record is not poisoned")
            .push((
                request.effect.id.as_str().to_string(),
                request.attempt,
                request.key.map(str::to_string),
            ));
        let mut scripted = self
            .effect
            .lock()
            .expect("the scripted outcomes are not poisoned");
        if scripted.is_empty() {
            EffectOutcome::Performed {
                receipt: capsulet_ir::Digest::of(b"performed"),
            }
        } else {
            scripted.remove(0)
        }
    }

    async fn compensate(&self, request: CompensationRequest<'_>) -> EffectOutcome {
        self.compensation_calls
            .lock()
            .expect("the record is not poisoned")
            .push((
                request.effect.id.as_str().to_string(),
                request.route.as_str().to_string(),
            ));
        let mut scripted = self
            .compensation
            .lock()
            .expect("the scripted outcomes are not poisoned");
        if scripted.is_empty() {
            EffectOutcome::Performed {
                receipt: capsulet_ir::Digest::of(b"compensated"),
            }
        } else {
            scripted.remove(0)
        }
    }
}
