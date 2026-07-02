use async_trait::async_trait;
use capsulet_core::{
    AgentDefinition, AgentId, AgentRunId, AgentRunStatus, AgentTraceEvent, GraphNode, NodeId,
    NodeKind,
};
use serde_json::json;
use thiserror::Error;

use crate::agents::AgentRunRecord;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTraceRecord {
    pub run_id: AgentRunId,
    pub sequence: u64,
    pub event_type: String,
    pub payload_json: String,
}

impl AgentTraceRecord {
    #[must_use]
    pub fn new(run_id: AgentRunId, event: &AgentTraceEvent) -> Self {
        Self {
            run_id,
            sequence: event.sequence(),
            event_type: event.event_type().to_string(),
            payload_json: event.payload_json().to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentNodeExecution {
    pub run_id: AgentRunId,
    pub agent_id: AgentId,
    pub node_id: NodeId,
    pub node_kind: NodeKind,
    pub state_version: u64,
    pub state_json: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStopReason {
    ValidatorPass,
    SafetyFailure,
    NoProgress,
    HumanEscalation,
}

impl AgentStopReason {
    const fn as_str(self) -> &'static str {
        match self {
            Self::ValidatorPass => "validator_pass",
            Self::SafetyFailure => "safety_failure",
            Self::NoProgress => "no_progress",
            Self::HumanEscalation => "human_escalation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentNodeOutcome {
    pub state_json: String,
    pub tokens_used: u64,
    pub cost_micros: u64,
    pub stop_reason: Option<AgentStopReason>,
}

impl AgentNodeOutcome {
    #[must_use]
    pub fn continue_with_state(
        state_json: impl Into<String>,
        tokens_used: u64,
        cost_micros: u64,
    ) -> Self {
        Self {
            state_json: state_json.into(),
            tokens_used,
            cost_micros,
            stop_reason: None,
        }
    }

    #[must_use]
    pub fn stop_with_state(
        state_json: impl Into<String>,
        reason: AgentStopReason,
        tokens_used: u64,
        cost_micros: u64,
    ) -> Self {
        Self {
            state_json: state_json.into(),
            tokens_used,
            cost_micros,
            stop_reason: Some(reason),
        }
    }
}

#[async_trait]
pub trait AgentNodeExecutor {
    type Error;

    async fn execute(&self, execution: AgentNodeExecution)
    -> Result<AgentNodeOutcome, Self::Error>;
}

#[async_trait]
pub trait AgentRuntimeRepository {
    type Error;

    async fn save_agent_run(&self, run: &AgentRunRecord) -> Result<(), Self::Error>;
    async fn append_trace_event(&self, event: &AgentTraceRecord) -> Result<(), Self::Error>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AgentRuntimeError {
    #[error("agent run {run_id} belongs to agent {actual_agent_id}, not {expected_agent_id}")]
    AgentMismatch {
        run_id: AgentRunId,
        expected_agent_id: AgentId,
        actual_agent_id: AgentId,
    },
    #[error("agent run {run_id} is already terminal: {status}")]
    TerminalRun {
        run_id: AgentRunId,
        status: AgentRunStatus,
    },
    #[error("repository error: {0}")]
    Repository(String),
    #[error("node executor error at node {node_id}: {message}")]
    NodeExecutor { node_id: NodeId, message: String },
}

pub struct AgentRuntime<'a, R, X> {
    repository: &'a R,
    executor: &'a X,
}

#[derive(Debug, Default)]
struct RuntimeCounters {
    sequence: u64,
    steps: u32,
    tokens: u64,
    cost_micros: u64,
}

impl<'a, R, X> AgentRuntime<'a, R, X>
where
    R: AgentRuntimeRepository + Sync,
    R::Error: ToString,
    X: AgentNodeExecutor + Sync,
    X::Error: ToString,
{
    #[must_use]
    pub const fn new(repository: &'a R, executor: &'a X) -> Self {
        Self {
            repository,
            executor,
        }
    }

    /// Executes one agent run through the graph's current static order.
    ///
    /// # Errors
    ///
    /// Returns [`AgentRuntimeError`] when the run does not belong to the agent,
    /// when the run is already terminal, or when persistence fails.
    pub async fn execute_run(
        &self,
        agent: &AgentDefinition,
        mut run: AgentRunRecord,
    ) -> Result<AgentRunRecord, AgentRuntimeError> {
        validate_run(agent, &run)?;
        let mut counters = RuntimeCounters::default();
        run.status = AgentRunStatus::Running;
        self.save_run(&run).await?;

        for node_id in agent.graph().static_order() {
            if self
                .stop_if_step_budget_exhausted(agent, &mut run, &counters)
                .await?
            {
                return Ok(run);
            }
            let Some(node) = agent
                .graph()
                .nodes()
                .iter()
                .find(|node| node.id() == node_id)
            else {
                continue;
            };
            let should_stop = self
                .execute_node(agent, node, &mut run, &mut counters)
                .await?;
            if should_stop
                || self
                    .stop_if_budget_exhausted(agent, &mut run, &counters)
                    .await?
            {
                return Ok(run);
            }
            self.save_run(&run).await?;
        }

        run.status = AgentRunStatus::Succeeded;
        self.append_trace(
            &run,
            counters.sequence,
            "run_succeeded",
            json!({
                "state_version": run.state_version,
                "steps": counters.steps,
                "tokens_used": counters.tokens,
                "cost_micros": counters.cost_micros,
            }),
        )
        .await?;
        self.save_run(&run).await?;
        Ok(run)
    }

    async fn stop_if_step_budget_exhausted(
        &self,
        agent: &AgentDefinition,
        run: &mut AgentRunRecord,
        counters: &RuntimeCounters,
    ) -> Result<bool, AgentRuntimeError> {
        if counters.steps < agent.budget().max_steps() {
            return Ok(false);
        }
        run.status = AgentRunStatus::Stopped;
        self.trace_budget_stop(run, counters.sequence, "max_steps")
            .await?;
        self.save_run(run).await?;
        Ok(true)
    }

    async fn execute_node(
        &self,
        agent: &AgentDefinition,
        node: &GraphNode,
        run: &mut AgentRunRecord,
        counters: &mut RuntimeCounters,
    ) -> Result<bool, AgentRuntimeError> {
        self.trace_node_started(run, counters.sequence, node)
            .await?;
        counters.sequence += 1;

        let outcome = match self.executor.execute(node_execution(run, node)).await {
            Ok(outcome) => outcome,
            Err(error) => {
                run.status = AgentRunStatus::Failed;
                self.trace_node_failed(run, counters.sequence, node, error)
                    .await?;
                self.save_run(run).await?;
                return Ok(true);
            }
        };
        counters.steps += 1;
        counters.tokens = counters.tokens.saturating_add(outcome.tokens_used);
        counters.cost_micros = counters.cost_micros.saturating_add(outcome.cost_micros);
        run.state_version += 1;
        self.trace_node_completed(run, counters.sequence, node, &outcome)
            .await?;
        run.state_json = outcome.state_json;
        counters.sequence += 1;
        if let Some(reason) = outcome.stop_reason {
            run.status = status_for_stop_reason(agent, reason);
            self.trace_run_stopped(run, counters.sequence, reason)
                .await?;
            self.save_run(run).await?;
            return Ok(true);
        }
        Ok(false)
    }

    async fn stop_if_budget_exhausted(
        &self,
        agent: &AgentDefinition,
        run: &mut AgentRunRecord,
        counters: &RuntimeCounters,
    ) -> Result<bool, AgentRuntimeError> {
        let exhausted = if counters.tokens >= agent.budget().max_tokens() {
            Some("max_tokens")
        } else if counters.cost_micros >= agent.budget().max_cost_micros() {
            Some("max_cost_micros")
        } else {
            None
        };
        let Some(budget) = exhausted else {
            return Ok(false);
        };
        run.status = AgentRunStatus::Stopped;
        self.trace_budget_stop(run, counters.sequence, budget)
            .await?;
        self.save_run(run).await?;
        Ok(true)
    }

    async fn save_run(&self, run: &AgentRunRecord) -> Result<(), AgentRuntimeError> {
        self.repository
            .save_agent_run(run)
            .await
            .map_err(|error| AgentRuntimeError::Repository(error.to_string()))
    }

    async fn trace_budget_stop(
        &self,
        run: &AgentRunRecord,
        sequence: u64,
        budget: &str,
    ) -> Result<(), AgentRuntimeError> {
        self.append_trace(
            run,
            sequence,
            "budget_exhausted",
            json!({
                "budget": budget,
                "state_version": run.state_version,
            }),
        )
        .await
    }

    async fn append_trace(
        &self,
        run: &AgentRunRecord,
        sequence: u64,
        event_type: &str,
        payload: serde_json::Value,
    ) -> Result<(), AgentRuntimeError> {
        let event = AgentTraceEvent::new(sequence, event_type, payload.to_string());
        self.repository
            .append_trace_event(&AgentTraceRecord::new(run.id.clone(), &event))
            .await
            .map_err(|error| AgentRuntimeError::Repository(error.to_string()))
    }

    async fn trace_node_started(
        &self,
        run: &AgentRunRecord,
        sequence: u64,
        node: &GraphNode,
    ) -> Result<(), AgentRuntimeError> {
        self.append_trace(
            run,
            sequence,
            "node_started",
            json!({
                "node_id": node.id().as_str(),
                "node_kind": node.kind().to_string(),
                "state_version": run.state_version,
            }),
        )
        .await
    }

    async fn trace_node_completed(
        &self,
        run: &AgentRunRecord,
        sequence: u64,
        node: &GraphNode,
        outcome: &AgentNodeOutcome,
    ) -> Result<(), AgentRuntimeError> {
        self.append_trace(
            run,
            sequence,
            "node_completed",
            json!({
                "node_id": node.id().as_str(),
                "state_version": run.state_version,
                "tokens_used": outcome.tokens_used,
                "cost_micros": outcome.cost_micros,
            }),
        )
        .await
    }

    async fn trace_node_failed(
        &self,
        run: &AgentRunRecord,
        sequence: u64,
        node: &GraphNode,
        error: X::Error,
    ) -> Result<(), AgentRuntimeError> {
        self.append_trace(
            run,
            sequence,
            "node_failed",
            json!({
                "node_id": node.id().as_str(),
                "message": error.to_string(),
            }),
        )
        .await
    }

    async fn trace_run_stopped(
        &self,
        run: &AgentRunRecord,
        sequence: u64,
        reason: AgentStopReason,
    ) -> Result<(), AgentRuntimeError> {
        self.append_trace(
            run,
            sequence,
            "run_stopped",
            json!({
                "reason": reason.as_str(),
                "state_version": run.state_version,
            }),
        )
        .await
    }
}

fn validate_run(agent: &AgentDefinition, run: &AgentRunRecord) -> Result<(), AgentRuntimeError> {
    if run.agent_id != *agent.id() {
        return Err(AgentRuntimeError::AgentMismatch {
            run_id: run.id.clone(),
            expected_agent_id: agent.id().clone(),
            actual_agent_id: run.agent_id.clone(),
        });
    }
    if is_terminal(run.status) {
        return Err(AgentRuntimeError::TerminalRun {
            run_id: run.id.clone(),
            status: run.status,
        });
    }
    Ok(())
}

fn node_execution(run: &AgentRunRecord, node: &GraphNode) -> AgentNodeExecution {
    AgentNodeExecution {
        run_id: run.id.clone(),
        agent_id: run.agent_id.clone(),
        node_id: node.id().clone(),
        node_kind: node.kind(),
        state_version: run.state_version,
        state_json: run.state_json.clone(),
    }
}

const fn is_terminal(status: AgentRunStatus) -> bool {
    matches!(
        status,
        AgentRunStatus::Succeeded
            | AgentRunStatus::Failed
            | AgentRunStatus::Cancelled
            | AgentRunStatus::Stopped
    )
}

fn status_for_stop_reason(agent: &AgentDefinition, reason: AgentStopReason) -> AgentRunStatus {
    match reason {
        AgentStopReason::ValidatorPass if agent.termination_policy().accept_on_validator_pass() => {
            AgentRunStatus::Succeeded
        }
        AgentStopReason::SafetyFailure if agent.termination_policy().stop_on_safety_failure() => {
            AgentRunStatus::Stopped
        }
        AgentStopReason::NoProgress if agent.termination_policy().stop_on_no_progress() => {
            AgentRunStatus::Stopped
        }
        AgentStopReason::HumanEscalation if agent.termination_policy().allow_human_escalation() => {
            AgentRunStatus::Stopped
        }
        _ => AgentRunStatus::Running,
    }
}
