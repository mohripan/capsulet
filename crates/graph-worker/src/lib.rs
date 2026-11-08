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

pub mod agent;
pub mod clock;
pub mod execute;
pub mod model;
pub mod runtime;
pub mod service;
mod telemetry;

pub use agent::AgentExecutor;
pub use clock::{Clock, SystemClock};
pub use execute::{
    CompensationRequest, EffectOutcome, EffectRequest, Executor, NodeOutcome, NodeRequest,
    ResolvedInput, ResolvedSource, resolve_inputs,
};
pub use model::{Completion, Embedding, ModelClient, ModelError, ModelRequest, OllamaClient};
pub use runtime::{GraphWorker, Progress, WorkerConfig, WorkerError};
pub use service::run;
