use capsulet_core::{AgentDefinition, AgentId, AgentRunId, AgentRunStatus};

use crate::commands::StartAgentRunCommand;
use crate::ports::AgentRepository;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunRecord {
    pub id: AgentRunId,
    pub agent_id: AgentId,
    pub status: AgentRunStatus,
    pub state_version: u64,
    pub state_json: String,
    /// How the durable runtime is executing this run. `None` for a run from before
    /// agent runs executed, which has no log to read.
    pub execution: Option<AgentRunExecution>,
}

/// Where a run's execution stands, as read from its log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunExecution {
    /// The IR run doing the work.
    pub ir_run_id: String,
    /// The exact definition version being executed, by digest.
    pub definition_digest: String,
    /// Why the run ended where it did, when it did not simply finish.
    pub detail: Option<String>,
    /// What the run is suspended on, when it is.
    pub waiting_on: Option<String>,
    /// When what the run was given and produced was erased, if it was.
    pub payloads_erased_at: Option<String>,
    pub usage: AgentRunUsage,
}

/// What a run has consumed, summed from its log.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AgentRunUsage {
    pub tokens: u64,
    pub cost_micro_units: u64,
    pub effects: u32,
    /// How many times something consumed resources nobody measured. Those are not
    /// in the totals above, and a total that cannot see them should not be read as
    /// complete.
    pub unmeasured: u32,
}

pub struct AgentService<'a, R> {
    repository: &'a R,
}

impl<'a, R> AgentService<'a, R>
where
    R: AgentRepository + Sync,
{
    #[must_use]
    pub const fn new(repository: &'a R) -> Self {
        Self { repository }
    }

    /// Stores a validated agent definition.
    ///
    /// # Errors
    ///
    /// Returns the repository error when persistence fails.
    pub async fn create_agent(&self, agent: &AgentDefinition) -> Result<(), R::Error> {
        self.repository.save_agent(agent).await
    }

    /// Starts an agent run in the queued state with state version zero.
    ///
    /// # Errors
    ///
    /// Returns the repository error when persistence fails.
    pub async fn start_run(&self, command: StartAgentRunCommand) -> Result<(), R::Error> {
        let record = AgentRunRecord {
            id: command.run_id,
            agent_id: command.agent_id,
            status: AgentRunStatus::Queued,
            state_version: 0,
            state_json: command.initial_state_json,
            execution: None,
        };
        self.repository.save_run(&record).await
    }
}
