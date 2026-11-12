//! The names shared by the translation that builds an agent's definition and the
//! executor that runs it.
//!
//! Neither depends on the other, deliberately: the adapter describes and the worker
//! executes. What they have to agree on is a small closed vocabulary, and it lives
//! here so a disagreement is a compile error rather than a node nobody can run.

/// The boundary node an agent's loop enters through.
pub const AGENT_ENTER: &str = "agent-enter";
/// The boundary node an agent's loop leaves through.
pub const AGENT_LEAVE: &str = "agent-leave";
/// The node that decides whether an agent's loop goes round again.
pub const AGENT_CONTINUE: &str = "agent-continue";
/// The boolean port the loop reads its continuation from.
pub const KEEP_GOING: &str = "keep-going";
/// The integer port the loop reads its progress from.
pub const DISTINCT_RESULTS: &str = "distinct-results";
/// What `agent-continue` remembers from one iteration to the next.
pub const SEEN_RESULTS: &str = "seen-results";
/// The prefix on `agent-continue`'s ports that read a validator's result.
pub const CHECK_PORT_PREFIX: &str = "check.";
/// The prefix on `agent-continue`'s ports that read a written state field.
pub const STATE_PORT_PREFIX: &str = "state.";
/// The parameter naming what a node does, since IR kinds are coarser than the source
/// graph's: a planner, a retriever and a model are all proposers.
pub const KIND_PARAMETER: &str = "capsulet.kind";
/// The parameter telling `agent-continue` whether a failed check goes round again.
pub const RETRY_PARAMETER: &str = "capsulet.retry-on-failed-check";
/// The authority an agent's human gate names.
pub const HUMAN_REVIEWER: &str = "human-reviewer";
/// The control reading a validator's finish carries: whether its check passed.
///
/// A check's verdict is part of the run's record rather than its content, so the
/// log, and every bundle of it, says how each check came out without carrying what
/// it checked.
pub const VERDICT_PASSED: &str = "verdict.passed";
/// Present, and true, when the check passed because a person waived its failure at
/// a gate rather than because its rules held.
pub const VERDICT_WAIVED: &str = "verdict.waived";
