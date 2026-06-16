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
    from: JobRunStatus,
    to: JobRunStatus,
}

impl StateTransitionError {
    #[must_use]
    pub const fn from(&self) -> JobRunStatus {
        self.from
    }

    #[must_use]
    pub const fn to(&self) -> JobRunStatus {
        self.to
    }
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
    id: JobRunId,
    job_definition_id: JobDefinitionId,
    execution_pool: ExecutionPoolName,
    input_json: String,
    status: JobRunStatus,
    attempt_count: u32,
    created_at: String,
}

impl JobRun {
    #[must_use]
    pub fn new(
        id: JobRunId,
        job_definition_id: JobDefinitionId,
        execution_pool: ExecutionPoolName,
    ) -> Self {
        Self {
            id,
            job_definition_id,
            execution_pool,
            input_json: "{}".to_string(),
            status: JobRunStatus::Queued,
            attempt_count: 0,
            created_at: String::new(),
        }
    }

    #[must_use]
    pub fn from_persisted(
        id: JobRunId,
        job_definition_id: JobDefinitionId,
        execution_pool: ExecutionPoolName,
        input_json: impl Into<String>,
        status: JobRunStatus,
        attempt_count: u32,
        created_at: impl Into<String>,
    ) -> Self {
        Self {
            id,
            job_definition_id,
            execution_pool,
            input_json: input_json.into(),
            status,
            attempt_count,
            created_at: created_at.into(),
        }
    }

    #[must_use]
    pub const fn id(&self) -> &JobRunId {
        &self.id
    }

    #[must_use]
    pub const fn job_definition_id(&self) -> &JobDefinitionId {
        &self.job_definition_id
    }

    #[must_use]
    pub const fn execution_pool(&self) -> &ExecutionPoolName {
        &self.execution_pool
    }

    #[must_use]
    pub fn input_json(&self) -> &str {
        &self.input_json
    }

    #[must_use]
    pub const fn status(&self) -> JobRunStatus {
        self.status
    }

    #[must_use]
    pub const fn attempt_count(&self) -> u32 {
        self.attempt_count
    }

    #[must_use]
    pub fn created_at(&self) -> &str {
        &self.created_at
    }

    #[must_use]
    pub const fn with_status(mut self, status: JobRunStatus) -> Self {
        self.status = status;
        self
    }

    /// Attaches validated run input as JSON.
    ///
    /// # Errors
    ///
    /// Returns an error when the input is empty.
    pub fn with_input(mut self, input_json: impl Into<String>) -> Result<Self, String> {
        let input_json = input_json.into();
        if input_json.trim().is_empty() {
            return Err("job run input cannot be empty".to_string());
        }
        self.input_json = input_json;
        Ok(self)
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

        assert_eq!(run.status(), JobRunStatus::Queued);
        assert_eq!(run.attempt_count(), 0);
    }

    #[test]
    fn allows_valid_run_lifecycle() {
        let mut run = run();

        run.transition_to(JobRunStatus::Leased)
            .expect("queued to leased");
        run.record_attempt_started().expect("leased to running");
        run.transition_to(JobRunStatus::Succeeded)
            .expect("running to succeeded");

        assert_eq!(run.status(), JobRunStatus::Succeeded);
        assert_eq!(run.attempt_count(), 1);
        assert!(run.status().is_terminal());
    }

    #[test]
    fn rejects_invalid_run_lifecycle() {
        let mut run = run();

        let error = run
            .transition_to(JobRunStatus::Succeeded)
            .expect_err("queued cannot go directly to succeeded");

        assert_eq!(error.from(), JobRunStatus::Queued);
        assert_eq!(error.to(), JobRunStatus::Succeeded);
    }

    #[test]
    fn allows_cancellation_from_non_terminal_states() {
        let mut queued = run();
        queued
            .transition_to(JobRunStatus::Cancelled)
            .expect("queued to cancelled");
        assert!(queued.status().is_terminal());

        let mut leased = run();
        leased
            .transition_to(JobRunStatus::Leased)
            .expect("queued to leased");
        leased
            .transition_to(JobRunStatus::Cancelled)
            .expect("leased to cancelled");
        assert!(leased.status().is_terminal());

        let mut running = run();
        running
            .transition_to(JobRunStatus::Leased)
            .expect("queued to leased");
        running.record_attempt_started().expect("leased to running");
        running
            .transition_to(JobRunStatus::Cancelled)
            .expect("running to cancelled");
        assert!(running.status().is_terminal());
    }

    #[test]
    fn terminal_states_are_stable() {
        for terminal in [
            JobRunStatus::Succeeded,
            JobRunStatus::Failed,
            JobRunStatus::Cancelled,
            JobRunStatus::TimedOut,
        ] {
            let mut run = run();
            run.transition_to(JobRunStatus::Leased)
                .expect("queued to leased");
            run.record_attempt_started().expect("leased to running");
            run.transition_to(terminal).expect("terminal transition");

            let error = run
                .transition_to(JobRunStatus::Queued)
                .expect_err("terminal state cannot be overwritten");
            assert_eq!(error.from(), terminal);
        }
    }

    #[test]
    fn supports_retry_scheduling_after_failure_or_timeout() {
        for retryable in [JobRunStatus::Failed, JobRunStatus::TimedOut] {
            let mut run = run();
            run.transition_to(JobRunStatus::Leased)
                .expect("queued to leased");
            run.record_attempt_started().expect("leased to running");
            run.transition_to(retryable).expect("retryable terminal");
            run.transition_to(JobRunStatus::RetryScheduled)
                .expect("schedule retry");
            run.transition_to(JobRunStatus::Queued)
                .expect("retry back to queue");
        }
    }
}
