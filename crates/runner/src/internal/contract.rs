//! What a runner is held to: a run's execution and inputs, its report, and cancellation.

use async_trait::async_trait;
use capsulet_core::{JobDefinition, JobRun, JobRunId};
use thiserror::Error;

use super::pools::ExecutionPoolConfig;

/// Execution result returned by a runner backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
}

/// Runner output for one execution attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    pub outcome: RunOutcome,
    pub logs: Option<String>,
    pub artifacts: Vec<CollectedArtifact>,
}

impl RunReport {
    #[must_use]
    pub fn succeeded(logs: Option<String>) -> Self {
        Self {
            outcome: RunOutcome::Succeeded,
            logs,
            artifacts: Vec::new(),
        }
    }

    #[must_use]
    pub fn succeeded_with_artifacts(
        logs: Option<String>,
        artifacts: Vec<CollectedArtifact>,
    ) -> Self {
        Self {
            outcome: RunOutcome::Succeeded,
            logs,
            artifacts,
        }
    }

    #[must_use]
    pub fn failed(logs: Option<String>) -> Self {
        Self {
            outcome: RunOutcome::Failed,
            logs,
            artifacts: Vec::new(),
        }
    }

    #[must_use]
    pub fn timed_out(logs: Option<String>) -> Self {
        Self {
            outcome: RunOutcome::TimedOut,
            logs,
            artifacts: Vec::new(),
        }
    }

    #[must_use]
    pub fn cancelled(logs: Option<String>) -> Self {
        Self {
            outcome: RunOutcome::Cancelled,
            logs,
            artifacts: Vec::new(),
        }
    }
}

/// Artifact bytes collected by a runner before persistence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectedArtifact {
    pub name: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// Artifact produced by a successful prerequisite and staged for this run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputArtifact {
    pub producer_step_id: String,
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Complete execution input for a leased job run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunExecution {
    pub run: JobRun,
    pub definition: JobDefinition,
    pub pool: ExecutionPoolConfig,
    pub input_artifacts: Vec<InputArtifact>,
}

/// Boundary for executing a leased job run.
#[async_trait]
pub trait Runner: Clone + Send + Sync + 'static {
    type Error: std::fmt::Display + Send + Sync + 'static;

    #[must_use]
    fn supports_reattachment(&self) -> bool {
        false
    }

    /// Executes a leased job run.
    ///
    /// # Errors
    ///
    /// Returns an implementation-specific error when the runner cannot produce
    /// an execution outcome.
    async fn execute<C>(
        &self,
        execution: &RunExecution,
        cancellation: &C,
    ) -> Result<RunReport, Self::Error>
    where
        C: CancellationCheck + Sync;
}

/// Read-side cancellation hook used while a runner is waiting for work.
#[async_trait]
pub trait CancellationCheck {
    type Error: std::fmt::Display + Send + Sync + 'static;

    async fn is_cancelled(&self, run_id: &JobRunId) -> Result<bool, Self::Error>;
}

/// Cancellation checker that never requests cancellation.
#[derive(Debug, Clone, Copy)]
pub struct NeverCancelled;

#[async_trait]
impl CancellationCheck for NeverCancelled {
    type Error = NeverCancelledError;

    async fn is_cancelled(&self, _run_id: &JobRunId) -> Result<bool, Self::Error> {
        Ok(false)
    }
}

#[derive(Debug, Error)]
pub enum NeverCancelledError {}
