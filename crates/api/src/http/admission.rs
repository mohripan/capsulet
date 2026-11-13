//! Admission control: whether a pool or the workflow queue takes more work, and the record of each decision.

use capsulet_observability::{self as observability};
use capsulet_storage::ObjectStore;

use crate::{error::ApiError, state::AppState, store::ApiStore};

pub(super) async fn enforce_job_run_admission<S, O>(
    state: &AppState<S, O>,
    execution_pool: &str,
) -> Result<(), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let config = state.admission;
    if config.max_queued_runs.is_none() && config.max_queued_runs_per_pool.is_none() {
        record_admission_decision("job_run", "accepted");
        return Ok(());
    }

    let snapshot = state
        .store
        .admission_snapshot(execution_pool)
        .await
        .map_err(|error| ApiError::AdmissionUnavailable(error.to_string()))?;
    if let Some(limit) = config.max_queued_runs
        && snapshot.queued_runs >= limit
    {
        record_admission_decision("job_run", "rejected");
        return Err(ApiError::QueueOverloaded(format!(
            "queued job runs {} reached global limit {limit}",
            snapshot.queued_runs
        )));
    }
    if let Some(limit) = config.max_queued_runs_per_pool
        && snapshot.queued_runs_in_pool >= limit
    {
        record_admission_decision("job_run", "rejected");
        return Err(ApiError::QueueOverloaded(format!(
            "queued job runs in pool {execution_pool} {} reached pool limit {limit}",
            snapshot.queued_runs_in_pool
        )));
    }

    record_admission_decision("job_run", "accepted");
    Ok(())
}

pub(super) async fn enforce_workflow_admission<S, O>(
    state: &AppState<S, O>,
    source: &str,
) -> Result<(), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let Some(limit) = state.admission.max_queued_workflow_runs else {
        record_admission_decision("workflow_run", "accepted");
        return Ok(());
    };

    let snapshot = state
        .store
        .admission_snapshot("")
        .await
        .map_err(|error| ApiError::AdmissionUnavailable(error.to_string()))?;
    if snapshot.queued_workflow_runs >= limit {
        record_admission_decision("workflow_run", "rejected");
        return Err(ApiError::QueueOverloaded(format!(
            "queued workflow runs from {source} {} reached limit {limit}",
            snapshot.queued_workflow_runs
        )));
    }

    record_admission_decision("workflow_run", "accepted");
    Ok(())
}

pub(super) fn record_admission_decision(kind: &'static str, decision: &'static str) {
    observability::metrics::counter!(
        "capsulet_admission_decisions_total",
        "kind" => kind,
        "decision" => decision,
    )
    .increment(1);
}
