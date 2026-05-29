use crate::domain::{ExecutionPoolName, JobDefinitionId, JobRun, JobRunId, JobRunStatus};

/// Command side input for the first manual submission use case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateManualRunCommand {
    pub run_id: JobRunId,
    pub job_definition_id: JobDefinitionId,
    pub execution_pool: ExecutionPoolName,
}

impl CreateManualRunCommand {
    #[must_use]
    pub fn into_job_run(self) -> JobRun {
        JobRun::new(self.run_id, self.job_definition_id, self.execution_pool)
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

impl From<&JobRun> for JobRunSummary {
    fn from(run: &JobRun) -> Self {
        Self {
            id: run.id.clone(),
            status: run.status,
            execution_pool: run.execution_pool.clone(),
            attempt_count: run.attempt_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CreateManualRunCommand, JobRunSummary};
    use crate::domain::{ExecutionPoolName, JobDefinitionId, JobRunId, JobRunStatus};

    #[test]
    fn manual_run_command_creates_queued_run() {
        let command = CreateManualRunCommand {
            run_id: JobRunId::new("run_1").expect("valid run id"),
            job_definition_id: JobDefinitionId::new("job_1").expect("valid job id"),
            execution_pool: ExecutionPoolName::new("mini").expect("valid pool"),
        };

        let run = command.into_job_run();
        let summary = JobRunSummary::from(&run);

        assert_eq!(summary.status, JobRunStatus::Queued);
        assert_eq!(summary.execution_pool.as_str(), "mini");
    }
}
