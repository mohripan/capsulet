//! Deciding a run again, at every decision its log records.
//!
//! [`decide`] is a pure function of the definition, the state a log folds to,
//! and a time. So every choice a worker made can be made again from the log
//! alone: fold the log up to the event, decide at the time the event was
//! recorded, and the decision must be the one the event records. A log that
//! folds is one the runtime *accepts*. A log that also replays is one the
//! runtime would have *written*.
//!
//! Not every event is a decision:
//!
//! - **The outside world writes some.** These are the admission, the run's
//!   inputs, and the answers to signals: a cancellation asked for, a gate's
//!   authority refusing, a wait resumed by something that arrived. The log
//!   records the answer, but not the signal it answered, so a replay counts
//!   these and does not judge them.
//! - **A decision's own work writes the rest.** After a node starts come what
//!   it consumed and how it ended. After an effect node starts come its claim,
//!   how the effect came out, and how the node ended. A replay checks that
//!   these follow the decision they belong to, in the order the worker writes
//!   them, but not what they contain: that is the node's work, which a replay
//!   cannot do again.
//!
//! Everything else is a decision, and must be the one `decide` makes. A worker
//! that died mid-node leaves work open. The next event then does not continue
//! it, so it is decided again rather than assumed, and a recovered run replays
//! like any other.

use capsulet_ir::definition::Definition;
use capsulet_ir::id::Identifier;

use crate::decide::{Decision, decide};
use crate::event::{RecordedEvent, RunEvent, RunFailure};
use crate::failure;
use crate::state::RunState;

/// What deciding a run again found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DecisionReplay {
    /// Events recording a decision that `decide` makes again from the log
    /// before them.
    pub replayed: u32,
    /// Events the outside world wrote: admission, inputs, answered signals.
    pub external: u32,
    /// Events a decision's own work wrote after it.
    pub consequences: u32,
    /// Where the log records something `decide` would not have done.
    pub divergences: Vec<Divergence>,
    /// What each event in the log is, in order: one per event.
    pub roles: Vec<Role>,
}

/// What the replay made of one event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// A decision `decide` makes again from the log before it.
    Decision,
    /// Written by the outside world, which the replay cannot judge.
    External,
    /// Written by a decision's own work, after the decision.
    Consequence,
    /// Something `decide` would not have written.
    Divergent,
}

impl Role {
    /// The role's name, for a report or a wire format.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Decision => "decision",
            Self::External => "external",
            Self::Consequence => "consequence",
            Self::Divergent => "divergent",
        }
    }
}

impl DecisionReplay {
    /// Whether every decision in the log is the one `decide` makes again.
    #[must_use]
    pub fn agrees(&self) -> bool {
        self.divergences.is_empty()
    }
}

/// One event `decide` would not have written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    pub position: u64,
    /// The event the log records there.
    pub recorded: String,
    /// What deciding again from the log before it produces instead.
    pub decided: String,
}

/// Work a decision opened, which its next events carry on.
#[derive(Debug, Clone)]
enum Open {
    /// A node running its work.
    Node { node: Identifier },
    /// An effect node: started, then claimed, then resolved, then ended.
    Effect {
        node: Identifier,
        effect: Identifier,
        attempt: u32,
        stage: Stage,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// The node is open; the claim comes next.
    Started,
    /// Claimed; how the effect came out comes next.
    Claimed,
    /// Resolved; the node's end comes next.
    Resolved,
}

/// Whether an event is what carrying out a decision writes first.
enum Matched {
    /// It is not.
    Not,
    /// It is, and the decision's work goes on in the events after it.
    Opens(Open),
    /// It is, and the decision is done.
    Done,
}

/// How an event relates to the work that is open.
enum Follow {
    /// It carries the work on.
    Continues(Open),
    /// It ends the work.
    Closes,
    /// It is not part of the work.
    No,
}

/// Decides `events` again, at each decision they record, against `definition`.
///
/// Needs nothing but the definition and the log: no clock, no store, no model.
#[must_use]
pub fn replay_decisions(definition: &Definition, events: &[RecordedEvent]) -> DecisionReplay {
    let mut replay = DecisionReplay::default();
    let mut open: Option<Open> = None;
    for (index, recorded) in events.iter().enumerate() {
        let event = &recorded.event;
        if is_external(event) {
            replay.external += 1;
            replay.roles.push(Role::External);
            continue;
        }
        if let Some(work) = open.take() {
            match follows(&work, event) {
                Follow::Continues(next) => {
                    replay.consequences += 1;
                    replay.roles.push(Role::Consequence);
                    open = Some(next);
                    continue;
                }
                Follow::Closes => {
                    replay.consequences += 1;
                    replay.roles.push(Role::Consequence);
                    continue;
                }
                Follow::No => {}
            }
        }
        let state = match RunState::fold(&events[..index]) {
            Ok(state) => state,
            Err(error) => {
                replay.divergences.push(Divergence {
                    position: recorded.position,
                    recorded: describe(event),
                    decided: format!("nothing: the log before it does not fold ({error})"),
                });
                replay.roles.push(Role::Divergent);
                continue;
            }
        };
        let decided = decide(definition, &state, recorded.at);
        let matched = records(&decided, event, &state);
        if let Matched::Opens(work) = matched {
            replay.replayed += 1;
            replay.roles.push(Role::Decision);
            open = Some(work);
        } else if matches!(matched, Matched::Done) {
            replay.replayed += 1;
            replay.roles.push(Role::Decision);
        } else if compensates(definition, &state, event) {
            // A compensation's event is stamped when the far side answered, a
            // little after the decision. Deciding at that later time can see a
            // deadline pass that had not when the worker decided, so the
            // compensation is checked against what the run owed instead.
            replay.replayed += 1;
            replay.roles.push(Role::Decision);
        } else if matches!(event, RunEvent::Resumed { .. }) {
            // Not a wait coming due: a signal that arrived and was answered,
            // which the log does not carry.
            replay.external += 1;
            replay.roles.push(Role::External);
        } else {
            replay.divergences.push(Divergence {
                position: recorded.position,
                recorded: describe(event),
                decided: describe_decision(&decided),
            });
            replay.roles.push(Role::Divergent);
        }
    }
    replay
}

/// Written by the outside world rather than by a decision.
fn is_external(event: &RunEvent) -> bool {
    matches!(
        event,
        RunEvent::Admitted { .. }
            | RunEvent::InputsBound { .. }
            | RunEvent::CancellationRequested { .. }
            | RunEvent::GateRejected { .. }
    )
}

/// Whether `event` carries on `work`, in the order the worker writes it.
fn follows(work: &Open, event: &RunEvent) -> Follow {
    match (work, event) {
        (Open::Node { node }, RunEvent::ResourcesConsumed { node: of, .. }) if of == node => {
            Follow::Continues(work.clone())
        }
        (
            Open::Node { node }
            | Open::Effect {
                node,
                stage: Stage::Resolved,
                ..
            },
            RunEvent::NodeFinished { node: of, .. } | RunEvent::NodeFailed { node: of, .. },
        ) if of == node => Follow::Closes,
        (
            Open::Effect {
                node,
                effect,
                attempt,
                stage: Stage::Started,
            },
            RunEvent::EffectClaimed {
                node: of,
                effect: claimed,
                attempt: tried,
                ..
            },
        ) if of == node && claimed == effect && tried == attempt => {
            Follow::Continues(Open::Effect {
                node: node.clone(),
                effect: effect.clone(),
                attempt: *attempt,
                stage: Stage::Claimed,
            })
        }
        (
            Open::Effect {
                node,
                effect,
                attempt,
                stage: Stage::Claimed,
            },
            RunEvent::EffectFinalized {
                node: of,
                effect: resolved,
                attempt: tried,
                ..
            }
            | RunEvent::EffectAbandoned {
                node: of,
                effect: resolved,
                attempt: tried,
                ..
            },
        ) if of == node && resolved == effect && tried == attempt => {
            Follow::Continues(Open::Effect {
                node: node.clone(),
                effect: effect.clone(),
                attempt: *attempt,
                stage: Stage::Resolved,
            })
        }
        _ => Follow::No,
    }
}

/// Whether `event` is what carrying out `decision` writes first, and what work
/// it leaves open.
#[expect(
    clippy::too_many_lines,
    reason = "one arm per decision keeps the worker's mapping readable in one place"
)]
fn records(decision: &Decision, event: &RunEvent, state: &RunState) -> Matched {
    match (decision, event) {
        (Decision::Start, RunEvent::Started { .. }) => Matched::Done,
        (Decision::Complete, RunEvent::Completed { outputs }) if *outputs == state.outputs() => {
            Matched::Done
        }
        (Decision::StartNode { node }, RunEvent::NodeStarted { node: started })
            if started == node =>
        {
            Matched::Opens(Open::Node { node: node.clone() })
        }
        // An effect node opens with the node, unless a recovered attempt finds
        // it already open, in which case the claim comes first.
        (
            Decision::PerformEffect {
                node,
                effect,
                attempt,
            },
            RunEvent::NodeStarted { node: started },
        ) if started == node && !state.running().contains_key(node) => {
            Matched::Opens(Open::Effect {
                node: node.clone(),
                effect: effect.clone(),
                attempt: *attempt,
                stage: Stage::Started,
            })
        }
        (
            Decision::PerformEffect {
                node,
                effect,
                attempt,
            }
            | Decision::RetryClaimedEffect {
                node,
                effect,
                attempt,
            },
            RunEvent::EffectClaimed {
                node: of,
                effect: claimed,
                attempt: tried,
                ..
            },
        ) if of == node
            && claimed == effect
            && tried == attempt
            && state.running().contains_key(node) =>
        {
            Matched::Opens(Open::Effect {
                node: node.clone(),
                effect: effect.clone(),
                attempt: *attempt,
                stage: Stage::Claimed,
            })
        }
        (
            Decision::HaltUncertainEffect { node, effect },
            RunEvent::EffectUncertain {
                node: of,
                effect: doubted,
                ..
            },
        ) if of == node && doubted == effect => Matched::Done,
        (
            Decision::StopLoop { region, reason },
            RunEvent::LoopStopped {
                region: stopped,
                reason: why,
            },
        ) if stopped == region && why == reason => Matched::Done,
        (
            Decision::StartIteration { region, index },
            RunEvent::IterationStarted {
                region: opened,
                index: at,
                ..
            },
        ) if opened == region && at == index => Matched::Done,
        (
            Decision::FinishIteration { region, index },
            RunEvent::IterationFinished {
                region: closed,
                record,
            },
        ) if closed == region && record.index == *index => Matched::Done,
        (Decision::TimeOutNode { node }, RunEvent::NodeFailed { node: failed, .. })
            if failed == node =>
        {
            Matched::Done
        }
        (Decision::Compensate { compensation }, event) if is_compensation(compensation, event) => {
            Matched::Done
        }
        (Decision::Cancel { by }, RunEvent::Cancelled { by: who }) if who == by => Matched::Done,
        (Decision::Suspend { wait }, RunEvent::Suspended { wait: on }) if on == wait => {
            Matched::Done
        }
        (Decision::ResumeWait { wait }, RunEvent::Resumed { wait: on, .. }) if on == wait => {
            Matched::Done
        }
        (Decision::Fail { reason }, RunEvent::Failed { reason: why }) if why == reason => {
            Matched::Done
        }
        _ => Matched::Not,
    }
}

/// What carrying out a compensation writes: that it happened, or, when it did
/// not or might not have, that the run ends with the effect still standing.
fn is_compensation(compensation: &failure::Compensation, event: &RunEvent) -> bool {
    match event {
        RunEvent::Compensated {
            node,
            effect,
            compensation: route,
            ..
        } => {
            *node == compensation.node
                && *effect == compensation.effect
                && *route == compensation.route
        }
        RunEvent::Failed {
            reason: RunFailure::EffectUncertain { node, effect },
        } => *node == compensation.node && *effect == compensation.effect,
        _ => false,
    }
}

/// Whether `event` carries out the compensation the run owes first.
fn compensates(definition: &Definition, state: &RunState, event: &RunEvent) -> bool {
    failure::pending_compensations(definition, state)
        .first()
        .is_some_and(|owed| is_compensation(owed, event))
}

fn describe(event: &RunEvent) -> String {
    match event {
        RunEvent::NodeStarted { node }
        | RunEvent::NodeFinished { node, .. }
        | RunEvent::NodeFailed { node, .. }
        | RunEvent::ResourcesConsumed { node, .. } => format!("{} `{node}`", event.as_str()),
        RunEvent::EffectClaimed {
            node,
            effect,
            attempt,
            ..
        }
        | RunEvent::EffectFinalized {
            node,
            effect,
            attempt,
            ..
        }
        | RunEvent::EffectAbandoned {
            node,
            effect,
            attempt,
            ..
        }
        | RunEvent::EffectUncertain {
            node,
            effect,
            attempt,
        } => format!("{} `{node}`/`{effect}` attempt {attempt}", event.as_str()),
        RunEvent::IterationStarted { region, index, .. } => {
            format!("{} `{region}` #{index}", event.as_str())
        }
        RunEvent::IterationFinished { region, record } => {
            format!("{} `{region}` #{}", event.as_str(), record.index)
        }
        RunEvent::LoopStopped { region, .. } => format!("{} `{region}`", event.as_str()),
        _ => event.as_str().to_string(),
    }
}

fn describe_decision(decision: &Decision) -> String {
    match decision {
        Decision::Start => "start the run".to_string(),
        Decision::StartNode { node } => format!("start `{node}`"),
        Decision::PerformEffect {
            node,
            effect,
            attempt,
        } => format!("perform `{node}`/`{effect}` attempt {attempt}"),
        Decision::RetryClaimedEffect {
            node,
            effect,
            attempt,
        } => format!("retry `{node}`/`{effect}` attempt {attempt}"),
        Decision::HaltUncertainEffect { node, effect } => {
            format!("halt on the uncertain `{node}`/`{effect}`")
        }
        Decision::StopLoop { region, .. } => format!("stop the loop `{region}`"),
        Decision::StartIteration { region, index } => format!("open `{region}` #{index}"),
        Decision::FinishIteration { region, index } => format!("close `{region}` #{index}"),
        Decision::Compensate { compensation } => format!(
            "compensate `{}`/`{}`",
            compensation.node, compensation.effect
        ),
        Decision::Cancel { .. } => "cancel".to_string(),
        Decision::TimeOutNode { node } => format!("time out `{node}`"),
        Decision::Suspend { .. } => "suspend".to_string(),
        Decision::ResumeWait { .. } => "resume the wait".to_string(),
        Decision::Complete => "complete".to_string(),
        Decision::Fail { reason } => format!("fail: {}", reason.describe()),
        Decision::Idle => "nothing: there was nothing to do".to_string(),
    }
}
