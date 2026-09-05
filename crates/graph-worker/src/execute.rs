//! What the worker delegates.
//!
//! The worker knows how to lease, fold, decide, and append. It does not know
//! how to run a verifier, call a model, or open a pull request, and that
//! separation is the point: everything above this trait is testable without a
//! network, and everything below it can fail in whatever way real work fails
//! without the durability story changing.
//!
//! The outcomes are typed rather than `Result<_, Box<dyn Error>>` because a
//! failure's *kind* is what a declared repair route is matched against. An
//! executor that could only say "it went wrong" would leave the routing table
//! in the IR with nothing to route on.

use std::collections::BTreeMap;

use async_trait::async_trait;
use capsulet_ir::definition::Definition;
use capsulet_ir::digest::Digest;
use capsulet_ir::effect::Effect;
use capsulet_ir::id::Identifier;
use capsulet_ir::loop_region::FailureKind;
use capsulet_ir::node::Node;

/// Everything an executor is told about the work it has been asked to do.
#[derive(Debug, Clone, Copy)]
pub struct NodeRequest<'a> {
    pub run: &'a str,
    pub definition: &'a Definition,
    pub node: &'a Node,
}

/// How running a node came out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeOutcome {
    Finished {
        /// Output values by port, content-addressed. Digests rather than values
        /// so a large output does not travel through the log.
        outputs: BTreeMap<String, Digest>,
    },
    Failed {
        failure: FailureKind,
        detail: String,
    },
}

/// An effect the worker has already claimed and is asking to be performed.
#[derive(Debug, Clone, Copy)]
pub struct EffectRequest<'a> {
    pub run: &'a str,
    pub node: &'a Identifier,
    pub effect: &'a Effect,
    pub attempt: u32,
    /// The idempotency key to present, where the effect declared one. Presenting
    /// a different key than the last attempt would make a retry a second effect.
    pub key: Option<&'a str>,
}

/// How performing an effect came out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectOutcome {
    /// It happened, and here is the proof.
    Performed { receipt: Digest },
    /// It did not happen, and the executor is sure of that.
    ///
    /// Sure is the operative word. Report this only when the far side rejected
    /// the request outright; a timeout is not this.
    Failed {
        failure: FailureKind,
        detail: String,
    },
    /// Nobody can say whether it happened — a timeout, a dropped connection, a
    /// response that never arrived.
    ///
    /// Saying so is far more useful than guessing, because what the runtime
    /// does next follows from the idempotency the IR declared.
    Uncertain { detail: String },
}

/// Performs the work the decision core asks for.
#[async_trait]
pub trait Executor: Send + Sync {
    /// Runs one node.
    async fn run_node(&self, request: NodeRequest<'_>) -> NodeOutcome;

    /// Performs one already-claimed effect.
    async fn perform_effect(&self, request: EffectRequest<'_>) -> EffectOutcome;
}
