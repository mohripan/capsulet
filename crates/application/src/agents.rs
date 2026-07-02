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
        };
        self.repository.save_run(&record).await
    }
}
