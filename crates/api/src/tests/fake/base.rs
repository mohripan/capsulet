//! What every store has: its error, and whether it is up.

use super::super::*;

#[async_trait::async_trait]
impl StoreBase for FakeStore {
    type Error = String;

    fn is_scope_mismatch(error: &Self::Error) -> bool {
        error == "scope mismatch"
    }

    fn is_already_exists(error: &Self::Error) -> bool {
        error == "already exists"
    }

    async fn ping(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn admission_snapshot(
        &self,
        execution_pool: &str,
    ) -> Result<AdmissionSnapshot, Self::Error> {
        let runs = self.runs.lock().map_err(|error| error.to_string())?;
        let queued_runs = runs
            .iter()
            .filter(|run| run.status() == JobRunStatus::Queued)
            .count();
        let queued_runs_in_pool = runs
            .iter()
            .filter(|run| {
                run.status() == JobRunStatus::Queued
                    && run.execution_pool().as_str() == execution_pool
            })
            .count();
        drop(runs);
        let queued_workflow_runs = self
            .workflow_runs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|run| run.status() == WorkflowRunStatus::Queued)
            .count();

        Ok(AdmissionSnapshot {
            queued_runs: u64::try_from(queued_runs).map_err(|error| error.to_string())?,
            queued_runs_in_pool: u64::try_from(queued_runs_in_pool)
                .map_err(|error| error.to_string())?,
            queued_workflow_runs: u64::try_from(queued_workflow_runs)
                .map_err(|error| error.to_string())?,
        })
    }
}
