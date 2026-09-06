//! The run's state, computed from its log.
//!
//! [`RunState::fold`] is the only way to build one. A worker starting fresh and
//! a worker recovering after a crash both call it over the same events and get
//! the same answer, which is what makes recovery ordinary rather than a special
//! path that only runs when something has already gone wrong.
//!
//! The fold is strict about what it accepts. A gap in positions, an event after
//! a terminal one, a node finishing that never started — each is a refusal, not
//! a shrug. A log that cannot be folded is a log nobody should be acting on.

use std::collections::{BTreeMap, BTreeSet};

use capsulet_ir::correctness::evidence::RecordedTime;
use capsulet_ir::definition::AssuranceMode;
use capsulet_ir::digest::Digest;
use capsulet_ir::id::Identifier;
use capsulet_ir::loop_region::{FailureKind, StopReason};
use thiserror::Error;

use crate::event::{ControlValue, Epoch, RecordedEvent, RunEvent, RunFailure, Wait};

/// Where a run is, as an execution concept. Never an assurance verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum RunStatus {
    /// Admitted, not yet started.
    #[default]
    Queued,
    Running,
    /// Durably suspended: a timer, an event, or a person.
    Waiting,
    Completed,
    Failed,
    Cancelled,
}

impl RunStatus {
    /// A short name for storage and messages.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// Whether nothing further will happen.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// An effect that was claimed and has not been resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutstandingEffect {
    pub node: Identifier,
    pub effect: Identifier,
    pub attempt: u32,
    pub key: Option<String>,
}

/// What a loop has done so far.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoopProgress {
    /// Iterations begun, which is what a budget is spent against — an
    /// iteration that started and crashed still consumed one.
    pub started: u32,
    pub finished: u32,
    /// The last progress reading, where the loop declares a measure.
    pub last_progress: Option<i128>,
    /// The reading before that. Kept because whether a measure moved the way it
    /// was declared to move needs two readings and the declaration, and the
    /// fold has no declaration — only [`crate::decide`] does.
    pub previous_progress: Option<i128>,
    /// How many consecutive iterations failed to move the measure at all.
    pub stalled: u32,
    /// When the open iteration began, where one is open. An iteration's wall
    /// time is the span its own events cover, and taking it from the log rather
    /// than from a timer the worker held is what lets a handover keep it.
    pub opened_at: Option<RecordedTime>,
    /// The invariant that did not hold in the most recent iteration.
    ///
    /// Cleared by an iteration in which every invariant held, so a loop that
    /// repaired itself is not stopped for a failure it has since fixed.
    pub failed_invariant: Option<Identifier>,
    pub stopped: Option<StopReason>,
}

/// A typed failure a node reported, and what it said about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeFailure {
    pub kind: FailureKind,
    pub detail: String,
}

/// What the run has consumed, summed from the log.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Spent {
    pub wall_ms: u64,
    pub tokens: u64,
    pub cost_micro_units: u64,
    pub effects: u32,
}

/// Why a log could not be folded.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FoldError {
    #[error("event at position {found} follows position {expected_after}; the log has a gap")]
    OutOfOrder { found: u64, expected_after: u64 },
    #[error("the log does not open with an admission event")]
    NotAdmitted,
    #[error("event `{event}` follows a terminal event; the run had already ended")]
    AfterTerminal { event: &'static str },
    #[error("node `{node}` finished without starting")]
    FinishedWithoutStarting { node: Identifier },
    #[error("node `{node}` started twice without finishing")]
    StartedTwice { node: Identifier },
    #[error("effect `{effect}` on `{node}` was finalized without being claimed")]
    FinalizedWithoutClaim {
        node: Identifier,
        effect: Identifier,
    },
    #[error("iteration {index} of `{region}` finished without starting")]
    IterationWithoutStart { region: Identifier, index: u32 },
    #[error("iteration {index} of `{region}` began while another was still open")]
    IterationAlreadyOpen { region: Identifier, index: u32 },
    #[error("resumed a wait the run was not suspended on")]
    ResumedWithoutWait,
}

/// Everything a decision needs to know about a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunState {
    status: RunStatus,
    definition: Digest,
    mode: AssuranceMode,
    epoch: Epoch,
    next_position: u64,
    /// Nodes in flight, and when each started. The start time is what a node
    /// deadline is measured from, and it has to survive a restart or a
    /// reattaching worker would give a stuck node a fresh timeout.
    running: BTreeMap<Identifier, RecordedTime>,
    finished: BTreeMap<Identifier, BTreeMap<String, Digest>>,
    outstanding: Vec<OutstandingEffect>,
    completed_effects: BTreeSet<(Identifier, Identifier)>,
    /// Finalized effects in the order they happened, so compensation can undo
    /// them in the reverse of that order.
    finalized_order: Vec<(Identifier, Identifier)>,
    compensated: BTreeSet<(Identifier, Identifier)>,
    /// Effects nobody could account for. Distinct from abandoned ones, which
    /// are known not to have happened and leave the run free to carry on.
    uncertain: Vec<(Identifier, Identifier)>,
    loops: BTreeMap<Identifier, LoopProgress>,
    failures: BTreeMap<Identifier, Vec<NodeFailure>>,
    /// The decision-relevant readings nodes have reported, by node and port.
    /// Reset with the node when an iteration reopens it, so a loop never reads
    /// last time round's answer.
    control: BTreeMap<(Identifier, String), ControlValue>,
    spent: Spent,
    waiting_on: Option<Wait>,
    failure: Option<RunFailure>,
    started_at: Option<RecordedTime>,
    cancel_requested: Option<Identifier>,
}

impl RunState {
    /// Rebuilds a run's state from its log.
    ///
    /// # Errors
    ///
    /// Returns [`FoldError`] when the log is not a history any run could have
    /// produced.
    pub fn fold(events: &[RecordedEvent]) -> Result<Self, FoldError> {
        let Some(first) = events.first() else {
            return Err(FoldError::NotAdmitted);
        };
        let RunEvent::Admitted { definition, mode } = &first.event else {
            return Err(FoldError::NotAdmitted);
        };

        let mut state = Self {
            status: RunStatus::Queued,
            definition: *definition,
            mode: *mode,
            epoch: first.epoch,
            next_position: first.position + 1,
            running: BTreeMap::new(),
            finished: BTreeMap::new(),
            outstanding: Vec::new(),
            completed_effects: BTreeSet::new(),
            finalized_order: Vec::new(),
            compensated: BTreeSet::new(),
            uncertain: Vec::new(),
            loops: BTreeMap::new(),
            failures: BTreeMap::new(),
            control: BTreeMap::new(),
            spent: Spent::default(),
            waiting_on: None,
            failure: None,
            started_at: None,
            cancel_requested: None,
        };

        if first.position != 0 {
            return Err(FoldError::OutOfOrder {
                found: first.position,
                expected_after: 0,
            });
        }

        for recorded in &events[1..] {
            state.apply(recorded)?;
        }
        Ok(state)
    }

    fn apply(&mut self, recorded: &RecordedEvent) -> Result<(), FoldError> {
        if recorded.position != self.next_position {
            return Err(FoldError::OutOfOrder {
                found: recorded.position,
                expected_after: self.next_position.saturating_sub(1),
            });
        }
        if self.status.is_terminal() {
            return Err(FoldError::AfterTerminal {
                event: recorded.event.as_str(),
            });
        }
        self.next_position = recorded.position + 1;
        self.epoch = self.epoch.max(recorded.epoch);

        match &recorded.event {
            RunEvent::Admitted { .. } => {
                // A second admission is not a history any run produced.
                return Err(FoldError::AfterTerminal {
                    event: recorded.event.as_str(),
                });
            }
            RunEvent::Started { .. } => {
                self.status = RunStatus::Running;
                self.started_at = Some(recorded.at);
            }
            RunEvent::NodeStarted { node } => {
                if self.running.insert(node.clone(), recorded.at).is_some() {
                    return Err(FoldError::StartedTwice { node: node.clone() });
                }
                self.status = RunStatus::Running;
            }
            RunEvent::NodeFinished {
                node,
                outputs,
                control,
            } => {
                if self.running.remove(node).is_none() {
                    return Err(FoldError::FinishedWithoutStarting { node: node.clone() });
                }
                self.finished.insert(node.clone(), outputs.clone());
                for (port, value) in control {
                    self.control.insert((node.clone(), port.clone()), *value);
                }
            }
            RunEvent::NodeFailed {
                node,
                failure,
                detail,
            } => {
                self.running.remove(node);
                // Kept, because how a node failed is what decides the route it
                // takes, and how often it has failed is what decides whether
                // that route is exhausted. Both have to survive a restart.
                self.failures
                    .entry(node.clone())
                    .or_default()
                    .push(NodeFailure {
                        kind: *failure,
                        detail: detail.clone(),
                    });
            }
            RunEvent::EffectClaimed { .. }
            | RunEvent::EffectFinalized { .. }
            | RunEvent::EffectAbandoned { .. }
            | RunEvent::EffectUncertain { .. } => self.apply_effect(&recorded.event)?,
            RunEvent::IterationStarted { .. } | RunEvent::IterationFinished { .. } => {
                self.apply_iteration(&recorded.event, recorded.at)?;
            }
            RunEvent::LoopStopped { region, reason } => {
                self.loops.entry(region.clone()).or_default().stopped = Some(reason.clone());
            }
            RunEvent::Suspended { wait } => {
                self.waiting_on = Some(wait.clone());
                self.status = RunStatus::Waiting;
            }
            RunEvent::Resumed { .. } => {
                if self.waiting_on.take().is_none() {
                    return Err(FoldError::ResumedWithoutWait);
                }
                self.status = RunStatus::Running;
            }
            RunEvent::CancellationRequested { by } => {
                self.cancel_requested = Some(by.clone());
            }
            RunEvent::Compensated { node, effect, .. } => {
                self.compensated.insert((node.clone(), effect.clone()));
            }
            RunEvent::Cancelled { .. } => self.status = RunStatus::Cancelled,
            RunEvent::Failed { reason } => {
                self.failure = Some(reason.clone());
                self.status = RunStatus::Failed;
            }
            RunEvent::Completed { .. } => self.status = RunStatus::Completed,
        }
        Ok(())
    }

    /// Effect claims, resolutions, and the ones nobody could resolve.
    fn apply_effect(&mut self, event: &RunEvent) -> Result<(), FoldError> {
        match event {
            RunEvent::EffectClaimed {
                node,
                effect,
                attempt,
                key,
            } => {
                self.outstanding.push(OutstandingEffect {
                    node: node.clone(),
                    effect: effect.clone(),
                    attempt: *attempt,
                    key: key.clone(),
                });
                self.spent.effects = self.spent.effects.saturating_add(1);
            }
            RunEvent::EffectFinalized { node, effect, .. } => {
                let before = self.outstanding.len();
                self.outstanding
                    .retain(|claim| !(&claim.node == node && &claim.effect == effect));
                if self.outstanding.len() == before {
                    return Err(FoldError::FinalizedWithoutClaim {
                        node: node.clone(),
                        effect: effect.clone(),
                    });
                }
                self.completed_effects
                    .insert((node.clone(), effect.clone()));
                self.finalized_order.push((node.clone(), effect.clone()));
            }
            RunEvent::EffectAbandoned { node, effect, .. } => {
                self.outstanding
                    .retain(|claim| !(&claim.node == node && &claim.effect == effect));
            }
            RunEvent::EffectUncertain { node, effect, .. } => {
                self.outstanding
                    .retain(|claim| !(&claim.node == node && &claim.effect == effect));
                self.uncertain.push((node.clone(), effect.clone()));
            }
            _ => {}
        }
        Ok(())
    }

    /// Loop iterations, and the budgets and progress they spend.
    fn apply_iteration(&mut self, event: &RunEvent, at: RecordedTime) -> Result<(), FoldError> {
        match event {
            RunEvent::IterationStarted {
                region,
                index,
                members,
            } => {
                let progress = self.loops.entry(region.clone()).or_default();
                // Two open iterations of one region is not a history any run
                // produces, and a fold that accepted it would be reconstructing
                // something that never happened.
                if progress.started > progress.finished {
                    return Err(FoldError::IterationAlreadyOpen {
                        region: region.clone(),
                        index: *index,
                    });
                }
                progress.started = progress.started.saturating_add(1);
                progress.opened_at = Some(at);

                // Opening an iteration resets the region's nodes so they can run
                // again, and clears the readings they reported last time round.
                // A loop that read a stale continuation would keep going on an
                // answer from an iteration that has already ended.
                for member in members {
                    self.finished.remove(member);
                    self.running.remove(member);
                    self.failures.remove(member);
                    self.control.retain(|(node, _), _| node != member);
                }
            }
            RunEvent::IterationFinished { region, record } => {
                let progress = self.loops.entry(region.clone()).or_default();
                if progress.finished >= progress.started {
                    return Err(FoldError::IterationWithoutStart {
                        region: region.clone(),
                        index: record.index,
                    });
                }
                progress.finished = progress.finished.saturating_add(1);

                // A measure that did not move is what non-progress means, and it
                // has to survive a restart, so it is counted here rather than
                // held by whoever happened to be running the loop.
                //
                // Whether it moved the *declared* way is a separate question,
                // answered where the declaration is: the fold has only events.
                if let Some(reading) = record.progress {
                    if progress.last_progress == Some(reading) {
                        progress.stalled = progress.stalled.saturating_add(1);
                    } else {
                        progress.stalled = 0;
                    }
                    progress.previous_progress = progress.last_progress;
                    progress.last_progress = Some(reading);
                }

                // An iteration in which everything held clears the last
                // failure, so a loop is never stopped for a problem it has
                // since repaired.
                progress.failed_invariant = record
                    .invariants
                    .iter()
                    .find(|outcome| !outcome.held)
                    .map(|outcome| outcome.invariant.clone());

                self.spent.wall_ms = self.spent.wall_ms.saturating_add(record.spent.wall_ms);
                self.spent.tokens = self.spent.tokens.saturating_add(record.spent.tokens);
                self.spent.cost_micro_units = self
                    .spent
                    .cost_micro_units
                    .saturating_add(record.spent.cost_micro_units);
            }
            _ => {}
        }
        Ok(())
    }

    /// Where the run is.
    #[must_use]
    pub const fn status(&self) -> RunStatus {
        self.status
    }

    /// The definition being executed, by digest.
    #[must_use]
    pub const fn definition(&self) -> &Digest {
        &self.definition
    }

    /// The assurance mode the run was admitted under.
    #[must_use]
    pub const fn mode(&self) -> AssuranceMode {
        self.mode
    }

    /// The highest epoch seen in the log.
    #[must_use]
    pub const fn epoch(&self) -> Epoch {
        self.epoch
    }

    /// The position the next event must take.
    #[must_use]
    pub const fn next_position(&self) -> u64 {
        self.next_position
    }

    /// Nodes that have finished.
    #[must_use]
    pub fn has_finished(&self, node: &Identifier) -> bool {
        self.finished.contains_key(node)
    }

    /// Nodes currently in flight.
    #[must_use]
    pub const fn running(&self) -> &BTreeMap<Identifier, RecordedTime> {
        &self.running
    }

    /// When the run itself started, if it has.
    #[must_use]
    pub const fn started_at(&self) -> Option<RecordedTime> {
        self.started_at
    }

    /// Who asked for the run to stop, if anybody has.
    #[must_use]
    pub const fn cancellation_requested(&self) -> Option<&Identifier> {
        self.cancel_requested.as_ref()
    }

    /// Effects known to have happened, oldest first.
    #[must_use]
    pub fn finalized_effects(&self) -> &[(Identifier, Identifier)] {
        &self.finalized_order
    }

    /// Effects nobody could account for, in the order the run gave up on them.
    ///
    /// A run with one of these cannot finish: somewhere out there an effect
    /// either happened or did not, and this system is not entitled to guess.
    #[must_use]
    pub fn uncertain_effects(&self) -> &[(Identifier, Identifier)] {
        &self.uncertain
    }

    /// Whether an effect that happened has since been undone.
    #[must_use]
    pub fn is_compensated(&self, node: &Identifier, effect: &Identifier) -> bool {
        self.compensated.contains(&(node.clone(), effect.clone()))
    }

    /// Effects claimed and not yet resolved, in claim order.
    ///
    /// After a crash this is the list of things nobody can currently say
    /// happened or not.
    #[must_use]
    pub fn outstanding_effects(&self) -> &[OutstandingEffect] {
        &self.outstanding
    }

    /// Whether an effect is known to have completed.
    #[must_use]
    pub fn effect_completed(&self, node: &Identifier, effect: &Identifier) -> bool {
        self.completed_effects
            .contains(&(node.clone(), effect.clone()))
    }

    /// What one finished node produced, by port.
    #[must_use]
    pub fn outputs_of(&self, node: &Identifier) -> BTreeMap<String, Digest> {
        self.finished.get(node).cloned().unwrap_or_default()
    }

    /// A reading a node reported for one of its ports.
    #[must_use]
    pub fn control_value(&self, node: &Identifier, port: &str) -> Option<ControlValue> {
        self.control.get(&(node.clone(), port.to_string())).copied()
    }

    /// Every output every finished node produced, keyed `node.port`.
    ///
    /// All of them rather than a selected few: which outputs are *the run's*
    /// results is a question about the graph, and this is a fold over the log.
    /// A caller holding the definition can narrow it; a caller that only has
    /// the log would otherwise have to guess.
    #[must_use]
    pub fn outputs(&self) -> BTreeMap<String, Digest> {
        self.finished
            .iter()
            .flat_map(|(node, outputs)| {
                outputs
                    .iter()
                    .map(move |(port, digest)| (format!("{node}.{port}"), *digest))
            })
            .collect()
    }

    /// A loop's progress.
    #[must_use]
    pub fn loop_progress(&self, region: &Identifier) -> LoopProgress {
        self.loops.get(region).cloned().unwrap_or_default()
    }

    /// What the run has consumed.
    #[must_use]
    pub const fn spent(&self) -> Spent {
        self.spent
    }

    /// What the run is suspended on, if anything.
    #[must_use]
    pub const fn waiting_on(&self) -> Option<&Wait> {
        self.waiting_on.as_ref()
    }

    /// Why the run failed, if it did.
    #[must_use]
    pub const fn failure(&self) -> Option<&RunFailure> {
        self.failure.as_ref()
    }

    /// Every failure a node has reported, oldest first.
    #[must_use]
    pub fn failures_of(&self, node: &Identifier) -> &[NodeFailure] {
        self.failures.get(node).map_or(&[], Vec::as_slice)
    }

    /// How many times a node reported this kind of failure.
    ///
    /// This is what a declared retry budget is counted against, and it comes
    /// from the log rather than a counter somebody kept, so a restart cannot
    /// hand a route its attempts back.
    #[must_use]
    pub fn failure_count(&self, node: &Identifier, kind: FailureKind) -> u32 {
        u32::try_from(
            self.failures_of(node)
                .iter()
                .filter(|failure| failure.kind == kind)
                .count(),
        )
        .unwrap_or(u32::MAX)
    }

    /// The most recent failure of a node that is not running and not finished.
    ///
    /// A node in that state is waiting for someone to decide what happens next,
    /// which is exactly what a repair route is for.
    #[must_use]
    pub fn unresolved_failure(&self, node: &Identifier) -> Option<&NodeFailure> {
        if self.running.contains_key(node) || self.finished.contains_key(node) {
            return None;
        }
        self.failures_of(node).last()
    }

    /// Every node whose most recent failure nobody has answered yet.
    #[must_use]
    pub fn unresolved_failures(&self) -> Vec<(&Identifier, &NodeFailure)> {
        self.failures
            .keys()
            .filter_map(|node| self.unresolved_failure(node).map(|failure| (node, failure)))
            .collect()
    }
}
