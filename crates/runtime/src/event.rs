//! What happened, in the order it happened.
//!
//! A run's history is these events and nothing else. Its status, its progress
//! through the graph, what a loop has spent, which effects are outstanding —
//! all of it is a fold over this log rather than a field somebody updated.
//!
//! That is not architectural taste. Recovery after a crash has to reconstruct
//! exactly what a running worker knew, and the only way to be sure it does is
//! for both to compute it the same way from the same source. A separate
//! "current state" column is a second answer that can disagree with the log,
//! and the disagreement always surfaces at the worst moment.
//!
//! Every event carries the fencing epoch it was written under, so a worker that
//! lost its lease and did not notice cannot append to a run it no longer owns.

use std::collections::{BTreeMap, BTreeSet};

use capsulet_ir::correctness::evidence::RecordedTime;
use capsulet_ir::definition::AssuranceMode;
use capsulet_ir::digest::Digest;
use capsulet_ir::id::Identifier;
use capsulet_ir::loop_region::{FailureKind, IterationRecord, StopReason};
use serde::{Deserialize, Serialize};

/// A lease generation.
///
/// Bumped every time a run is leased. A write naming an older epoch is refused,
/// which is what stops a paused worker from waking up and corrupting a run
/// somebody else has since taken over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Epoch(pub u64);

impl Epoch {
    /// The next generation.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

/// A value a decision has to be taken on.
///
/// Node outputs travel as digests, so a large one does not end up in the log.
/// That is right for a value the run carries and wrong for a value the run has
/// to *read*: nobody can evaluate a loop's continuation condition from a
/// digest. The IR already names exactly which ports are decision-relevant — a
/// continuation, an invariant, a progress measure — and those, and only those,
/// are recorded as values.
///
/// Both cases are exact. There is no floating point here for the same reason
/// there is none in the IR: a decision that depends on a value nobody can
/// reproduce bit-for-bit is a decision nobody can replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ControlValue {
    Bool { value: bool },
    Integer { value: i128 },
}

impl ControlValue {
    /// The boolean this is, if it is one.
    #[must_use]
    pub const fn as_bool(self) -> Option<bool> {
        match self {
            Self::Bool { value } => Some(value),
            Self::Integer { .. } => None,
        }
    }

    /// The integer this is, if it is one.
    #[must_use]
    pub const fn as_integer(self) -> Option<i128> {
        match self {
            Self::Integer { value } => Some(value),
            Self::Bool { .. } => None,
        }
    }
}

/// What a run is waiting for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Wait {
    /// Until a moment. Compared against a time the caller supplies, never one
    /// this crate reads.
    Timer { until: RecordedTime },
    /// Until a named external event arrives.
    Event { name: Identifier },
    /// Until a person decides.
    HumanGate {
        node: Identifier,
        obligation: Identifier,
    },
}

/// Why a run stopped short.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunFailure {
    /// A node reported a typed failure.
    Node {
        node: Identifier,
        failure: FailureKind,
        detail: String,
    },
    /// A non-idempotent effect was claimed and never finalized, so nobody can
    /// say whether it happened. The run stops rather than guessing.
    EffectUncertain {
        node: Identifier,
        effect: Identifier,
    },
    /// A budget ran out at the run level.
    BudgetExhausted { resource: String },
    /// A loop declared a continuation, an invariant, or a progress measure, and
    /// the node that was supposed to evaluate it reported nothing.
    ///
    /// The run stops rather than assuming a value. Assuming `true` would loop
    /// forever on a check that never ran; assuming `false` would report a loop
    /// as finished when nothing checked whether it was.
    ControlMissing {
        region: Identifier,
        node: Identifier,
        port: String,
        /// What kind of reading the declaration needed.
        expected: String,
    },
    /// A loop stopped for a reason that is not the loop finishing its work.
    ///
    /// Carried rather than flattened into a message, because "the repair budget
    /// ran out" and "the invariant stopped holding" are different facts about
    /// the run, and a certificate that renders both as `failed` has thrown away
    /// the part anybody would act on.
    LoopStopped {
        region: Identifier,
        reason: StopReason,
    },
}

/// One thing that happened.
///
/// Externally tagged — `{"node_started": {...}}` — rather than carrying the
/// kind as a field alongside the data. That is not a style choice. Serde reads
/// an internally-tagged enum by buffering the whole object first, and its
/// buffer cannot hold a 128-bit integer, which is exactly what a loop's
/// progress measure is. An internally-tagged `RunEvent` therefore writes an
/// iteration record it cannot read back, and a run with a progress measure
/// becomes unrecoverable the moment it restarts.
///
/// The variant key is [`RunEvent::as_str`], so `payload -> kind` is the
/// variant's own body — which is how the database triggers reach into it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunEvent {
    /// The run was admitted against a definition. Always first.
    Admitted {
        definition: Digest,
        mode: AssuranceMode,
    },
    Started {
        by: Identifier,
    },
    NodeStarted {
        node: Identifier,
    },
    NodeFinished {
        node: Identifier,
        /// Output values by port, content-addressed.
        outputs: BTreeMap<String, Digest>,
        /// The decision-relevant readings this node produced, by port. Only the
        /// ports a loop declares as its continuation, an invariant, or a
        /// progress measure belong here; everything else is an output.
        #[serde(default)]
        control: BTreeMap<String, ControlValue>,
    },
    NodeFailed {
        node: Identifier,
        failure: FailureKind,
        detail: String,
    },
    /// Recorded *before* an effect is attempted, so a crash leaves a trace.
    EffectClaimed {
        node: Identifier,
        effect: Identifier,
        attempt: u32,
        /// The idempotency key handed to the far side, where there is one.
        key: Option<String>,
    },
    /// Recorded after the effect is known to have happened.
    EffectFinalized {
        node: Identifier,
        effect: Identifier,
        attempt: u32,
        receipt: Digest,
    },
    /// Recorded when the far side refused outright: the effect definitely did
    /// not happen, and the run may carry on deciding what to do about that.
    ///
    /// Distinct from [`RunEvent::EffectUncertain`] on purpose. "It did not
    /// happen" and "nobody knows whether it happened" lead to different
    /// decisions, and collapsing them would make every refusal look like the
    /// one case that has to stop the run.
    EffectAbandoned {
        node: Identifier,
        effect: Identifier,
        attempt: u32,
        reason: String,
    },
    /// Recorded for a claim nobody can resolve. The run stops.
    EffectUncertain {
        node: Identifier,
        effect: Identifier,
        attempt: u32,
    },
    IterationStarted {
        region: Identifier,
        index: u32,
        /// The region's nodes, named here rather than looked up.
        ///
        /// Starting an iteration resets them so they can run again, and the
        /// fold has no definition to ask which nodes those are. Carrying them
        /// makes the log say what it did without a reader needing the
        /// definition to interpret it.
        #[serde(default)]
        members: BTreeSet<Identifier>,
    },
    IterationFinished {
        region: Identifier,
        record: Box<IterationRecord>,
    },
    LoopStopped {
        region: Identifier,
        reason: StopReason,
    },
    Suspended {
        wait: Wait,
    },
    Resumed {
        wait: Wait,
        by: Identifier,
    },
    /// Somebody asked for the run to stop.
    ///
    /// A request rather than the stop itself, because stopping in the middle of
    /// an effect is worse than not stopping: the run would end with something
    /// outstanding that nobody can account for. The decision core picks the
    /// next safe point.
    CancellationRequested {
        by: Identifier,
    },
    /// A reversible effect that happened was undone.
    Compensated {
        node: Identifier,
        effect: Identifier,
        /// The compensation route the IR declared for it.
        compensation: Identifier,
        receipt: Digest,
    },
    Cancelled {
        by: Identifier,
    },
    Failed {
        reason: RunFailure,
    },
    Completed {
        outputs: BTreeMap<String, Digest>,
    },
}

impl RunEvent {
    /// A short name, for logs and metrics.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Admitted { .. } => "admitted",
            Self::Started { .. } => "started",
            Self::NodeStarted { .. } => "node_started",
            Self::NodeFinished { .. } => "node_finished",
            Self::NodeFailed { .. } => "node_failed",
            Self::EffectClaimed { .. } => "effect_claimed",
            Self::EffectFinalized { .. } => "effect_finalized",
            Self::EffectAbandoned { .. } => "effect_abandoned",
            Self::EffectUncertain { .. } => "effect_uncertain",
            Self::IterationStarted { .. } => "iteration_started",
            Self::IterationFinished { .. } => "iteration_finished",
            Self::LoopStopped { .. } => "loop_stopped",
            Self::Suspended { .. } => "suspended",
            Self::Resumed { .. } => "resumed",
            Self::CancellationRequested { .. } => "cancellation_requested",
            Self::Compensated { .. } => "compensated",
            Self::Cancelled { .. } => "cancelled",
            Self::Failed { .. } => "failed",
            Self::Completed { .. } => "completed",
        }
    }

    /// Whether this event ends the run.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed { .. } | Self::Failed { .. } | Self::Cancelled { .. }
        )
    }
}

/// An event as it was stored: its place in the log and the lease that wrote it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedEvent {
    /// Position in this run's log, starting at zero and gapless.
    pub position: u64,
    pub epoch: Epoch,
    /// When it was recorded, as data. Nothing here reads a clock.
    pub at: RecordedTime,
    pub event: RunEvent,
}
