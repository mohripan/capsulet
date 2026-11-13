//! Agent definitions and agent runs: what a run did, why, what it cost, and what replaying it shows.

use capsulet_application::AgentRunRecord;
use capsulet_core::{AgentBudget, AgentDefinition, AgentTerminationPolicy, TerminationCondition};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::{error::ApiError, http::json_from_string};

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateAgentRequest {
    pub id: Option<String>,
    pub name: String,
    pub graph_id: String,
    pub budget: AgentBudgetRequest,
    #[serde(default)]
    pub termination_conditions: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AgentBudgetRequest {
    #[serde(rename = "max_steps")]
    pub steps: u32,
    #[serde(rename = "max_tokens")]
    pub tokens: u64,
    #[serde(rename = "max_seconds")]
    pub seconds: u64,
    #[serde(rename = "max_cost_micros")]
    pub cost_micros: u64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct StartAgentRunRequest {
    pub id: Option<String>,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub initial_state: Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ListAgentsResponse {
    pub(crate) agents: Vec<AgentResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ListAgentRunsResponse {
    pub(crate) agent_runs: Vec<AgentRunResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct AgentResponse {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) graph_id: String,
    pub(crate) budget: AgentBudgetResponse,
    pub(crate) termination_conditions: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct AgentBudgetResponse {
    #[serde(rename = "max_steps")]
    pub(crate) steps: u32,
    #[serde(rename = "max_tokens")]
    pub(crate) tokens: u64,
    #[serde(rename = "max_seconds")]
    pub(crate) seconds: u64,
    #[serde(rename = "max_cost_micros")]
    pub(crate) cost_micros: u64,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct AgentRunResponse {
    pub(crate) id: String,
    pub(crate) agent_id: String,
    /// Where the run is, as an execution concept. `stopped` means a budget, a lack of
    /// progress, a safety check or a person ended it. None of these, `succeeded`
    /// included, says the result is right; that is what `assurance` is for.
    pub(crate) status: String,
    /// How many nodes have finished.
    pub(crate) state_version: u64,
    #[schema(value_type = Object)]
    pub(crate) state: Value,
    /// Always `unverified` today: agent runs execute in `observe` mode, so nothing
    /// has checked their result. Never inferred from `status`.
    pub(crate) assurance: String,
    /// How the durable runtime is executing this run. Absent on a run from before
    /// agent runs executed.
    pub(crate) execution: Option<AgentRunExecutionResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct AgentRunExecutionResponse {
    /// The IR run doing the work, whose log is `GET /v1/agent-runs/{id}/events`.
    pub(crate) ir_run_id: String,
    /// The exact definition version being executed.
    pub(crate) definition_digest: String,
    /// Why the run ended where it did, when it did not simply finish.
    pub(crate) detail: Option<String>,
    /// What the run is suspended on, when it is.
    pub(crate) waiting_on: Option<String>,
    /// When what the run was given and produced was erased, if it was. An erased
    /// run's `state` is empty and its log still lists every event, by digest.
    pub(crate) payloads_erased_at: Option<String>,
    pub(crate) usage: AgentRunUsageResponse,
}

/// A run packed to be understood elsewhere, with nothing it was given or produced.
///
/// `body` is the bundle as JSON text and `digest` is the digest of exactly those
/// bytes, so a reader checks it before believing anything inside. `capsulet-replay`
/// reads one offline.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct AgentRunBundleResponse {
    /// Always `capsulet.agent_run_bundle`.
    pub(crate) kind: String,
    pub(crate) bundle_version: u32,
    pub(crate) digest: String,
    pub(crate) body: String,
}

/// What a run did, read from its own log the way `capsulet-replay` reads its
/// bundle.
///
/// It is made from the same bundle `exportAgentRunBundle` seals, by the same code,
/// so this account and an offline replay of the export cannot differ. Every label is
/// an observed fact, and none certifies a result: a check that held says only that
/// its rules held on its input.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct AgentRunExplanationResponse {
    pub(crate) run_id: String,
    pub(crate) agent_id: String,
    /// The trace the run's spans were recorded under, when it was traced.
    pub(crate) trace_id: Option<String>,
    /// `observe`, `verify` or `enforce`; absent when the log records no admission.
    pub(crate) assurance_mode: Option<String>,
    /// What the run's result amounts to: `unverified` (completed in observe mode,
    /// nothing certified it), `see_certificate` (completed under a mode that
    /// certifies), `abstained` (ended without a result) or `pending`.
    pub(crate) result: String,
    /// The status the log folds to; absent when it does not fold.
    pub(crate) status: Option<String>,
    /// Why the log does not fold, when it does not.
    pub(crate) fold_error: Option<String>,
    /// Whether the log supports what was reported and every decision in it is one
    /// the runtime makes again.
    pub(crate) agrees: bool,
    /// Where the log and what was reported disagree.
    pub(crate) disagreements: Vec<String>,
    /// Whether the definition is the version the run was admitted with.
    pub(crate) definition_matches: bool,
    pub(crate) decisions: DecisionReplayResponse,
    pub(crate) spent: AgentRunUsageResponse,
    pub(crate) ended_because: Option<String>,
    pub(crate) waiting_on: Option<String>,
    /// Nodes started and not finished when the log ends.
    pub(crate) unfinished: Vec<String>,
    pub(crate) nodes: Vec<NodeExplanationResponse>,
    pub(crate) effects: Vec<EffectExplanationResponse>,
}

/// Every decision in a run's log, made again from the log before it.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct DecisionReplayResponse {
    /// Events recording a decision the runtime makes again.
    pub(crate) replayed: u32,
    /// Events the outside world wrote: admission, inputs, answered signals.
    pub(crate) external: u32,
    /// Events a decision's own work wrote after it.
    pub(crate) consequences: u32,
    /// Events the runtime would not have written.
    pub(crate) divergences: Vec<DivergenceResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct DivergenceResponse {
    pub(crate) position: u64,
    /// The event the log records there.
    pub(crate) recorded: String,
    /// What deciding again from the log before it produces instead.
    pub(crate) decided: String,
}

/// One node's part in a run, and what its last attempt amounts to.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct NodeExplanationResponse {
    pub(crate) node: String,
    pub(crate) kind: String,
    pub(crate) attempts: u32,
    pub(crate) finished: u32,
    /// The kind of each failed attempt, in order.
    pub(crate) failures: Vec<String>,
    /// What doing the node's work again would need: `recorded in the log`, a
    /// model, the project's memory, the world, or only its inputs.
    pub(crate) rework: String,
    /// `proposed`, `checked`, `waived`, `disputed`, `verdict_unrecorded`,
    /// `unverified`, `failed`, `interrupted`, `not_run` or `structural`.
    pub(crate) result: String,
    /// What the result label does and does not say.
    pub(crate) meaning: String,
    /// A check's rules, as its definition states them: what `checked` or
    /// `disputed` is about.
    pub(crate) rules: Vec<CheckRuleResponse>,
    /// What the node was observed to do: its events, in order.
    pub(crate) events: Vec<ExplainedEventResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CheckRuleResponse {
    pub(crate) rule: String,
    pub(crate) value: String,
}

/// One event, with what the replay made of it and the evidence it names.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ExplainedEventResponse {
    pub(crate) position: u64,
    pub(crate) kind: String,
    /// `decision`, `external`, `consequence` or `divergent`.
    pub(crate) role: String,
    /// The content digests the event names: inputs, outputs, receipts.
    pub(crate) evidence: Vec<EvidenceDigestResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct EvidenceDigestResponse {
    pub(crate) name: String,
    pub(crate) digest: String,
}

/// One effect's attempts, and how the last one came out.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct EffectExplanationResponse {
    pub(crate) node: String,
    pub(crate) effect: String,
    pub(crate) attempts: u32,
    /// `performed`, `refused`, `uncertain`, or `outstanding`.
    pub(crate) outcome: String,
}

/// What erasing an agent run's payloads did.
#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct AgentRunErasureResponse {
    /// How many stored values this request erased. Zero when an earlier request
    /// already had, which makes asking again harmless.
    pub(crate) values_erased: u64,
    /// How many values the run held that another run of the project still holds.
    /// They are kept for that run, and go when it is erased too.
    pub(crate) values_kept: u64,
    /// The run as it reads now.
    pub(crate) run: AgentRunResponse,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct AgentRunUsageResponse {
    pub(crate) tokens: u64,
    pub(crate) cost_micro_units: u64,
    pub(crate) effects: u32,
    /// How many times something consumed resources nobody measured. Those are not in
    /// the totals, so a non-zero value means the totals are a lower bound.
    pub(crate) unmeasured: u32,
}

/// A person's answer at an agent run's gate.
#[derive(Debug, Deserialize, ToSchema)]
pub struct DecideAgentRunRequest {
    /// Whether to waive the failure that raised the gate, or end the run.
    pub approved: bool,
}

impl From<&AgentDefinition> for AgentResponse {
    fn from(agent: &AgentDefinition) -> Self {
        Self {
            id: agent.id().as_str().to_string(),
            name: agent.name().to_string(),
            graph_id: agent.graph().id().as_str().to_string(),
            budget: AgentBudgetResponse::from(agent.budget()),
            termination_conditions: agent
                .termination_policy()
                .conditions()
                .iter()
                .map(ToString::to_string)
                .collect(),
        }
    }
}

impl From<&AgentBudget> for AgentBudgetResponse {
    fn from(budget: &AgentBudget) -> Self {
        Self {
            steps: budget.max_steps(),
            tokens: budget.max_tokens(),
            seconds: budget.max_seconds(),
            cost_micros: budget.max_cost_micros(),
        }
    }
}

impl AgentRunResponse {
    pub(crate) fn new(run: &AgentRunRecord) -> Result<Self, ApiError> {
        Ok(Self {
            id: run.id.as_str().to_string(),
            agent_id: run.agent_id.as_str().to_string(),
            status: run.status.to_string(),
            state_version: run.state_version,
            state: json_from_string(&run.state_json)?,
            assurance: "unverified".to_string(),
            execution: run
                .execution
                .as_ref()
                .map(|execution| AgentRunExecutionResponse {
                    ir_run_id: execution.ir_run_id.clone(),
                    definition_digest: execution.definition_digest.clone(),
                    detail: execution.detail.clone(),
                    waiting_on: execution.waiting_on.clone(),
                    payloads_erased_at: execution.payloads_erased_at.clone(),
                    usage: AgentRunUsageResponse {
                        tokens: execution.usage.tokens,
                        cost_micro_units: execution.usage.cost_micro_units,
                        effects: execution.usage.effects,
                        unmeasured: execution.usage.unmeasured,
                    },
                }),
        })
    }
}

impl AgentBudgetRequest {
    pub(crate) fn into_budget(self) -> Result<AgentBudget, ApiError> {
        AgentBudget::new(self.steps, self.tokens, self.seconds, self.cost_micros)
            .map_err(|error| ApiError::Validation(error.to_string()))
    }
}

pub(crate) fn termination_policy_from_conditions(
    conditions: Vec<String>,
) -> Result<AgentTerminationPolicy, ApiError> {
    if conditions.is_empty() {
        return Ok(AgentTerminationPolicy::default_rag());
    }
    Ok(AgentTerminationPolicy::new(
        conditions
            .into_iter()
            .map(|condition| parse_termination_condition(&condition))
            .collect::<Result<Vec<_>, _>>()?,
    ))
}

pub(super) fn parse_termination_condition(value: &str) -> Result<TerminationCondition, ApiError> {
    match value {
        "validator_pass" => Ok(TerminationCondition::ValidatorPass),
        "safety_failure" => Ok(TerminationCondition::SafetyFailure),
        "no_progress" => Ok(TerminationCondition::NoProgress),
        "human_escalation" => Ok(TerminationCondition::HumanEscalation),
        value => Err(ApiError::Validation(format!(
            "unknown termination condition {value}"
        ))),
    }
}
