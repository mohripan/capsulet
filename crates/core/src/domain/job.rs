use std::fmt::{self, Display};
use std::str::FromStr;

use super::{ExecutionPoolName, JobDefinitionId, JobRunId, ParseDomainValueError};

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

impl FromStr for JobRunStatus {
    type Err = ParseDomainValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queued" => Ok(Self::Queued),
            "leased" => Ok(Self::Leased),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "timed_out" => Ok(Self::TimedOut),
            "retry_scheduled" => Ok(Self::RetryScheduled),
            value => Err(ParseDomainValueError::new("job run status", value)),
        }
    }
}

/// Intent that drives a job run state change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobRunTransition {
    Lease,
    StartAttempt,
    Succeed,
    Fail,
    Cancel,
    TimeOut,
    ScheduleRetry,
    Requeue,
}

impl JobRunTransition {
    const fn target(self) -> JobRunStatus {
        match self {
            Self::Lease => JobRunStatus::Leased,
            Self::StartAttempt => JobRunStatus::Running,
            Self::Succeed => JobRunStatus::Succeeded,
            Self::Fail => JobRunStatus::Failed,
            Self::Cancel => JobRunStatus::Cancelled,
            Self::TimeOut => JobRunStatus::TimedOut,
            Self::ScheduleRetry => JobRunStatus::RetryScheduled,
            Self::Requeue => JobRunStatus::Queued,
        }
    }
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
    fn transition_to(&mut self, next: JobRunStatus) -> Result<(), StateTransitionError> {
        if is_allowed_transition(self.status, next) {
            self.status = next;
            return Ok(());
        }

        Err(StateTransitionError {
            from: self.status,
            to: next,
        })
    }

    /// Applies a domain transition to this job run.
    ///
    /// # Errors
    ///
    /// Returns [`StateTransitionError`] when the transition is invalid for the current state.
    pub fn apply(&mut self, transition: JobRunTransition) -> Result<(), StateTransitionError> {
        self.transition_to(transition.target())?;
        if transition == JobRunTransition::StartAttempt {
            self.attempt_count += 1;
        }
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
    use super::{
        ExecutionPoolName, JobDefinitionId, JobRun, JobRunId, JobRunStatus, JobRunTransition,
    };

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

        run.apply(JobRunTransition::Lease)
            .expect("queued to leased");
        run.apply(JobRunTransition::StartAttempt)
            .expect("leased to running");
        run.apply(JobRunTransition::Succeed)
            .expect("running to succeeded");

        assert_eq!(run.status(), JobRunStatus::Succeeded);
        assert_eq!(run.attempt_count(), 1);
        assert!(run.status().is_terminal());
    }

    #[test]
    fn rejects_invalid_run_lifecycle() {
        let mut run = run();

        let error = run
            .apply(JobRunTransition::Succeed)
            .expect_err("queued cannot go directly to succeeded");

        assert_eq!(error.from(), JobRunStatus::Queued);
        assert_eq!(error.to(), JobRunStatus::Succeeded);
    }

    #[test]
    fn allows_cancellation_from_non_terminal_states() {
        let mut queued = run();
        queued
            .apply(JobRunTransition::Cancel)
            .expect("queued to cancelled");
        assert!(queued.status().is_terminal());

        let mut leased = run();
        leased
            .apply(JobRunTransition::Lease)
            .expect("queued to leased");
        leased
            .apply(JobRunTransition::Cancel)
            .expect("leased to cancelled");
        assert!(leased.status().is_terminal());

        let mut running = run();
        running
            .apply(JobRunTransition::Lease)
            .expect("queued to leased");
        running
            .apply(JobRunTransition::StartAttempt)
            .expect("leased to running");
        running
            .apply(JobRunTransition::Cancel)
            .expect("running to cancelled");
        assert!(running.status().is_terminal());
    }

    #[test]
    fn terminal_states_are_stable() {
        for (transition, terminal) in [
            (JobRunTransition::Succeed, JobRunStatus::Succeeded),
            (JobRunTransition::Fail, JobRunStatus::Failed),
            (JobRunTransition::Cancel, JobRunStatus::Cancelled),
            (JobRunTransition::TimeOut, JobRunStatus::TimedOut),
        ] {
            let mut run = run();
            run.apply(JobRunTransition::Lease)
                .expect("queued to leased");
            run.apply(JobRunTransition::StartAttempt)
                .expect("leased to running");
            run.apply(transition).expect("terminal transition");

            let error = run
                .apply(JobRunTransition::Requeue)
                .expect_err("terminal state cannot be overwritten");
            assert_eq!(error.from(), terminal);
        }
    }

    #[test]
    fn supports_retry_scheduling_after_failure_or_timeout() {
        for retryable in [JobRunTransition::Fail, JobRunTransition::TimeOut] {
            let mut run = run();
            run.apply(JobRunTransition::Lease)
                .expect("queued to leased");
            run.apply(JobRunTransition::StartAttempt)
                .expect("leased to running");
            run.apply(retryable).expect("retryable terminal");
            run.apply(JobRunTransition::ScheduleRetry)
                .expect("schedule retry");
            run.apply(JobRunTransition::Requeue)
                .expect("retry back to queue");
        }
    }
}
