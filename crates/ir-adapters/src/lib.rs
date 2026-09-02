//! Translating what Capsulet runs today into the IR.
//!
//! These adapters are additive. Nothing here is wired into an execution path:
//! the scheduler still runs workflows the way it always has, and the agent
//! runtime still runs graphs the way it always has. What changes is that both
//! can now be *described* in one representation, which is the prerequisite for
//! M3 running them from it.
//!
//! The interesting output is not the translation, it is the honesty about it.
//! Some constructs map exactly. Some map with something lost — a soft
//! dependency whose semantics the IR does not yet carry, an opaque JSON state
//! field that was never structured, a continuation the current runtime decides
//! outside the graph. Every one of those is recorded as an
//! [`AdaptationNote`] on the result and as a row in the published coverage
//! report, so loss has to be declared rather than discovered later by whoever
//! trusted the translation.

pub mod graph;
pub mod memory;
pub mod workflow;

use std::fmt::{self, Display};

use capsulet_ir::admission::AdmissionRefusal;
use capsulet_ir::definition::Definition;
use capsulet_ir::id::IdentifierError;

pub use graph::{from_agent, from_graph};
pub use memory::{MemoryEvidence, from_memory_claim, wrap_reasoning_verdict};
pub use workflow::from_workflow;

/// Why a source definition could not be translated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterError {
    /// A name in the source is not a legal IR identifier.
    Identifier {
        what: &'static str,
        source: IdentifierError,
    },
    /// The source uses a construct this adapter does not know how to express.
    ///
    /// Deliberately an error rather than a silent approximation: an
    /// approximation that nobody was told about is worse than a translation
    /// that refuses.
    Unsupported { construct: String },
    /// The translation produced something the IR itself refuses.
    NotAdmissible { refusal: AdmissionRefusal },
}

impl Display for AdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identifier { what, source } => write!(formatter, "{what}: {source}"),
            Self::Unsupported { construct } => {
                write!(formatter, "`{construct}` has no representation in IR v1")
            }
            Self::NotAdmissible { refusal } => {
                write!(formatter, "the translation is not admissible: {refusal}")
            }
        }
    }
}

impl std::error::Error for AdapterError {}

/// Something the translation could not carry across exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdaptationNote {
    /// The source construct, named the way its own model names it.
    pub construct: String,
    /// What was lost, and what stands in for it.
    pub detail: String,
}

impl AdaptationNote {
    fn new(construct: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            construct: construct.into(),
            detail: detail.into(),
        }
    }
}

/// A translated definition, plus everything the translation had to give up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adapted {
    pub definition: Definition,
    pub notes: Vec<AdaptationNote>,
}

impl Adapted {
    /// Whether anything was lost on the way in.
    #[must_use]
    pub fn is_lossless(&self) -> bool {
        self.notes.is_empty()
    }
}

/// How well a source construct survives translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CoverageLevel {
    /// Expressible in the IR with nothing lost.
    Full,
    /// Expressible, but something the source knew is not carried, and the
    /// result records what.
    WithLoss,
    /// Not expressible yet. Translation refuses rather than approximating.
    Unsupported,
}

impl CoverageLevel {
    /// The name used in the published report.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::WithLoss => "with recorded loss",
            Self::Unsupported => "unsupported",
        }
    }
}

/// One row of the coverage report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coverage {
    pub construct: &'static str,
    pub level: CoverageLevel,
    pub note: &'static str,
}

/// What these adapters can and cannot carry.
///
/// This is the source of the published report, and a test compares the two, so
/// a construct that starts losing information has to say so here first.
#[must_use]
pub fn coverage() -> Vec<Coverage> {
    vec![
        Coverage {
            construct: "workflow step",
            level: CoverageLevel::WithLoss,
            note: "Becomes an effect node. The job it runs is an external side effect whose \
                   inputs and outputs are not modelled, so its ports are opaque.",
        },
        Coverage {
            construct: "workflow step dependency (hard)",
            level: CoverageLevel::Full,
            note: "Becomes a control edge.",
        },
        Coverage {
            construct: "workflow step dependency (soft, always)",
            level: CoverageLevel::WithLoss,
            note: "Becomes a control edge. The IR has no concept yet for continuing past a failed \
                   predecessor, so the distinction is recorded rather than expressed.",
        },
        Coverage {
            construct: "workflow step timeout and deadline",
            level: CoverageLevel::Full,
            note: "Become node and definition wall-clock budgets.",
        },
        Coverage {
            construct: "graph node kind",
            level: CoverageLevel::Full,
            note: "Maps onto IR node kinds, with validators becoming verifiers and model, \
                   retrieval, and ranking nodes becoming proposers.",
        },
        Coverage {
            construct: "graph port type",
            level: CoverageLevel::WithLoss,
            note: "Maps through the published alias table. The `json` tag has no structure to \
                   carry, and its opacity is recorded on the value.",
        },
        Coverage {
            construct: "graph hyperedge",
            level: CoverageLevel::Full,
            note: "Becomes a hyperedge whose combination names every contributing source.",
        },
        Coverage {
            construct: "graph state field endpoint",
            level: CoverageLevel::WithLoss,
            note: "Becomes a graph-level input or output. Shared mutable agent state is not a \
                   value the IR models, so the sharing is not carried.",
        },
        Coverage {
            construct: "agent budget",
            level: CoverageLevel::Full,
            note: "Becomes the loop region's iteration, token, time, and cost bounds.",
        },
        Coverage {
            construct: "agent termination condition",
            level: CoverageLevel::WithLoss,
            note: "Becomes repair routes and stop reasons. The current runtime decides \
                   continuation outside the graph, so a placeholder condition node stands in for \
                   the port a loop needs.",
        },
        Coverage {
            construct: "governed memory claim",
            level: CoverageLevel::WithLoss,
            note: "Becomes evidence plus a grounding obligation. Confidence is carried as text \
                   because a float has no canonical encoding.",
        },
        Coverage {
            construct: "governed memory write",
            level: CoverageLevel::Full,
            note: "Becomes a protected boundary over a memory-write effect.",
        },
        Coverage {
            construct: "stored reasoning certificate",
            level: CoverageLevel::WithLoss,
            note: "Wraps into a platform certificate without re-deciding it. The original \
                   verdict is carried as a declared oracle, because the snapshot it was decided \
                   against is not stored with it.",
        },
        Coverage {
            construct: "workflow run and step run state",
            level: CoverageLevel::Unsupported,
            note: "Execution history belongs to the durable runtime, which is M3. Only \
                   definitions are translated here.",
        },
        Coverage {
            construct: "automation trigger",
            level: CoverageLevel::Unsupported,
            note: "Triggers bind a schedule or an event to a workflow. The IR describes the \
                   workflow; binding is control-plane concern, not M2.",
        },
    ]
}
