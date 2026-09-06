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
use capsulet_ir::loop_region::{LoopSpec, StopReason};
use capsulet_ir::node::NodeKind;
use capsulet_ir::region::{Region, RegionKind};

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
    /// Open an iteration of a loop region, resetting its nodes so they run
    /// again.
    StartIteration { region: Identifier, index: u32 },
    /// Close the open iteration, recording what it did.
    FinishIteration { region: Identifier, index: u32 },
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

/// The decision itself, before compensation is taken into account.
fn decide_next(definition: &Definition, state: &RunState, now: RecordedTime) -> Decision {
    if state.status().is_terminal() {
        return Decision::Idle;
    }

    // A claim still open outranks every other kind of work: it is the one thing
    // that gets worse the longer it is left, and proceeding past it would be
    // the duplication this milestone exists to prevent.
    if let Some(decision) = decide_outstanding_effect(definition, state) {
        return decision;
    }

    // A claim the run gave up on, having failed to account for it, ends the
    // run. Carrying on would mean building on a step whose outcome this system
    // is not entitled to assume either way.
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

    // Loop regions before ordinary nodes: a region's body only runs inside an
    // iteration, and opening one is what makes those nodes ready at all.
    if let Some(decision) = decide_iteration(definition, state) {
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

        let reason = loops::exhausted(spec, state)
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

/// Opens, closes, and ends loop iterations.
///
/// A region's body is not ordinary graph work. Its nodes run once per
/// iteration, and between iterations they are reset, so "has this node
/// finished" is a question about the current iteration rather than about the
/// run. Deciding that here — before the ordinary readiness check — is what
/// keeps the two from being confused.
fn decide_iteration(definition: &Definition, state: &RunState) -> Option<Decision> {
    // Every loop region gets a look. Returning early on the first one with an
    // iteration in flight would leave a sibling loop unable to start, because
    // its nodes are only ready while it has an iteration open.
    definition
        .graph
        .regions()
        .find_map(|region| match &region.kind {
            RegionKind::Loop { spec } => decide_one_loop(definition, state, region, spec),
            RegionKind::Plain => None,
        })
}

/// What one loop region's next step is, if it has one.
fn decide_one_loop(
    definition: &Definition,
    state: &RunState,
    region: &Region,
    spec: &LoopSpec,
) -> Option<Decision> {
    let progress = state.loop_progress(&region.id);
    if progress.stopped.is_some() {
        return None;
    }

    // An iteration is open when one has started and not finished. It closes
    // when the region's exit has run; until then the body is still going, and
    // ordinary readiness takes it from here, restricted to this region's nodes.
    if progress.started > progress.finished {
        return state
            .has_finished(&region.exit)
            .then(|| Decision::FinishIteration {
                region: region.id.clone(),
                index: progress.finished,
            });
    }

    // No iteration open, and the loop has not run at all.
    if progress.finished == 0 {
        return region_is_reachable(definition, state, region).then(|| Decision::StartIteration {
            region: region.id.clone(),
            index: 0,
        });
    }

    // The last iteration finished, so the continuation says what happens next.
    let continuation = &spec.continuation;
    let reading = state
        .control_value(&continuation.evaluated_by, continuation.port.as_str())
        .and_then(crate::event::ControlValue::as_bool);
    let Some(keep_going) = reading else {
        return Some(Decision::Fail {
            reason: RunFailure::ControlMissing {
                region: region.id.clone(),
                node: continuation.evaluated_by.clone(),
                port: continuation.port.as_str().to_string(),
                expected: "a boolean continuation".to_string(),
            },
        });
    };

    if !keep_going {
        // The only stop reason that means the loop finished its work.
        return Some(Decision::StopLoop {
            region: region.id.clone(),
            reason: StopReason::ConditionFalse,
        });
    }

    // The count is checked here: not before the continuation is read, because a
    // loop whose condition has gone false did not exhaust anything and naming a
    // budget as its reason would be a false statement about a loop that
    // finished; and not continuously, because stopping mid-iteration throws
    // away the work it did and leaves no record that it ran.
    Some(loops::iterations_exhausted(spec, &progress).map_or_else(
        || Decision::StartIteration {
            region: region.id.clone(),
            index: progress.finished,
        },
        |reason| Decision::StopLoop {
            region: region.id.clone(),
            reason,
        },
    ))
}

/// Whether everything the region depends on from outside it has finished.
fn region_is_reachable(definition: &Definition, state: &RunState, region: &Region) -> bool {
    definition
        .graph
        .predecessors_of(&region.entry)
        .into_iter()
        .filter(|node| !region.nodes.contains(*node))
        .all(|node| state.has_finished(node))
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
        // A node inside a loop region runs as part of an iteration, never on
        // its own. Without this a region's body would run once as ordinary
        // graph work and the loop would never get to drive it.
        .filter(|node| iteration_is_open_for(definition, state, &node.id))
        .find(|node| {
            definition
                .graph
                .predecessors_of(&node.id)
                .into_iter()
                .all(|predecessor| predecessor_is_settled(definition, state, &node.id, predecessor))
        })
        .map(|node| node.id.clone())
}

/// Whether a predecessor has finished in a way the dependent node can rely on.
///
/// A node inside a loop finishes once per iteration, and finishing is not the
/// same as being done: the next iteration resets it and runs it again. Within
/// the loop that is exactly what the next node wants — it is reading this
/// iteration's value. Outside it, it is not: a node downstream of the region
/// that started on the strength of one iteration's output would be acting on a
/// value the loop was still working on. So a predecessor inside a loop counts
/// for its own region's nodes at once, and for everybody else only once the
/// loop has stopped.
fn predecessor_is_settled(
    definition: &Definition,
    state: &RunState,
    dependent: &Identifier,
    predecessor: &Identifier,
) -> bool {
    if !state.has_finished(predecessor) {
        return false;
    }
    let Some(region) = definition.graph.region_of_node(predecessor) else {
        return true;
    };
    if !matches!(region.kind, RegionKind::Loop { .. }) || region.nodes.contains(dependent) {
        return true;
    }
    state.loop_progress(&region.id).stopped.is_some()
}

/// Whether a node may run right now, given the region it sits in.
///
/// A node outside every loop region always may. One inside a loop may only
/// while that loop has an iteration open.
fn iteration_is_open_for(definition: &Definition, state: &RunState, node: &Identifier) -> bool {
    let Some(region) = definition.graph.region_of_node(node) else {
        return true;
    };
    if !matches!(region.kind, RegionKind::Loop { .. }) {
        return true;
    }
    let progress = state.loop_progress(&region.id);
    progress.stopped.is_none() && progress.started > progress.finished
}
