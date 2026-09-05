//! Stopping: on request, on a deadline, and on the way out.
//!
//! Three things that look unrelated and are not. Each is a question about when
//! it is *safe* to stop, and each has the same wrong answer available: stop
//! immediately. A run cancelled between claiming an effect and finalizing it
//! ends with something nobody can account for. A node timed out while its
//! worker is still alive gets its work done twice. A run that fails after
//! publishing something reversible leaves it published.
//!
//! So cancellation is a request the decision core honours at the next safe
//! point, a deadline is measured from times the log recorded rather than a
//! clock this crate reads, and compensation is work the run still owes before
//! it may end.

use capsulet_ir::correctness::evidence::RecordedTime;
use capsulet_ir::definition::Definition;
use capsulet_ir::digest::Digest;
use capsulet_ir::effect::Reversibility;
use capsulet_ir::id::Identifier;
use capsulet_ir::loop_region::FailureKind;

use crate::event::RunEvent;
use crate::state::RunState;

/// A reversible effect that happened and has not been undone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compensation {
    pub node: Identifier,
    pub effect: Identifier,
    /// The route the IR declared for undoing it.
    pub route: Identifier,
}

/// A node that has been running past its declared deadline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedOut {
    pub node: Identifier,
    pub started_at: RecordedTime,
    pub deadline_ms: u64,
}

/// The event that asks a run to stop.
///
/// Deliberately not the stop itself. Whoever asks does not know whether an
/// effect is in flight, and the run does.
#[must_use]
pub fn request_cancellation(by: Identifier) -> RunEvent {
    RunEvent::CancellationRequested { by }
}

/// The event that ends a cancelled run.
#[must_use]
pub fn cancelled(by: Identifier) -> RunEvent {
    RunEvent::Cancelled { by }
}

/// The event recording that an effect was undone.
#[must_use]
pub fn compensated(compensation: &Compensation, receipt: Digest) -> RunEvent {
    RunEvent::Compensated {
        node: compensation.node.clone(),
        effect: compensation.effect.clone(),
        compensation: compensation.route.clone(),
        receipt,
    }
}

/// Whether it is safe to stop the run now.
///
/// It is not safe while an effect is claimed and unresolved: ending there
/// leaves something outstanding that nobody will ever be able to account for,
/// which is a worse outcome than taking a moment longer to stop.
#[must_use]
pub fn safe_to_stop(state: &RunState) -> bool {
    state.outstanding_effects().is_empty()
}

/// What the run still owes before it may end, most recent first.
///
/// Reverse order because compensations undo a sequence: the last thing done is
/// the first thing to take back.
#[must_use]
pub fn pending_compensations(definition: &Definition, state: &RunState) -> Vec<Compensation> {
    state
        .finalized_effects()
        .iter()
        .rev()
        .filter(|(node, effect)| !state.is_compensated(node, effect))
        .filter_map(|(node, effect)| {
            let declared = definition
                .graph
                .node(node)?
                .effects
                .iter()
                .find(|each| &each.id == effect)?;
            // An irreversible effect has no compensation, and pretending it
            // does is how a system claims to have undone something it has not.
            match &declared.reversibility {
                Reversibility::Reversible { compensation } => Some(Compensation {
                    node: node.clone(),
                    effect: effect.clone(),
                    route: compensation.clone(),
                }),
                Reversibility::Irreversible => None,
            }
        })
        .collect()
}

/// The first node that has outrun its declared wall-time budget.
///
/// Measured from the time the log says it started, against the time the caller
/// supplies. Neither is read from a clock here, so a worker that reattaches to
/// a node a dead worker left running sees the same deadline the dead one did
/// rather than starting the timer again.
///
/// A node with no declared wall time is not timed out; a budget of zero would
/// mean a node that can never run, which admission already refuses.
#[must_use]
pub fn timed_out_node(
    definition: &Definition,
    state: &RunState,
    now: RecordedTime,
) -> Option<TimedOut> {
    state.running().iter().find_map(|(node, started_at)| {
        let declared = definition.graph.node(node)?;
        let deadline_ms = declared.budget.wall_ms;
        if deadline_ms == 0 {
            return None;
        }
        let elapsed = now.epoch_millis().saturating_sub(started_at.epoch_millis());
        (elapsed >= 0 && u64::try_from(elapsed).unwrap_or(u64::MAX) >= deadline_ms).then(|| {
            TimedOut {
                node: node.clone(),
                started_at: *started_at,
                deadline_ms,
            }
        })
    })
}

/// The event a timeout produces.
///
/// A typed failure rather than a bare stop, so the loop it sits in can route it
/// like any other failure — a timeout answered by a declared retry is a normal
/// thing for a workflow to want.
#[must_use]
pub fn timed_out(node: &TimedOut) -> RunEvent {
    RunEvent::NodeFailed {
        node: node.node.clone(),
        failure: FailureKind::BudgetExhaustion,
        detail: format!(
            "the node passed its declared wall-time budget of {}ms",
            node.deadline_ms
        ),
    }
}

/// Whether the run as a whole has outrun its declared wall time.
#[must_use]
pub fn run_deadline_passed(definition: &Definition, state: &RunState, now: RecordedTime) -> bool {
    let Some(started_at) = state.started_at() else {
        return false;
    };
    let budget = definition.budget.wall_ms;
    if budget == 0 {
        return false;
    }
    let elapsed = now.epoch_millis().saturating_sub(started_at.epoch_millis());
    elapsed >= 0 && u64::try_from(elapsed).unwrap_or(u64::MAX) >= budget
}
