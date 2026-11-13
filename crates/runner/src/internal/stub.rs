//! A deterministic runner, for tests and smoke tests.

use async_trait::async_trait;
use thiserror::Error;

use super::contract::{
    CancellationCheck, CollectedArtifact, RunExecution, RunOutcome, RunReport, Runner,
};

/// Deterministic runner used for tests and local failure simulation.
#[derive(Debug, Clone)]
pub struct StubRunner {
    outcome: RunOutcome,
    logs: Option<String>,
    artifact: Option<String>,
}

impl StubRunner {
    /// Creates a stub runner that always succeeds.
    #[must_use]
    pub const fn success() -> Self {
        Self {
            outcome: RunOutcome::Succeeded,
            logs: None,
            artifact: None,
        }
    }

    /// Creates a stub runner that always fails.
    #[must_use]
    pub const fn failure() -> Self {
        Self {
            outcome: RunOutcome::Failed,
            logs: None,
            artifact: None,
        }
    }

    /// Creates a successful stub runner that emits one text artifact.
    #[must_use]
    pub fn success_with_artifact(text: impl Into<String>) -> Self {
        Self {
            outcome: RunOutcome::Succeeded,
            logs: None,
            artifact: Some(text.into()),
        }
    }

    /// Creates a successful stub runner with deterministic logs.
    #[must_use]
    pub fn success_with_logs(logs: impl Into<String>) -> Self {
        Self {
            outcome: RunOutcome::Succeeded,
            logs: Some(logs.into()),
            artifact: None,
        }
    }

    /// Creates a successful stub runner with deterministic logs and one text artifact.
    #[must_use]
    pub fn success_with_logs_and_artifact(
        logs: impl Into<String>,
        artifact: impl Into<String>,
    ) -> Self {
        Self {
            outcome: RunOutcome::Succeeded,
            logs: Some(logs.into()),
            artifact: Some(artifact.into()),
        }
    }
}

#[async_trait]
impl Runner for StubRunner {
    type Error = StubRunnerError;

    async fn execute<C>(
        &self,
        execution: &RunExecution,
        cancellation: &C,
    ) -> Result<RunReport, Self::Error>
    where
        C: CancellationCheck + Sync,
    {
        if cancellation
            .is_cancelled(execution.run.id())
            .await
            .map_err(|_| StubRunnerError::CancellationCheck)?
        {
            return Ok(RunReport::cancelled(None));
        }
        let artifact = self.artifact.clone().map(|text| CollectedArtifact {
            name: "stub-artifact.txt".to_string(),
            content_type: "text/plain".to_string(),
            bytes: text.into_bytes(),
        });
        let report = match self.outcome {
            RunOutcome::Succeeded => RunReport::succeeded_with_artifacts(
                self.logs.clone(),
                artifact.into_iter().collect(),
            ),
            RunOutcome::Failed => RunReport::failed(self.logs.clone()),
            RunOutcome::TimedOut => RunReport::timed_out(self.logs.clone()),
            RunOutcome::Cancelled => RunReport::cancelled(self.logs.clone()),
        };
        Ok(report)
    }
}

/// Error type for [`StubRunner`].
#[derive(Debug, Error)]
pub enum StubRunnerError {
    #[error("cancellation check failed")]
    CancellationCheck,
}
