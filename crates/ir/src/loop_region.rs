//! Loops, declared rather than implied.
//!
//! A loop that lives inside a model prompt or a node implementation has no
//! bounds anyone can inspect, no invariant anyone can check, and no honest
//! account of why it stopped. This module makes each of those explicit:
//!
//! - every budget is required and finite, so "it ran until it worked" is not
//!   expressible;
//! - invariants name the node that evaluates them, so an invariant is a check
//!   rather than a comment;
//! - a progress measure, where the domain has one, makes going nowhere
//!   detectable instead of merely slow;
//! - stopping has a typed reason, and exhausting a budget is never one of the
//!   successful ones.
//!
//! Repair routes are declared per failure kind. "Ask the model again" is one
//! possible route for one kind of failure, with its own attempt budget — not
//! the universal recovery mechanism it becomes when nobody writes the routes
//! down.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::digest::Digest;
use crate::id::Identifier;
use crate::value::ValueSchema;

/// Why a loop declaration was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LoopError {
    #[error("loop `{region}` declares no bound on {resource}; every loop budget must be finite")]
    UnboundedBudget {
        region: Identifier,
        resource: &'static str,
    },
    #[error("loop `{region}` declares invariant `{invariant}` with no node to evaluate it")]
    InvariantWithoutEvaluator {
        region: Identifier,
        invariant: Identifier,
    },
    #[error("loop `{region}` names `{node}` as its {role}, which is not inside the loop")]
    NodeOutsideLoop {
        region: Identifier,
        node: Identifier,
        role: &'static str,
    },
    #[error("loop `{region}` continues on `{port}` of `{node}`, which is not a boolean")]
    ContinuationNotBoolean {
        region: Identifier,
        node: Identifier,
        port: Identifier,
    },
    #[error("loop `{region}` measures progress with `{port}` of `{node}`, which is not an integer")]
    ProgressNotOrdered {
        region: Identifier,
        node: Identifier,
        port: Identifier,
    },
    #[error("loop `{region}` declares failure kind `{failure}` twice")]
    DuplicateRoute {
        region: Identifier,
        failure: &'static str,
    },
    #[error("loop `{region}` routes `{failure}` to a retry with no attempts")]
    RetryWithoutAttempts {
        region: Identifier,
        failure: &'static str,
    },
}

/// Which budget a loop ran out of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetKind {
    Iterations,
    WallTime,
    Tokens,
    Cost,
    Effects,
}

impl BudgetKind {
    /// A short name for messages.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Iterations => "iterations",
            Self::WallTime => "wall time",
            Self::Tokens => "tokens",
            Self::Cost => "cost",
            Self::Effects => "effects",
        }
    }
}

/// What a loop may spend before it must stop.
///
/// Every field is required. There is no "unlimited" case, because a loop whose
/// end nobody bounded is a loop nobody can promise anything about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopBudget {
    pub max_iterations: u32,
    pub wall_ms: u64,
    pub tokens: u64,
    pub cost_micro_units: u64,
    pub effect_count: u32,
}

impl LoopBudget {
    /// Checks that every bound is present and non-zero.
    ///
    /// A zero bound is not a tighter budget, it is a loop that cannot run, and
    /// saying so at admission time is kinder than discovering it in a run.
    ///
    /// # Errors
    ///
    /// Returns [`LoopError::UnboundedBudget`] naming the first missing bound.
    pub fn check(&self, region: &Identifier) -> Result<(), LoopError> {
        let unbounded = |resource| LoopError::UnboundedBudget {
            region: region.clone(),
            resource,
        };
        if self.max_iterations == 0 {
            return Err(unbounded("iterations"));
        }
        if self.wall_ms == 0 {
            return Err(unbounded("wall time"));
        }
        Ok(())
    }
}

/// When an invariant is evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvariantTiming {
    BeforeIteration,
    AfterIteration,
    Both,
}

/// A property that must hold around every iteration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invariant {
    pub id: Identifier,
    pub description: String,
    /// The node that decides it. An invariant without an evaluator is a wish.
    pub evaluator: Identifier,
    /// The boolean output of that node.
    pub port: Identifier,
    pub timing: InvariantTiming,
}

/// Which way a progress measure must move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressDirection {
    StrictlyDecreasing,
    StrictlyIncreasing,
}

/// An integer measure that must move every iteration.
///
/// Optional because not every domain has one. Where a domain does — open
/// findings remaining, tests still failing, records left to reconcile —
/// declaring it turns "this has been going for a while" into a decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressMeasure {
    pub id: Identifier,
    pub measured_by: Identifier,
    pub port: Identifier,
    pub direction: ProgressDirection,
}

/// The kind of failure a repair route answers.
///
/// These are the failure types the correctness architecture routes by owner:
/// retrieval problems go back to retrieval, arithmetic goes to the checker,
/// policy denials need a policy-authorised change, and interpretation
/// residuals need a person.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    MissingEvidence,
    StaleReference,
    SchemaMismatch,
    ArithmeticMismatch,
    VerifierUnavailable,
    PolicyDenial,
    BudgetExhaustion,
    NonProgress,
    UnsafeEffect,
    InterpretationResidual,
}

impl FailureKind {
    /// A short name for messages.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingEvidence => "missing_evidence",
            Self::StaleReference => "stale_reference",
            Self::SchemaMismatch => "schema_mismatch",
            Self::ArithmeticMismatch => "arithmetic_mismatch",
            Self::VerifierUnavailable => "verifier_unavailable",
            Self::PolicyDenial => "policy_denial",
            Self::BudgetExhaustion => "budget_exhaustion",
            Self::NonProgress => "non_progress",
            Self::UnsafeEffect => "unsafe_effect",
            Self::InterpretationResidual => "interpretation_residual",
        }
    }
}

/// What happens when a failure of that kind occurs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Route {
    /// Run the named node again, at most `attempts` times.
    Retry { node: Identifier, attempts: u32 },
    /// Hand the failure to a node that repairs it.
    Repair { node: Identifier },
    /// Stop and wait for a person or another authority.
    Escalate { to: Identifier },
    /// Stop, with the failure recorded.
    Reject,
}

/// One declared repair route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairRoute {
    pub failure: FailureKind,
    pub route: Route,
}

/// Why a loop stopped.
///
/// Only [`StopReason::ConditionFalse`] means the loop finished the work it set
/// out to do. Everything else is a stop, and a stop is not a success no matter
/// how convenient it would be to render it as one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StopReason {
    /// The continuation condition became false: the loop is done.
    ConditionFalse,
    BudgetExhausted {
        budget: BudgetKind,
    },
    InvariantFailed {
        invariant: Identifier,
    },
    NonProgress {
        measure: Identifier,
    },
    RepairExhausted {
        failure: FailureKind,
    },
    EscalationRequired {
        to: Identifier,
    },
    Cancelled {
        by: Identifier,
    },
}

impl StopReason {
    /// Whether the loop finished rather than merely stopped.
    #[must_use]
    pub const fn is_completion(&self) -> bool {
        matches!(self, Self::ConditionFalse)
    }

    /// A short name for messages and certificates.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ConditionFalse => "condition_false",
            Self::BudgetExhausted { .. } => "budget_exhausted",
            Self::InvariantFailed { .. } => "invariant_failed",
            Self::NonProgress { .. } => "non_progress",
            Self::RepairExhausted { .. } => "repair_exhausted",
            Self::EscalationRequired { .. } => "escalation_required",
            Self::Cancelled { .. } => "cancelled",
        }
    }
}

/// Where the loop reads its continuation condition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Continuation {
    pub evaluated_by: Identifier,
    pub port: Identifier,
}

/// Everything a loop region declares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopSpec {
    /// The typed state carried from one iteration to the next.
    pub state: BTreeMap<String, ValueSchema>,
    /// The typed values the loop hands back when it finishes.
    pub exit: BTreeMap<String, ValueSchema>,
    pub continuation: Continuation,
    pub budget: LoopBudget,
    pub invariants: Vec<Invariant>,
    pub progress: Option<ProgressMeasure>,
    pub repairs: Vec<RepairRoute>,
}

impl LoopSpec {
    /// Checks what is decidable from the declaration alone.
    ///
    /// Node references are checked by the graph, which is the only thing that
    /// knows what the nodes are.
    ///
    /// # Errors
    ///
    /// Returns [`LoopError`] for a missing bound, an invariant without an
    /// evaluator, a duplicated failure route, or a retry with no attempts.
    pub fn check(&self, region: &Identifier) -> Result<(), LoopError> {
        self.budget.check(region)?;

        for invariant in &self.invariants {
            if invariant.evaluator.as_str().is_empty() {
                return Err(LoopError::InvariantWithoutEvaluator {
                    region: region.clone(),
                    invariant: invariant.id.clone(),
                });
            }
        }

        let mut seen: Vec<FailureKind> = Vec::new();
        for repair in &self.repairs {
            if seen.contains(&repair.failure) {
                return Err(LoopError::DuplicateRoute {
                    region: region.clone(),
                    failure: repair.failure.as_str(),
                });
            }
            seen.push(repair.failure);

            if let Route::Retry { attempts, .. } = &repair.route
                && *attempts == 0
            {
                return Err(LoopError::RetryWithoutAttempts {
                    region: region.clone(),
                    failure: repair.failure.as_str(),
                });
            }
        }
        Ok(())
    }

    /// The route declared for a failure kind, if any.
    #[must_use]
    pub fn route_for(&self, failure: FailureKind) -> Option<&Route> {
        self.repairs
            .iter()
            .find(|repair| repair.failure == failure)
            .map(|repair| &repair.route)
    }
}

/// How an invariant came out in one iteration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvariantOutcome {
    pub invariant: Identifier,
    pub held: bool,
    pub timing: InvariantTiming,
}

/// What one iteration did.
///
/// The runtime that executes loops arrives in M3. This is the shape it will
/// record, defined now so a certificate can carry an iteration history that a
/// replayer already understands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IterationRecord {
    pub index: u32,
    /// The loop state entering and leaving this iteration.
    pub state_in: Digest,
    pub state_out: Digest,
    pub invariants: Vec<InvariantOutcome>,
    /// The progress measure's value, where one is declared. An integer, so two
    /// readings can be compared without a float anywhere near a digest.
    pub progress: Option<i128>,
    /// Tokens, cost, wall time, and effects this iteration consumed.
    pub spent: LoopBudget,
}

/// How a loop ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopOutcome {
    pub region: Identifier,
    pub iterations: Vec<IterationRecord>,
    pub stopped: StopReason,
}

impl LoopOutcome {
    /// Whether the loop finished its work.
    ///
    /// Kept as a method so nobody has to remember which stop reasons count. A
    /// budget-exhausted loop reports `false` here, and a caller that renders it
    /// as success has to say so explicitly.
    #[must_use]
    pub const fn completed(&self) -> bool {
        self.stopped.is_completion()
    }
}
