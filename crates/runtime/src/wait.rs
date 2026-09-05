//! Waiting, durably.
//!
//! A run that is waiting holds nothing in memory. It is suspended by an event
//! and resumed by an event, and in between there is no worker, no timer thread,
//! and no callback registration that a restart could lose. Whatever the run is
//! waiting for arrives from outside, and the log is where it lands.
//!
//! Matching a signal to a wait is the interesting part, and it is deliberately
//! strict. A signal that does not match resumes nothing: an event named
//! something else, a timer that is not due, a person who does not hold the
//! authority the gate named. Each of those is a refusal with a reason rather
//! than a resumption that happens to be wrong, because a run that resumed on
//! the wrong signal is a run whose whole remaining history is unexplainable.

use std::collections::BTreeSet;

use capsulet_ir::correctness::evidence::RecordedTime;
use capsulet_ir::id::Identifier;
use thiserror::Error;

use crate::event::{RunEvent, Wait};

/// Something that arrived from outside the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signal {
    pub kind: SignalKind,
    /// Who or what delivered it.
    pub by: Identifier,
    /// The authorities the deliverer holds, as the control plane resolved them.
    ///
    /// Passed in rather than looked up, because deciding who holds what is a
    /// policy question and this crate does not reach for policy — or anything
    /// else — at decision time.
    pub authorities: BTreeSet<Identifier>,
}

impl Signal {
    /// A named external event, from a webhook or another run.
    #[must_use]
    pub fn event(name: Identifier, by: Identifier) -> Self {
        Self {
            kind: SignalKind::Event { name },
            by,
            authorities: BTreeSet::new(),
        }
    }

    /// A person's decision, with the authorities they hold.
    #[must_use]
    pub fn decision(
        obligation: Identifier,
        by: Identifier,
        authorities: BTreeSet<Identifier>,
    ) -> Self {
        Self {
            kind: SignalKind::HumanDecision { obligation },
            by,
            authorities,
        }
    }

    /// A timer the scheduler believes is due. Believing is not enough — the
    /// wait is still checked against the time the caller supplies.
    #[must_use]
    pub fn timer(by: Identifier) -> Self {
        Self {
            kind: SignalKind::TimerElapsed,
            by,
            authorities: BTreeSet::new(),
        }
    }
}

/// What kind of thing arrived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalKind {
    Event { name: Identifier },
    HumanDecision { obligation: Identifier },
    TimerElapsed,
}

/// The event that suspends a run.
#[must_use]
pub fn suspend(wait: Wait) -> RunEvent {
    RunEvent::Suspended { wait }
}

/// Whether a timer is due at the time the caller supplies.
///
/// `now` is a parameter for the same reason it is everywhere else here: a
/// decision that reads a clock cannot be replayed, and recovery is replay.
#[must_use]
pub fn timer_is_due(wait: &Wait, now: RecordedTime) -> bool {
    matches!(wait, Wait::Timer { until } if until.epoch_millis() <= now.epoch_millis())
}

/// Matches a signal against what the run is waiting for.
///
/// # Errors
///
/// Returns [`WaitError::NotWaiting`] when the run is not suspended,
/// [`WaitError::Mismatch`] when the signal is for something else,
/// [`WaitError::NotDue`] for a timer that has not elapsed, and
/// [`WaitError::Unauthorised`] when the deliverer does not hold the authority
/// the gate named.
pub fn resume(
    waiting_on: Option<&Wait>,
    signal: &Signal,
    now: RecordedTime,
) -> Result<RunEvent, WaitError> {
    let wait = waiting_on.ok_or(WaitError::NotWaiting)?;

    match (wait, &signal.kind) {
        (Wait::Timer { until }, SignalKind::TimerElapsed) => {
            if until.epoch_millis() > now.epoch_millis() {
                return Err(WaitError::NotDue { until: *until, now });
            }
        }
        (Wait::Event { name }, SignalKind::Event { name: arrived }) => {
            if name != arrived {
                return Err(WaitError::Mismatch {
                    waiting_on: wait.clone(),
                    arrived: signal.kind.clone(),
                });
            }
        }
        (
            Wait::HumanGate { obligation, .. },
            SignalKind::HumanDecision {
                obligation: decided,
            },
        ) => {
            if obligation != decided {
                return Err(WaitError::Mismatch {
                    waiting_on: wait.clone(),
                    arrived: signal.kind.clone(),
                });
            }
            // A gate that anyone could open is not a gate. The authority named
            // in the wait has to be one the deliverer actually holds.
            if !signal.authorities.contains(obligation) {
                return Err(WaitError::Unauthorised {
                    obligation: obligation.clone(),
                    by: signal.by.clone(),
                });
            }
        }
        _ => {
            return Err(WaitError::Mismatch {
                waiting_on: wait.clone(),
                arrived: signal.kind.clone(),
            });
        }
    }

    Ok(RunEvent::Resumed {
        wait: wait.clone(),
        by: signal.by.clone(),
    })
}

/// Why a signal did not resume a run.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WaitError {
    #[error("the run is not suspended, so there is nothing to resume")]
    NotWaiting,
    #[error("the run is waiting for {waiting_on:?}, and {arrived:?} arrived")]
    Mismatch {
        waiting_on: Wait,
        arrived: SignalKind,
    },
    #[error("the timer is due at {until:?} and it is {now:?}")]
    NotDue {
        until: RecordedTime,
        now: RecordedTime,
    },
    #[error("`{by}` does not hold `{obligation}`, which this gate requires")]
    Unauthorised {
        obligation: Identifier,
        by: Identifier,
    },
}
