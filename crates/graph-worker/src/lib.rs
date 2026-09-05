//! The durable graph worker.
//!
//! One orchestration path for IR runs: lease a run, fold its log, ask the
//! decision core what may happen next, do that, append what resulted. Nothing
//! here decides anything [`capsulet_runtime`] could decide, which is what makes
//! "kill the worker at any point and restart it" an ordinary thing to do rather
//! than a scenario with its own recovery code.
//!
//! What is left for this crate is the part that genuinely needs the outside
//! world: holding a lease, doing the work, and writing the result down.

pub mod clock;
pub mod execute;
pub mod runtime;
pub mod service;

pub use clock::{Clock, SystemClock};
pub use execute::{
    CompensationRequest, EffectOutcome, EffectRequest, Executor, NodeOutcome, NodeRequest,
};
pub use runtime::{GraphWorker, Progress, WorkerConfig, WorkerError};
pub use service::run;
