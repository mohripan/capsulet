use async_trait::async_trait;
use capsulet_application::{CreateManualRunCommand, JobRunRepository, JobRunSummary};
use capsulet_core::{ExecutionPoolName, JobDefinitionId, JobRunId, JobRunStatus};

#[test]
fn manual_run_command_creates_queued_run() {
    let command = CreateManualRunCommand {
        run_id: JobRunId::new("run_1").expect("valid run id"),
        job_definition_id: JobDefinitionId::new("job_1").expect("valid job id"),
        execution_pool: ExecutionPoolName::new("mini").expect("valid pool"),
        input_json: None,
    };

    let run = command.into_job_run();
    let summary = JobRunSummary::from(&run);

    assert_eq!(summary.status, JobRunStatus::Queued);
}

fn assert_repository_port_is_exported<T: JobRunRepository>() {}

#[test]
fn job_run_repository_port_is_exported() {
    struct Marker;

    #[async_trait]
    impl JobRunRepository for Marker {
        type Error = std::convert::Infallible;

        async fn save(&self, _run: &capsulet_core::JobRun) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn find_by_id(
            &self,
            _id: &JobRunId,
        ) -> Result<Option<capsulet_core::JobRun>, Self::Error> {
            Ok(None)
        }
    }

    assert_repository_port_is_exported::<Marker>();
}
