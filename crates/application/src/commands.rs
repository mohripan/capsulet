use capsulet_core::{
    AgentId, AgentRunId, ExecutionPoolName, JobDefinitionId, JobRun, JobRunId, JobRunStatus,
};

/// Command side input for the first manual submission use case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateManualRunCommand {
    pub run_id: JobRunId,
    pub job_definition_id: JobDefinitionId,
    pub execution_pool: ExecutionPoolName,
    pub input_json: Option<String>,
}

impl CreateManualRunCommand {
    #[must_use]
    pub fn into_job_run(self) -> JobRun {
        let run = JobRun::new(self.run_id, self.job_definition_id, self.execution_pool);
        if let Some(input_json) = self.input_json {
            return JobRun::from_persisted(
                run.id().clone(),
                run.job_definition_id().clone(),
                run.execution_pool().clone(),
                input_json,
                run.status(),
                run.attempt_count(),
                run.created_at(),
            );
        }
        run
    }
}

/// Query side projection suitable for lists and API responses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRunSummary {
    pub id: JobRunId,
    pub status: JobRunStatus,
    pub execution_pool: ExecutionPoolName,
    pub attempt_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartAgentRunCommand {
    pub run_id: AgentRunId,
    pub agent_id: AgentId,
    pub initial_state_json: String,
}

impl From<&JobRun> for JobRunSummary {
    fn from(run: &JobRun) -> Self {
        Self {
            id: run.id().clone(),
            status: run.status(),
            execution_pool: run.execution_pool().clone(),
            attempt_count: run.attempt_count(),
        }
    }
}
