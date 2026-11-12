//! A run, packed to be understood somewhere else.
//!
//! A bundle carries what somebody needs to see how a run went without access to the
//! installation it ran on: the definition it executed, its whole log, and what the
//! installation reported about it. It deliberately leaves out what the run was given
//! and produced. The log names those by digest only, so a bundle can go to whoever is
//! debugging without handing them the content.
//!
//! The body travels as text, with a digest over exactly those bytes. A reader checks
//! the digest before believing anything inside, and nothing depends on two JSON
//! encoders agreeing about whitespace or key order.
//!
//! [`explain`] is what reading one is for. It folds the log again, which needs nothing
//! but the bundle, and says how far that goes: the decisions replay, the node work
//! does not, and for each node it says what the work would have needed. Each
//! decision is made again from the log before it ([`crate::replay`]), so a log that
//! the runtime would not have written disagrees even when it folds.

use std::collections::BTreeMap;

use capsulet_ir::agent::{
    AGENT_CONTINUE, AGENT_ENTER, AGENT_LEAVE, KIND_PARAMETER, VERDICT_PASSED, VERDICT_WAIVED,
};
use capsulet_ir::definition::AssuranceMode;
use capsulet_ir::definition::Definition;
use capsulet_ir::digest::Digest;
use capsulet_ir::id::Identifier;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::event::{ControlValue, RecordedEvent, RunEvent};
use crate::replay::{DecisionReplay, replay_decisions};
use crate::state::{RunState, RunStatus, Spent};

/// What a sealed bundle says it is.
pub const RUN_BUNDLE_KIND: &str = "capsulet.agent_run_bundle";
/// The bundle format this code writes and reads.
pub const RUN_BUNDLE_VERSION: u32 = 1;

/// A bundle as it travels: its body, and a digest over the body's bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedRunBundle {
    pub kind: String,
    pub bundle_version: u32,
    pub digest: String,
    /// A [`RunBundle`], as JSON text.
    pub body: String,
}

/// Everything a bundle says about one run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunBundle {
    pub run_id: String,
    pub agent_id: String,
    pub tenant_id: String,
    pub project_id: String,
    /// What the installation reported, to be checked against the log.
    pub reported: ReportedRun,
    /// The definition version the run executed.
    pub definition: Definition,
    /// The run's whole log, in order.
    pub events: Vec<RecordedEvent>,
    /// The trace the run's spans were recorded under, when it was traced.
    pub trace_id: Option<String>,
    /// Whether what the run was given and produced had been erased when this was
    /// packed. The log is the same either way.
    pub payloads_erased: bool,
    /// What was deliberately left out, in words.
    pub excluded: Vec<String>,
}

/// What the installation said about a run when it was packed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportedRun {
    /// The run's status as the durable runtime names it: `queued`, `running`,
    /// `waiting`, `completed`, `failed` or `cancelled`.
    pub status: String,
    pub tokens: u64,
    pub cost_micro_units: u64,
    pub effects: u32,
    pub unmeasured: u32,
}

/// Why a bundle could not be read, or could not be believed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BundleError {
    #[error("this is not a run bundle: it says it is `{0}`")]
    WrongKind(String),
    #[error("bundle version {0} is not one this reader knows")]
    UnsupportedVersion(u32),
    #[error("the body does not match its digest: it says {recorded}, the bytes are {actual}")]
    DigestMismatch { recorded: String, actual: String },
    #[error("the body could not be read: {0}")]
    Body(String),
}

impl RunBundle {
    /// Seals the bundle: its body as text, and the digest of that text.
    ///
    /// # Errors
    ///
    /// Returns [`BundleError::Body`] when the bundle cannot be encoded.
    pub fn seal(&self) -> Result<SealedRunBundle, BundleError> {
        let body =
            serde_json::to_string(self).map_err(|error| BundleError::Body(error.to_string()))?;
        Ok(SealedRunBundle {
            kind: RUN_BUNDLE_KIND.to_string(),
            bundle_version: RUN_BUNDLE_VERSION,
            digest: Digest::of(body.as_bytes()).to_string(),
            body,
        })
    }
}

impl SealedRunBundle {
    /// Opens the bundle, after checking it is what it says and that its body is the
    /// one its digest names.
    ///
    /// # Errors
    ///
    /// Returns [`BundleError`] when the kind, version or digest is wrong, or the
    /// body does not parse. Nothing inside is returned unless all of it checks.
    pub fn open(&self) -> Result<RunBundle, BundleError> {
        if self.kind != RUN_BUNDLE_KIND {
            return Err(BundleError::WrongKind(self.kind.clone()));
        }
        if self.bundle_version != RUN_BUNDLE_VERSION {
            return Err(BundleError::UnsupportedVersion(self.bundle_version));
        }
        let actual = Digest::of(self.body.as_bytes()).to_string();
        if actual != self.digest {
            return Err(BundleError::DigestMismatch {
                recorded: self.digest.clone(),
                actual,
            });
        }
        serde_json::from_str(&self.body).map_err(|error| BundleError::Body(error.to_string()))
    }
}

/// What a node's work would need to be done again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rework {
    /// Not work at all: a boundary or the loop's own decision, which the log
    /// records and replays.
    Recorded,
    /// A pure function of its inputs. Doing it again needs the input values,
    /// which a bundle does not carry.
    FromInputs,
    /// A model call, which can answer differently every time it is asked.
    Model,
    /// The project's memory as it stood at the time.
    Memory,
    /// Something outside: a job, or a claim proposed to the project.
    Effect,
    /// A kind this reader does not know.
    Unknown,
}

impl Rework {
    /// A short phrase for a report.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Recorded => "recorded in the log",
            Self::FromInputs => "deterministic, given its inputs",
            Self::Model => "needs the model",
            Self::Memory => "needs the project's memory",
            Self::Effect => "acted on the world",
            Self::Unknown => "unknown kind",
        }
    }
}

/// What a node's last attempt amounts to, from what the log records of it.
///
/// Every label is an observed fact. None says a result is correct: a check that
/// held says only that its rules held on its input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeResult {
    /// A model's answer: a proposal that nothing has certified.
    Proposed,
    /// A check whose rules held on its input.
    Checked,
    /// A check whose rules did not hold, which passed because a person waived
    /// the failure at a gate.
    Waived,
    /// A check whose rules did not hold.
    Disputed,
    /// A check that finished without recording its verdict: a log written
    /// before checks recorded one.
    VerdictUnrecorded,
    /// Work that finished and that nothing checked: a deterministic step, a
    /// retrieval, an effect.
    Unverified,
    /// The last attempt failed.
    Failed,
    /// Started, and not finished when the log ends.
    Interrupted,
    /// Never started.
    NotRun,
    /// The agent loop's own structure, which decides rather than produces.
    Structural,
}

impl NodeResult {
    /// The label, for a report or a wire format.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Checked => "checked",
            Self::Waived => "waived",
            Self::Disputed => "disputed",
            Self::VerdictUnrecorded => "verdict_unrecorded",
            Self::Unverified => "unverified",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
            Self::NotRun => "not_run",
            Self::Structural => "structural",
        }
    }

    /// What the label does and does not say.
    #[must_use]
    pub const fn meaning(self) -> &'static str {
        match self {
            Self::Proposed => "a model's answer, which nothing has certified",
            Self::Checked => {
                "its rules held on its input; that is all a check says, not that the input is right"
            }
            Self::Waived => "its rules did not hold, and a person waived the failure at a gate",
            Self::Disputed => "its rules did not hold on its input",
            Self::VerdictUnrecorded => "finished, in a log from before checks recorded a verdict",
            Self::Unverified => "finished, and nothing checked what it produced",
            Self::Failed => "its last attempt failed",
            Self::Interrupted => "started, and not finished when the log ends",
            Self::NotRun => "never started",
            Self::Structural => "the agent loop's own step",
        }
    }
}

/// What the run's result amounts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunResult {
    /// It completed in observe mode: its outputs were captured and nothing
    /// certified them, whatever its checks found.
    Unverified,
    /// It completed in a mode that issues a certificate. The certificate says
    /// what the result amounts to, and a run bundle does not carry it.
    SeeCertificate,
    /// It ended without a result: it failed or was cancelled.
    Abstained,
    /// It has not ended.
    Pending,
}

impl RunResult {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unverified => "unverified",
            Self::SeeCertificate => "see_certificate",
            Self::Abstained => "abstained",
            Self::Pending => "pending",
        }
    }
}

/// One node's part in the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeAccount {
    pub node: Identifier,
    pub kind: String,
    pub attempts: u32,
    pub finished: u32,
    /// The kind of each failed attempt, in order.
    pub failures: Vec<String>,
    pub rework: Rework,
    /// What its last attempt amounts to.
    pub result: NodeResult,
    /// A check's rules, as its definition states them (`non_empty`, `true`): what
    /// `Checked` or `Disputed` is about.
    pub rules: Vec<(String, String)>,
}

/// One effect's attempts, and how the last one came out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectAccount {
    pub node: Identifier,
    pub effect: Identifier,
    pub attempts: u32,
    /// `performed`, `refused`, `uncertain`, or `outstanding` when it was claimed
    /// and nothing resolved it yet.
    pub outcome: &'static str,
}

/// What folding a bundle's log again says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Explanation {
    /// The status the log folds to, or why it does not fold.
    pub status: Result<RunStatus, String>,
    /// Where the log and what was reported disagree. Empty when they agree.
    pub disagreements: Vec<String>,
    /// Whether the bundle's definition is the one the run was admitted with.
    pub definition_matches: bool,
    pub spent: Spent,
    pub nodes: Vec<NodeAccount>,
    pub effects: Vec<EffectAccount>,
    /// Nodes started and not finished when the log ends: what an interrupted run
    /// was doing.
    pub unfinished: Vec<Identifier>,
    pub ended_because: Option<String>,
    pub waiting_on: Option<String>,
    /// Every decision in the log, made again from the log before it.
    pub decisions: DecisionReplay,
    /// The mode the run was admitted in.
    pub assurance: Option<AssuranceMode>,
    /// What the run's result amounts to.
    pub result: RunResult,
}

impl Explanation {
    /// Whether the log supports everything the installation reported, and
    /// records only decisions the runtime makes again.
    #[must_use]
    pub fn agrees(&self) -> bool {
        self.status.is_ok()
            && self.definition_matches
            && self.disagreements.is_empty()
            && self.decisions.agrees()
    }
}

/// Folds a bundle's log again and accounts for what it shows.
///
/// Needs nothing but the bundle: no clock, no store, no model.
#[must_use]
pub fn explain(bundle: &RunBundle) -> Explanation {
    let admitted = bundle
        .events
        .iter()
        .find_map(|recorded| match &recorded.event {
            RunEvent::Admitted { definition, .. } => Some(*definition),
            _ => None,
        });
    let definition_matches =
        admitted.is_some() && capsulet_ir::digest_of(&bundle.definition).ok() == admitted;

    let assurance = bundle
        .events
        .iter()
        .find_map(|recorded| match &recorded.event {
            RunEvent::Admitted { mode, .. } => Some(*mode),
            _ => None,
        });
    let folded = RunState::fold(&bundle.events).map_err(|error| error.to_string());
    let mut disagreements = Vec::new();
    let mut spent = Spent::default();
    let mut unfinished = Vec::new();
    let mut ended_because = None;
    let mut waiting_on = None;
    if let Ok(state) = &folded {
        spent = state.spent();
        if state.status().as_str() != bundle.reported.status {
            disagreements.push(format!(
                "reported `{}`, and the log folds to `{}`",
                bundle.reported.status,
                state.status().as_str()
            ));
        }
        let reported = [
            ("tokens", bundle.reported.tokens, spent.tokens),
            (
                "cost",
                bundle.reported.cost_micro_units,
                spent.cost_micro_units,
            ),
            (
                "effects",
                u64::from(bundle.reported.effects),
                u64::from(spent.effects),
            ),
            (
                "unmeasured",
                u64::from(bundle.reported.unmeasured),
                u64::from(spent.unmeasured),
            ),
        ];
        for (what, said, found) in reported {
            if said != found {
                disagreements.push(format!(
                    "reported {what} {said}, and the log sums to {found}"
                ));
            }
        }
        unfinished = state.running().keys().cloned().collect();
        ended_because = state.failure().map(crate::event::RunFailure::describe);
        waiting_on = state.waiting_on().map(crate::event::Wait::describe);
    }

    let folded_status = folded.as_ref().ok().map(RunState::status);
    Explanation {
        status: folded.map(|state| state.status()),
        disagreements,
        definition_matches,
        spent,
        nodes: node_accounts(bundle),
        effects: effect_accounts(&bundle.events),
        unfinished,
        ended_because,
        waiting_on,
        decisions: replay_decisions(&bundle.definition, &bundle.events),
        assurance,
        result: run_result(folded_status, assurance),
    }
}

fn run_result(status: Option<RunStatus>, assurance: Option<AssuranceMode>) -> RunResult {
    match status {
        Some(RunStatus::Completed) => match assurance {
            Some(AssuranceMode::Observe) | None => RunResult::Unverified,
            Some(AssuranceMode::Verify | AssuranceMode::Enforce) => RunResult::SeeCertificate,
        },
        Some(RunStatus::Failed | RunStatus::Cancelled) => RunResult::Abstained,
        Some(RunStatus::Queued | RunStatus::Running | RunStatus::Waiting) | None => {
            RunResult::Pending
        }
    }
}

fn node_accounts(bundle: &RunBundle) -> Vec<NodeAccount> {
    let mut accounts: BTreeMap<Identifier, NodeAccount> = bundle
        .definition
        .graph
        .nodes()
        .map(|node| {
            let kind = node
                .parameters
                .get(KIND_PARAMETER)
                .cloned()
                .unwrap_or_else(|| "none".to_string());
            let rework = rework_of(&kind);
            let rules = node
                .parameters
                .iter()
                .filter_map(|(name, value)| {
                    name.strip_prefix("rule.")
                        .map(|rule| (rule.to_string(), value.clone()))
                })
                .collect();
            (
                node.id.clone(),
                NodeAccount {
                    node: node.id.clone(),
                    kind,
                    attempts: 0,
                    finished: 0,
                    failures: Vec::new(),
                    rework,
                    result: NodeResult::NotRun,
                    rules,
                },
            )
        })
        .collect();
    // What each node's last attempt came to, as the log records it.
    let mut last: BTreeMap<Identifier, Last> = BTreeMap::new();
    for recorded in &bundle.events {
        match &recorded.event {
            RunEvent::NodeStarted { node } => {
                if let Some(account) = accounts.get_mut(node) {
                    account.attempts = account.attempts.saturating_add(1);
                }
                last.insert(node.clone(), Last::Started);
            }
            RunEvent::NodeFinished { node, control, .. } => {
                if let Some(account) = accounts.get_mut(node) {
                    account.finished = account.finished.saturating_add(1);
                }
                let reading = |name: &str| match control.get(name) {
                    Some(ControlValue::Bool { value }) => Some(*value),
                    _ => None,
                };
                last.insert(
                    node.clone(),
                    Last::Finished {
                        passed: reading(VERDICT_PASSED),
                        waived: reading(VERDICT_WAIVED) == Some(true),
                    },
                );
            }
            RunEvent::NodeFailed { node, failure, .. } => {
                if let Some(account) = accounts.get_mut(node) {
                    account.failures.push(failure.as_str().to_string());
                }
                last.insert(node.clone(), Last::Failed);
            }
            _ => {}
        }
    }
    for account in accounts.values_mut() {
        account.result = node_result(account, last.get(&account.node).copied());
    }
    accounts.into_values().collect()
}

/// A node's last attempt, as the log records it.
#[derive(Debug, Clone, Copy)]
enum Last {
    Started,
    Finished { passed: Option<bool>, waived: bool },
    Failed,
}

fn node_result(account: &NodeAccount, last: Option<Last>) -> NodeResult {
    if account.rework == Rework::Recorded {
        return NodeResult::Structural;
    }
    match last {
        None => NodeResult::NotRun,
        Some(Last::Started) => NodeResult::Interrupted,
        Some(Last::Failed) => NodeResult::Failed,
        Some(Last::Finished { passed, waived }) if account.kind == "validator" => match passed {
            Some(true) if waived => NodeResult::Waived,
            Some(true) => NodeResult::Checked,
            Some(false) => NodeResult::Disputed,
            None => NodeResult::VerdictUnrecorded,
        },
        Some(Last::Finished { .. }) if account.rework == Rework::Model => NodeResult::Proposed,
        Some(Last::Finished { .. }) => NodeResult::Unverified,
    }
}

/// The node an event is about, when it is about one.
#[must_use]
pub fn node_of(event: &RunEvent) -> Option<&Identifier> {
    match event {
        RunEvent::NodeStarted { node }
        | RunEvent::NodeFinished { node, .. }
        | RunEvent::NodeFailed { node, .. }
        | RunEvent::ResourcesConsumed { node, .. }
        | RunEvent::EffectClaimed { node, .. }
        | RunEvent::EffectFinalized { node, .. }
        | RunEvent::EffectAbandoned { node, .. }
        | RunEvent::EffectUncertain { node, .. }
        | RunEvent::GateRejected { node, .. }
        | RunEvent::Compensated { node, .. } => Some(node),
        _ => None,
    }
}

/// The evidence an event names by digest: the definition the run was admitted
/// against, its inputs, a node's or the run's outputs, an effect's receipt.
#[must_use]
pub fn evidence_of(event: &RunEvent) -> Vec<(String, Digest)> {
    match event {
        RunEvent::Admitted { definition, .. } => vec![("definition".to_string(), *definition)],
        RunEvent::InputsBound { inputs } => inputs
            .iter()
            .map(|(name, digest)| (format!("input {name}"), *digest))
            .collect(),
        RunEvent::NodeFinished { outputs, .. } | RunEvent::Completed { outputs } => outputs
            .iter()
            .map(|(name, digest)| (format!("output {name}"), *digest))
            .collect(),
        RunEvent::EffectFinalized { receipt, .. } | RunEvent::Compensated { receipt, .. } => {
            vec![("receipt".to_string(), *receipt)]
        }
        _ => Vec::new(),
    }
}

fn rework_of(kind: &str) -> Rework {
    match kind {
        AGENT_ENTER | AGENT_LEAVE | AGENT_CONTINUE => Rework::Recorded,
        "query_normalizer" | "prompt_builder" | "return" | "reranker" | "validator" => {
            Rework::FromInputs
        }
        "llm" | "planner" | "embedding" => Rework::Model,
        "retriever" | "memory_read" => Rework::Memory,
        "job" | "memory_write" => Rework::Effect,
        _ => Rework::Unknown,
    }
}

fn effect_accounts(events: &[RecordedEvent]) -> Vec<EffectAccount> {
    let mut accounts: BTreeMap<(Identifier, Identifier), EffectAccount> = BTreeMap::new();
    for recorded in events {
        let (node, effect, outcome) = match &recorded.event {
            RunEvent::EffectClaimed { node, effect, .. } => (node, effect, "outstanding"),
            RunEvent::EffectFinalized { node, effect, .. } => (node, effect, "performed"),
            RunEvent::EffectAbandoned { node, effect, .. } => (node, effect, "refused"),
            RunEvent::EffectUncertain { node, effect, .. } => (node, effect, "uncertain"),
            _ => continue,
        };
        let account = accounts
            .entry((node.clone(), effect.clone()))
            .or_insert_with(|| EffectAccount {
                node: node.clone(),
                effect: effect.clone(),
                attempts: 0,
                outcome,
            });
        if matches!(recorded.event, RunEvent::EffectClaimed { .. }) {
            account.attempts = account.attempts.saturating_add(1);
        }
        account.outcome = outcome;
    }
    accounts.into_values().collect()
}
