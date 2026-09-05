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

use std::collections::BTreeMap;

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
}

/// One thing that happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
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
    /// Recorded during recovery for a claim nobody can resolve.
    EffectUncertain {
        node: Identifier,
        effect: Identifier,
        attempt: u32,
    },
    IterationStarted {
        region: Identifier,
        index: u32,
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
            Self::EffectUncertain { .. } => "effect_uncertain",
            Self::IterationStarted { .. } => "iteration_started",
            Self::IterationFinished { .. } => "iteration_finished",
            Self::LoopStopped { .. } => "loop_stopped",
            Self::Suspended { .. } => "suspended",
            Self::Resumed { .. } => "resumed",
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
