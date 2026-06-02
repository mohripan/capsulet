use async_trait::async_trait;
use capsulet_core::JobRun;
use thiserror::Error;

/// Execution result returned by a runner backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    Succeeded,
    Failed,
}

/// Boundary for executing a leased job run.
#[async_trait]
pub trait Runner: Clone + Send + Sync + 'static {
    type Error: std::fmt::Display + Send + Sync + 'static;

    /// Executes a leased job run.
    ///
    /// # Errors
    ///
    /// Returns an implementation-specific error when the runner cannot produce
    /// an execution outcome.
    async fn execute(&self, run: &JobRun) -> Result<RunOutcome, Self::Error>;
}

/// Deterministic runner used before Kubernetes Job execution exists.
#[derive(Debug, Clone, Copy)]
pub struct StubRunner {
    outcome: RunOutcome,
}

impl StubRunner {
    /// Creates a stub runner that always succeeds.
    #[must_use]
    pub const fn success() -> Self {
        Self {
            outcome: RunOutcome::Succeeded,
        }
    }

    /// Creates a stub runner that always fails.
    #[must_use]
    pub const fn failure() -> Self {
        Self {
            outcome: RunOutcome::Failed,
        }
    }
}

#[async_trait]
impl Runner for StubRunner {
    type Error = StubRunnerError;

    async fn execute(&self, _run: &JobRun) -> Result<RunOutcome, Self::Error> {
        Ok(self.outcome)
    }
}

/// Error type for [`StubRunner`].
#[derive(Debug, Error)]
pub enum StubRunnerError {}

#[cfg(test)]
mod tests {
    use capsulet_core::{ExecutionPoolName, JobDefinitionId, JobRun, JobRunId};

    use super::{RunOutcome, Runner, StubRunner};

    fn run() -> JobRun {
        JobRun::new(
            JobRunId::new("run_1").expect("valid run id"),
            JobDefinitionId::new("job_hello_python").expect("valid definition id"),
            ExecutionPoolName::new("mini").expect("valid pool"),
        )
    }

    #[tokio::test]
    async fn stub_success_runner_returns_success() {
        let outcome = StubRunner::success()
            .execute(&run())
            .await
            .expect("stub outcome");

        assert_eq!(outcome, RunOutcome::Succeeded);
    }

    #[tokio::test]
    async fn stub_failure_runner_returns_failure() {
        let outcome = StubRunner::failure()
            .execute(&run())
            .await
            .expect("stub outcome");

        assert_eq!(outcome, RunOutcome::Failed);
    }
}
