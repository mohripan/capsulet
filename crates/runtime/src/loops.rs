//! Loops that a restart cannot reset.
//!
//! Everything a loop has spent — iterations, tokens, cost, wall time — is
//! counted from its log, so a worker that dies mid-loop and a worker that picks
//! it up reach the same answer about how much is left. A loop whose budget was
//! exhausted before the crash is still exhausted after it. That is the whole
//! point: a bound that a restart hands back is not a bound.
//!
//! The three ways a loop stops short each need something the fold does not
//! have, which is why they live here rather than in [`crate::state`]:
//!
//! - an invariant failure needs the iteration's recorded outcomes;
//! - non-progress needs the *declared direction*, because a measure that moved
//!   the wrong way has not made progress even though it moved;
//! - a repair route needs the declaration to say what a failure kind leads to,
//!   and the log to say how many attempts it has already had.
//!
//! Nothing here executes an iteration. It produces the events a worker appends
//! and reads the ones already appended.

use capsulet_ir::id::Identifier;
use capsulet_ir::loop_region::{
    BudgetKind, FailureKind, IterationRecord, LoopSpec, ProgressDirection, ProgressMeasure, Route,
    StopReason,
};

use crate::event::RunEvent;
use crate::state::{LoopProgress, RunState};

/// What a declared route says to do about a failure that has happened.
///
/// One case per [`Route`], plus the one a route does not cover: the attempts it
/// allowed are used up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Repair {
    /// Run the node again. `attempt` is how many times it has already failed,
    /// so the first retry is attempt 1.
    Retry { node: Identifier, attempt: u32 },
    /// Hand the failure to the node that repairs it.
    Hand { node: Identifier },
    /// Stop and wait for the named authority.
    Escalate { to: Identifier },
    /// Stop, with the failure recorded.
    Reject,
    /// The declared attempts are spent.
    Exhausted { failure: FailureKind },
}

/// The route a loop declared for this kind of failure, if it declared one.
///
/// Absence is meaningful: a loop that says nothing about a failure kind has not
/// silently opted into retrying it. The caller fails the run instead, which is
/// the honest reading of a declaration that does not mention this case.
#[must_use]
pub fn route_for(spec: &LoopSpec, failure: FailureKind) -> Option<&Route> {
    spec.repairs
        .iter()
        .find(|declared| declared.failure == failure)
        .map(|declared| &declared.route)
}

/// What to do about a failure, given what the loop declared and what has
/// already been tried.
///
/// The attempt count comes from the log, so a worker that restarts mid-repair
/// does not get the retry budget back.
#[must_use]
pub fn repair(
    spec: &LoopSpec,
    state: &RunState,
    node: &Identifier,
    failure: FailureKind,
) -> Option<Repair> {
    match route_for(spec, failure)? {
        Route::Retry {
            node: target,
            attempts,
        } => {
            let already = state.failure_count(node, failure);
            if already > *attempts {
                Some(Repair::Exhausted { failure })
            } else {
                Some(Repair::Retry {
                    node: target.clone(),
                    attempt: already,
                })
            }
        }
        Route::Repair { node: target } => Some(Repair::Hand {
            node: target.clone(),
        }),
        Route::Escalate { to } => Some(Repair::Escalate { to: to.clone() }),
        Route::Reject => Some(Repair::Reject),
    }
}

/// Which bound, if any, a loop has reached.
///
/// Counted from iterations *started*, because an iteration that began and then
/// crashed still spent what it spent.
#[must_use]
pub fn exhausted(spec: &LoopSpec, progress: &LoopProgress, state: &RunState) -> Option<StopReason> {
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
    if spec.budget.tokens > 0 && spent.tokens >= spec.budget.tokens {
        return Some(StopReason::BudgetExhausted {
            budget: BudgetKind::Tokens,
        });
    }
    if spec.budget.cost_micro_units > 0 && spent.cost_micro_units >= spec.budget.cost_micro_units {
        return Some(StopReason::BudgetExhausted {
            budget: BudgetKind::Cost,
        });
    }
    if spec.budget.effect_count > 0 && spent.effects >= spec.budget.effect_count {
        return Some(StopReason::BudgetExhausted {
            budget: BudgetKind::Effects,
        });
    }
    None
}

/// The invariant that did not hold, if one did not.
#[must_use]
pub fn invariant_failure(progress: &LoopProgress) -> Option<StopReason> {
    progress
        .failed_invariant
        .clone()
        .map(|invariant| StopReason::InvariantFailed { invariant })
}

/// Whether the declared measure has stopped making progress.
///
/// Two ways to fail, and the second is why this needs the declaration. A
/// measure that has not moved for two consecutive iterations is going nowhere.
/// A measure that moved *against* the direction it was declared to move is
/// worse than going nowhere, and one such reading is enough — a loop declared
/// strictly decreasing that went up has contradicted its own declaration.
#[must_use]
pub fn non_progress(measure: &ProgressMeasure, progress: &LoopProgress) -> Option<StopReason> {
    let stopped = || StopReason::NonProgress {
        measure: measure.id.clone(),
    };

    if progress.stalled >= 2 {
        return Some(stopped());
    }

    let (Some(last), Some(previous)) = (progress.last_progress, progress.previous_progress) else {
        return None;
    };
    let moved_the_declared_way = match measure.direction {
        ProgressDirection::StrictlyDecreasing => last < previous,
        ProgressDirection::StrictlyIncreasing => last > previous,
    };
    (!moved_the_declared_way && last != previous).then(stopped)
}

/// The event that opens an iteration.
///
/// Written before the iteration runs, so a crash inside it still spent the
/// iteration. That is deliberate: a budget that only counts what finished is a
/// budget a crash loop can spend forever.
#[must_use]
pub fn iteration_started(region: &Identifier, index: u32) -> RunEvent {
    RunEvent::IterationStarted {
        region: region.clone(),
        index,
    }
}

/// The event that closes an iteration, carrying what it did.
#[must_use]
pub fn iteration_finished(region: &Identifier, record: IterationRecord) -> RunEvent {
    RunEvent::IterationFinished {
        region: region.clone(),
        record: Box::new(record),
    }
}

/// The event that ends a loop, with the reason it ended.
///
/// Only [`StopReason::ConditionFalse`] means the loop did what it set out to
/// do. Recording the reason rather than a boolean is what stops a budget
/// exhaustion from being read later as a success.
#[must_use]
pub fn loop_stopped(region: &Identifier, reason: StopReason) -> RunEvent {
    RunEvent::LoopStopped {
        region: region.clone(),
        reason,
    }
}
