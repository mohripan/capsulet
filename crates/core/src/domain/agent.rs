use std::fmt::{self, Display};

use super::{AgentId, GraphDefinition, GraphError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentBudget {
    steps: u32,
    tokens: u64,
    seconds: u64,
    cost_micros: u64,
}

impl AgentBudget {
    /// Creates a required budget envelope for an agent run.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::InvalidBudget`] when any limit is zero.
    pub const fn new(
        max_steps: u32,
        max_tokens: u64,
        max_seconds: u64,
        max_cost_micros: u64,
    ) -> Result<Self, GraphError> {
        if max_steps == 0 || max_tokens == 0 || max_seconds == 0 || max_cost_micros == 0 {
            return Err(GraphError::InvalidBudget);
        }
        Ok(Self {
            steps: max_steps,
            tokens: max_tokens,
            seconds: max_seconds,
            cost_micros: max_cost_micros,
        })
    }

    #[must_use]
    pub const fn max_steps(&self) -> u32 {
        self.steps
    }

    #[must_use]
    pub const fn max_tokens(&self) -> u64 {
        self.tokens
    }

    #[must_use]
    pub const fn max_seconds(&self) -> u64 {
        self.seconds
    }

    #[must_use]
    pub const fn max_cost_micros(&self) -> u64 {
        self.cost_micros
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTerminationPolicy {
    conditions: Vec<TerminationCondition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminationCondition {
    ValidatorPass,
    SafetyFailure,
    NoProgress,
    HumanEscalation,
}

impl Display for TerminationCondition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ValidatorPass => "validator_pass",
            Self::SafetyFailure => "safety_failure",
            Self::NoProgress => "no_progress",
            Self::HumanEscalation => "human_escalation",
        })
    }
}

impl AgentTerminationPolicy {
    #[must_use]
    pub fn new(conditions: Vec<TerminationCondition>) -> Self {
        Self { conditions }
    }

    #[must_use]
    pub fn default_rag() -> Self {
        Self {
            conditions: vec![
                TerminationCondition::ValidatorPass,
                TerminationCondition::SafetyFailure,
                TerminationCondition::NoProgress,
                TerminationCondition::HumanEscalation,
            ],
        }
    }

    #[must_use]
    pub fn accept_on_validator_pass(&self) -> bool {
        self.conditions
            .contains(&TerminationCondition::ValidatorPass)
    }

    #[must_use]
    pub fn stop_on_safety_failure(&self) -> bool {
        self.conditions
            .contains(&TerminationCondition::SafetyFailure)
    }

    #[must_use]
    pub fn stop_on_no_progress(&self) -> bool {
        self.conditions.contains(&TerminationCondition::NoProgress)
    }

    #[must_use]
    pub fn allow_human_escalation(&self) -> bool {
        self.conditions
            .contains(&TerminationCondition::HumanEscalation)
    }

    #[must_use]
    pub fn conditions(&self) -> &[TerminationCondition] {
        &self.conditions
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Stopped,
}

impl Display for AgentRunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Stopped => "stopped",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentStateSnapshot {
    version: u64,
    state_json: String,
}

impl AgentStateSnapshot {
    #[must_use]
    pub fn new(version: u64, state_json: impl Into<String>) -> Self {
        Self {
            version,
            state_json: state_json.into(),
        }
    }

    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    #[must_use]
    pub fn state_json(&self) -> &str {
        &self.state_json
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTraceEvent {
    sequence: u64,
    event_type: String,
    payload_json: String,
}

impl AgentTraceEvent {
    #[must_use]
    pub fn new(
        sequence: u64,
        event_type: impl Into<String>,
        payload_json: impl Into<String>,
    ) -> Self {
        Self {
            sequence,
            event_type: event_type.into(),
            payload_json: payload_json.into(),
        }
    }

    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub fn event_type(&self) -> &str {
        &self.event_type
    }

    #[must_use]
    pub fn payload_json(&self) -> &str {
        &self.payload_json
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDefinition {
    id: AgentId,
    name: String,
    graph: GraphDefinition,
    budget: AgentBudget,
    termination_policy: AgentTerminationPolicy,
}

impl AgentDefinition {
    /// Builds an agent definition from a validated typed graph.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError`] when the required budget or termination policy is absent.
    pub fn new(
        id: AgentId,
        name: impl Into<String>,
        graph: GraphDefinition,
        budget: Option<AgentBudget>,
        termination_policy: Option<AgentTerminationPolicy>,
    ) -> Result<Self, GraphError> {
        let budget = budget.ok_or(GraphError::MissingBudgetPolicy)?;
        let termination_policy = termination_policy.ok_or(GraphError::MissingTerminationPolicy)?;
        Ok(Self {
            id,
            name: name.into(),
            graph,
            budget,
            termination_policy,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &AgentId {
        &self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn graph(&self) -> &GraphDefinition {
        &self.graph
    }

    #[must_use]
    pub const fn budget(&self) -> &AgentBudget {
        &self.budget
    }

    #[must_use]
    pub const fn termination_policy(&self) -> &AgentTerminationPolicy {
        &self.termination_policy
    }
}
