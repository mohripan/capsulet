use std::fmt::{self, Display};

use super::{ExecutionPoolName, JobDefinitionId, JobRunId};

/// Durable state of a job run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobRunStatus {
    Queued,
    Leased,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
    RetryScheduled,
}

impl JobRunStatus {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::TimedOut
        )
    }
}

impl Display for JobRunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Queued => "queued",
            Self::Leased => "leased",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
            Self::RetryScheduled => "retry_scheduled",
        };
        f.write_str(value)
    }
}

/// Invalid state transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateTransitionError {
    pub from: JobRunStatus,
    pub to: JobRunStatus,
}

impl Display for StateTransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "cannot transition job run from {} to {}",
            self.from, self.to
        )
    }
}

impl std::error::Error for StateTransitionError {}

/// Job run aggregate root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRun {
    pub id: JobRunId,
    pub job_definition_id: JobDefinitionId,
    pub execution_pool: ExecutionPoolName,
    pub status: JobRunStatus,
    pub attempt_count: u32,
}

impl JobRun {
    #[must_use]
    pub const fn new(
        id: JobRunId,
        job_definition_id: JobDefinitionId,
        execution_pool: ExecutionPoolName,
    ) -> Self {
        Self {
            id,
            job_definition_id,
            execution_pool,
            status: JobRunStatus::Queued,
            attempt_count: 0,
        }
    }

    /// Applies a valid job run state transition.
    ///
    /// # Errors
    ///
    /// Returns [`StateTransitionError`] when the transition is not allowed by
    /// the job run state machine.
    pub fn transition_to(&mut self, next: JobRunStatus) -> Result<(), StateTransitionError> {
        if is_allowed_transition(self.status, next) {
            self.status = next;
            return Ok(());
        }

        Err(StateTransitionError {
            from: self.status,
            to: next,
        })
    }

    /// Records that a new execution attempt has started.
    ///
    /// # Errors
    ///
    /// Returns [`StateTransitionError`] when the run cannot move to `running`.
    pub fn record_attempt_started(&mut self) -> Result<(), StateTransitionError> {
        self.transition_to(JobRunStatus::Running)?;
        self.attempt_count += 1;
        Ok(())
    }
}

const fn is_allowed_transition(from: JobRunStatus, to: JobRunStatus) -> bool {
    matches!(
        (from, to),
        (
            JobRunStatus::Queued,
            JobRunStatus::Leased | JobRunStatus::Cancelled
        ) | (
            JobRunStatus::Leased,
            JobRunStatus::Running | JobRunStatus::Cancelled,
        ) | (
            JobRunStatus::Running,
            JobRunStatus::Succeeded
                | JobRunStatus::Failed
                | JobRunStatus::TimedOut
                | JobRunStatus::Cancelled,
        ) | (
            JobRunStatus::Failed | JobRunStatus::TimedOut,
            JobRunStatus::RetryScheduled,
        ) | (JobRunStatus::RetryScheduled, JobRunStatus::Queued)
    )
}

#[cfg(test)]
mod tests {
    use super::{ExecutionPoolName, JobDefinitionId, JobRun, JobRunId, JobRunStatus};

    fn run() -> JobRun {
        JobRun::new(
            JobRunId::new("run_1").expect("valid run id"),
            JobDefinitionId::new("job_send_email").expect("valid job definition id"),
            ExecutionPoolName::new("mini").expect("valid pool"),
        )
    }

    #[test]
    fn job_run_starts_queued() {
        let run = run();

        assert_eq!(run.status, JobRunStatus::Queued);
        assert_eq!(run.attempt_count, 0);
    }

    #[test]
    fn allows_valid_run_lifecycle() {
        let mut run = run();

        run.transition_to(JobRunStatus::Leased)
            .expect("queued to leased");
        run.record_attempt_started().expect("leased to running");
        run.transition_to(JobRunStatus::Succeeded)
            .expect("running to succeeded");

        assert_eq!(run.status, JobRunStatus::Succeeded);
        assert_eq!(run.attempt_count, 1);
        assert!(run.status.is_terminal());
    }

    #[test]
    fn rejects_invalid_run_lifecycle() {
        let mut run = run();

        let error = run
            .transition_to(JobRunStatus::Succeeded)
            .expect_err("queued cannot go directly to succeeded");

        assert_eq!(error.from, JobRunStatus::Queued);
        assert_eq!(error.to, JobRunStatus::Succeeded);
    }
}
