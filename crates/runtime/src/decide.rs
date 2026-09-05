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
//! prevent. Then loops, then unanswered failures, and only then new work: a run
//! that starts a fresh node while a failure sits unhandled is harder to explain
//! afterwards than it was to write.

use capsulet_ir::correctness::evidence::RecordedTime;
use capsulet_ir::definition::Definition;
use capsulet_ir::effect::Idempotency;
use capsulet_ir::id::Identifier;
use capsulet_ir::loop_region::StopReason;
use capsulet_ir::node::NodeKind;
use capsulet_ir::region::RegionKind;

use crate::event::{RunFailure, Wait};
use crate::failure::{self, Compensation};
use crate::loops::{self, Repair};
use crate::state::{NodeFailure, RunState, RunStatus};

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
    /// Undo a reversible effect that happened, before the run ends.
    Compensate { compensation: Compensation },
    /// End the run because somebody asked, at a point where stopping is safe.
    Cancel { by: Identifier },
    /// A node has outrun its declared wall time.
    TimeOutNode { node: Identifier },
    /// Suspend durably on something outside the run.
    Suspend { wait: Wait },
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
    let next = decide_next(definition, state, now);
    // A run that is about to fail still owes whatever it published. Ending
    // first and compensating afterwards is not an option: after the terminal
    // event there is no run left to do it.
    if matches!(next, Decision::Fail { .. })
        && let Some(compensation) = failure::pending_compensations(definition, state).first()
    {
        return Decision::Compensate {
            compensation: compensation.clone(),
        };
    }
    next
}

fn decide_next(definition: &Definition, state: &RunState, now: RecordedTime) -> Decision {
    if state.status().is_terminal() {
        return Decision::Idle;
    }

    // An effect nobody can account for outranks everything else.
    if let Some(decision) = decide_outstanding_effect(definition, state) {
        return decision;
    }

    // An effect nobody could account for ends the run, whatever else remains.
    // Carrying on past one would mean building on a step whose outcome this
    // system is not entitled to assume either way.
    if let Some((node, effect)) = state.uncertain_effects().first() {
        return Decision::Fail {
            reason: RunFailure::EffectUncertain {
                node: node.clone(),
                effect: effect.clone(),
            },
        };
    }

    // Cancellation is honoured here: after any outstanding effect has been
    // resolved, and before anything new is started. Stopping earlier would end
    // the run with something in flight; stopping later would perform an effect
    // somebody has already asked not to happen.
    if let Some(by) = state.cancellation_requested() {
        if let Some(compensation) = failure::pending_compensations(definition, state).first() {
            return Decision::Compensate {
                compensation: compensation.clone(),
            };
        }
        if failure::safe_to_stop(state) {
            return Decision::Cancel { by: by.clone() };
        }
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

    // A node that outran its declared budget while nobody was watching it — the
    // reattach case — becomes a typed failure, which the loop it sits in can
    // then route like any other.
    if let Some(timed_out) = failure::timed_out_node(definition, state, now) {
        return Decision::TimeOutNode {
            node: timed_out.node,
        };
    }

    if failure::run_deadline_passed(definition, state, now) {
        return Decision::Fail {
            reason: RunFailure::BudgetExhausted {
                resource: "wall time".to_string(),
            },
        };
    }

    if let Some(decision) = decide_loop(definition, state) {
        return decision;
    }

    if let Some(decision) = decide_failure(definition, state) {
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

/// Checks every loop's bounds, invariants, and progress against its log.
fn decide_loop(definition: &Definition, state: &RunState) -> Option<Decision> {
    for region in definition.graph.regions() {
        let RegionKind::Loop { spec } = &region.kind else {
            continue;
        };
        let progress = state.loop_progress(&region.id);
        // A loop that stopped for anything but finishing its work leaves the
        // run with nowhere to go. Carrying on past it would mean running the
        // body of a loop that has already been stopped.
        if let Some(stopped) = &progress.stopped {
            if stopped.is_completion() {
                continue;
            }
            return Some(Decision::Fail {
                reason: RunFailure::LoopStopped {
                    region: region.id.clone(),
                    reason: stopped.clone(),
                },
            });
        }

        let reason = loops::exhausted(spec, &progress, state)
            .or_else(|| loops::invariant_failure(&progress))
            .or_else(|| {
                spec.progress
                    .as_ref()
                    .and_then(|measure| loops::non_progress(measure, &progress))
            });

        if let Some(reason) = reason {
            return Some(Decision::StopLoop {
                region: region.id.clone(),
                reason,
            });
        }
    }
    None
}

/// Answers the first failure nobody has answered.
///
/// A failed node that is neither running nor finished is waiting for a
/// decision. Where the loop it sits in declared a route for that kind of
/// failure, the route is the decision. Where it did not, the run fails —
/// silence in a declaration is not consent to retry.
fn decide_failure(definition: &Definition, state: &RunState) -> Option<Decision> {
    let (node, failure) = state.unresolved_failures().into_iter().next()?;
    let stop = || {
        Some(Decision::Fail {
            reason: RunFailure::Node {
                node: node.clone(),
                failure: failure.kind,
                detail: failure.detail.clone(),
            },
        })
    };

    let Some(region) = definition.graph.region_of_node(node) else {
        return stop();
    };
    let RegionKind::Loop { spec } = &region.kind else {
        return stop();
    };
    // A loop that has already stopped is not going to repair anything.
    if state.loop_progress(&region.id).stopped.is_some() {
        return stop();
    }

    loops::repair(spec, state, node, failure.kind).map_or_else(
        // The loop declared nothing for this failure kind.
        stop,
        |repair| Some(apply_repair(repair, &region.id, node, failure)),
    )
}

/// Turns a declared route into the decision that carries it out.
fn apply_repair(
    repair: Repair,
    region: &Identifier,
    failed: &Identifier,
    failure: &NodeFailure,
) -> Decision {
    match repair {
        Repair::Retry { node, .. } | Repair::Hand { node } => Decision::StartNode { node },
        // Escalation is a durable wait, not a spin: the run suspends until the
        // named authority decides, and nothing is held in worker memory.
        Repair::Escalate { to } => Decision::Suspend {
            wait: Wait::HumanGate {
                node: failed.clone(),
                obligation: to,
            },
        },
        Repair::Reject => Decision::Fail {
            reason: RunFailure::Node {
                node: failed.clone(),
                failure: failure.kind,
                detail: failure.detail.clone(),
            },
        },
        Repair::Exhausted { failure } => Decision::StopLoop {
            region: region.clone(),
            reason: StopReason::RepairExhausted { failure },
        },
    }
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
        .filter(|node| !state.running().contains_key(&node.id))
        .find(|node| {
            definition
                .graph
                .predecessors_of(&node.id)
                .into_iter()
                .all(|predecessor| state.has_finished(predecessor))
        })
        .map(|node| node.id.clone())
}
