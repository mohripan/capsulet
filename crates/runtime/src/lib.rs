//! The durable graph runtime's decision core.
//!
//! This crate decides; it does not act. Given a definition, a run's event log,
//! and a time the caller supplies, it says what may happen next. The worker
//! that performs the work lives elsewhere, holds the database handle, and
//! appends the events that result.
//!
//! The split exists for one reason: recovery has to reconstruct exactly what a
//! running worker knew. If deciding depended on ambient state — a clock, a
//! connection, a field somebody set — then a recovered run could reach a
//! different conclusion from the run it is resuming, and nobody would find out
//! until an effect happened twice. Here, state is a fold over the log and the
//! decision is a function of that fold, so a worker that starts fresh and one
//! that recovers compute the same thing by the same route.
//!
//! Purity is enforced by `tests/purity.rs` rather than trusted, exactly as in
//! [`capsulet_ir`].

pub mod decide;
pub mod effect;
pub mod event;
pub mod failure;
pub mod loops;
pub mod state;
pub mod wait;

pub use decide::{Decision, decide};
pub use effect::{EffectAttempt, EffectContext, EffectError, KeySource, check_effect_keys};
pub use event::{Epoch, RecordedEvent, RunEvent, RunFailure, Wait};
pub use failure::{Compensation, TimedOut};
pub use loops::{Repair, repair};
pub use state::{
    FoldError, LoopProgress, NodeFailure, OutstandingEffect, RunState, RunStatus, Spent,
};
pub use wait::{Signal, SignalKind, WaitError, resume, suspend};
