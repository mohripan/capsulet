use capsulet_application::execution::WorkerTickOutcome;

#[test]
fn worker_tick_outcome_has_stable_metric_names() {
    assert_eq!(
        WorkerTickOutcome::NoRunAvailable.as_str(),
        "no_run_available"
    );
    assert_eq!(WorkerTickOutcome::RunSucceeded.as_str(), "run_succeeded");
    assert_eq!(
        WorkerTickOutcome::RunRetryScheduled.as_str(),
        "run_retry_scheduled"
    );
}
