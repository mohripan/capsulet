//! What may happen next.
//!
//! `decide` is a pure function of the definition, the folded state, and a time
//! the caller supplies. It returns what the worker is permitted to do; the
//! worker does it and appends the resulting events, and the next call sees them.
//!
//! Nothing here performs an action, and nothing here reads a clock. That is
//! what lets the same function drive a healthy run, a recovered one, and a test
//! that steps a run through a hundred crash points without a database in sight.
//!
//! The order of the checks matters and is deliberate. Terminal states first,
//! because a finished run has nothing to decide. Outstanding effects next,
//! because an effect nobody can account for outranks any amount of remaining
//! work — proceeding past one would be the duplication this milestone exists to
//! prevent.

use capsulet_ir::correctness::evidence::RecordedTime;
use capsulet_ir::definition::Definition;
use capsulet_ir::effect::Idempotency;
use capsulet_ir::id::Identifier;
use capsulet_ir::loop_region::{BudgetKind, LoopSpec, StopReason};
use capsulet_ir::node::NodeKind;
use capsulet_ir::region::RegionKind;

use crate::event::{RunFailure, Wait};
use crate::state::{RunState, RunStatus};

/// Something the worker is permitted to do now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Begin the run.
    Start,
    /// Run a node whose predecessors have all finished.
    StartNode { node: Identifier },
    /// Perform a declared effect, claiming it first.
    PerformEffect {
        node: Identifier,
        effect: Identifier,
        /// Attempt number, so a retry claims a distinct row.
        attempt: u32,
    },
    /// Re-attempt an effect that was claimed but never finalized, because the
    /// IR declares repeating it is safe.
    RetryClaimedEffect {
        node: Identifier,
        effect: Identifier,
        attempt: u32,
    },
    /// Stop: an effect was claimed, its outcome is unknown, and the IR says it
    /// must not be repeated. Guessing here is exactly the failure mode the
    /// declaration exists to prevent.
    HaltUncertainEffect {
        node: Identifier,
        effect: Identifier,
    },
    /// Stop a loop, with the reason recorded.
    StopLoop {
        region: Identifier,
        reason: StopReason,
    },
    /// The wait the run is suspended on is due.
    ResumeWait { wait: Wait },
    /// Every node has finished.
    Complete,
    /// Stop the run.
    Fail { reason: RunFailure },
    /// There is nothing to do until something outside the run changes.
    Idle,
}

/// Decides what the worker may do next.
///
/// `now` is supplied rather than read, so a timer can be evaluated without this
/// crate reaching for a clock and without a test having to wait.
#[must_use]
pub fn decide(definition: &Definition, state: &RunState, now: RecordedTime) -> Decision {
    if state.status().is_terminal() {
        return Decision::Idle;
    }

    // An effect nobody can account for outranks everything else.
    if let Some(decision) = decide_outstanding_effect(definition, state) {
        return decision;
    }

    if state.status() == RunStatus::Waiting {
        return match state.waiting_on() {
            Some(Wait::Timer { until }) if until.epoch_millis() <= now.epoch_millis() => {
                Decision::ResumeWait {
                    wait: Wait::Timer { until: *until },
                }
            }
            // An event or a human decision arrives from outside; the run has
            // nothing to do until it does.
            _ => Decision::Idle,
        };
    }

    if state.status() == RunStatus::Queued {
        return Decision::Start;
    }

    if let Some(decision) = decide_loop(definition, state) {
        return decision;
    }

    if let Some(node) = next_ready_node(definition, state) {
        let declared = definition.graph.node(&node);
        // An effect node's whole purpose is its effect, so starting it means
        // performing that effect under a claim.
        if let Some(effect) = declared
            .filter(|declared| declared.kind == NodeKind::Effect)
            .and_then(|declared| declared.effects.first())
            && !state.effect_completed(&node, &effect.id)
        {
            return Decision::PerformEffect {
                node,
                effect: effect.id.clone(),
                attempt: 0,
            };
        }
        return Decision::StartNode { node };
    }

    if !state.running().is_empty() {
        // Work is in flight elsewhere.
        return Decision::Idle;
    }

    if definition
        .graph
        .nodes()
        .all(|node| state.has_finished(&node.id))
    {
        return Decision::Complete;
    }

    // Nothing is ready, nothing is running, and nodes remain: the graph cannot
    // make progress. Saying so is better than spinning.
    Decision::Idle
}

/// Resolves a claim left behind by a crash, according to what the IR declared.
fn decide_outstanding_effect(definition: &Definition, state: &RunState) -> Option<Decision> {
    let claim = state.outstanding_effects().first()?;
    let declared = definition
        .graph
        .node(&claim.node)
        .and_then(|node| node.effects.iter().find(|effect| effect.id == claim.effect))?;

    match &declared.idempotency {
        // Repeating changes nothing, so repeat.
        Idempotency::Idempotent => Some(Decision::RetryClaimedEffect {
            node: claim.node.clone(),
            effect: claim.effect.clone(),
            attempt: claim.attempt.saturating_add(1),
        }),
        // The far side deduplicates on the key this run already used, so
        // repeating with that same key is safe.
        Idempotency::Keyed { .. } => Some(Decision::RetryClaimedEffect {
            node: claim.node.clone(),
            effect: claim.effect.clone(),
            attempt: claim.attempt,
        }),
        // Nobody can say whether it happened, and the definition says it must
        // not happen twice. Stop.
        Idempotency::NonIdempotent => Some(Decision::HaltUncertainEffect {
            node: claim.node.clone(),
            effect: claim.effect.clone(),
        }),
    }
}

/// Checks every loop's bounds against what the log says it has spent.
fn decide_loop(definition: &Definition, state: &RunState) -> Option<Decision> {
    for region in definition.graph.regions() {
        let RegionKind::Loop { spec } = &region.kind else {
            continue;
        };
        let progress = state.loop_progress(&region.id);
        if progress.stopped.is_some() {
            continue;
        }
        if let Some(reason) = exhausted(spec, &progress, state) {
            return Some(Decision::StopLoop {
                region: region.id.clone(),
                reason,
            });
        }
    }
    None
}

/// Which bound, if any, this loop has reached.
///
/// Counted from iterations *started*, because an iteration that began and then
/// crashed still spent what it spent.
fn exhausted(
    spec: &LoopSpec,
    progress: &crate::state::LoopProgress,
    state: &RunState,
) -> Option<StopReason> {
    let spent = state.spent();
    if progress.started >= spec.budget.max_iterations {
        return Some(StopReason::BudgetExhausted {
            budget: BudgetKind::Iterations,
        });
    }
    if spent.wall_ms >= spec.budget.wall_ms {
        return Some(StopReason::BudgetExhausted {
            budget: BudgetKind::WallTime,
        });
    }
    if spent.tokens >= spec.budget.tokens && spec.budget.tokens > 0 {
        return Some(StopReason::BudgetExhausted {
            budget: BudgetKind::Tokens,
        });
    }
    if spent.cost_micro_units >= spec.budget.cost_micro_units && spec.budget.cost_micro_units > 0 {
        return Some(StopReason::BudgetExhausted {
            budget: BudgetKind::Cost,
        });
    }
    // A measure that has not moved for two consecutive iterations is not
    // progress, whichever worker observed them.
    if let Some(measure) = &spec.progress
        && progress.stalled >= 2
    {
        return Some(StopReason::NonProgress {
            measure: measure.id.clone(),
        });
    }
    None
}

/// The next node whose predecessors have all finished.
///
/// Canonical order, so two workers folding the same log choose the same node
/// and a replay makes the same choices as the original run.
fn next_ready_node(definition: &Definition, state: &RunState) -> Option<Identifier> {
    definition
        .graph
        .nodes()
        .filter(|node| !state.has_finished(&node.id))
        .filter(|node| !state.running().contains(&node.id))
        .find(|node| {
            definition
                .graph
                .predecessors_of(&node.id)
                .into_iter()
                .all(|predecessor| state.has_finished(predecessor))
        })
        .map(|node| node.id.clone())
}
