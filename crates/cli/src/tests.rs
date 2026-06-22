use clap::Parser;
use reqwest::Url;

use super::{
    ApiClient, ArtifactResponse, ArtifactsCommand, Cli, Command, JobRunResponse, RunCommand,
    format_artifacts_table, format_run_detail, format_run_status, format_runs_table,
};

#[test]
fn parses_submit_command() {
    let cli = Cli::parse_from([
        "capsulet",
        "--api-url",
        "http://localhost:8080",
        "submit",
        "job_hello_python",
        "--pool",
        "mini",
        "--run-id",
        "run_cli_test",
    ]);

    assert_eq!(cli.api_url.as_str(), "http://localhost:8080/");
    let Command::Submit {
        job_definition_id,
        pool,
        run_id,
    } = cli.command
    else {
        panic!("expected submit command");
    };
    assert_eq!(job_definition_id, "job_hello_python");
    assert_eq!(pool, "mini");
    assert_eq!(run_id.as_deref(), Some("run_cli_test"));
}

#[test]
fn parses_submit_script_command() {
    let cli = Cli::parse_from([
        "capsulet",
        "submit-script",
        "job.py",
        "--host-group",
        "mini",
        "--run-id",
        "run_script",
    ]);

    let Command::SubmitScript { path, pool, run_id } = cli.command else {
        panic!("expected submit-script command");
    };
    assert_eq!(path, std::path::PathBuf::from("job.py"));
    assert_eq!(pool, "mini");
    assert_eq!(run_id.as_deref(), Some("run_script"));
}

#[test]
fn parses_run_get_command() {
    let cli = Cli::parse_from(["capsulet", "run", "get", "run_123"]);

    let Command::Run(RunCommand::Get { id }) = cli.command else {
        panic!("expected run get command");
    };
    assert_eq!(id, "run_123");
}

#[test]
fn parses_status_command() {
    let cli = Cli::parse_from(["capsulet", "status", "run_123"]);

    let Command::Status { id } = cli.command else {
        panic!("expected status command");
    };
    assert_eq!(id, "run_123");
}

#[test]
fn parses_logs_command() {
    let cli = Cli::parse_from(["capsulet", "logs", "run_123"]);

    let Command::Logs { id } = cli.command else {
        panic!("expected logs command");
    };
    assert_eq!(id, "run_123");
}

#[test]
fn parses_cancel_command() {
    let cli = Cli::parse_from(["capsulet", "cancel", "run_123"]);

    let Command::Cancel { id } = cli.command else {
        panic!("expected cancel command");
    };
    assert_eq!(id, "run_123");
}

#[test]
fn parses_artifact_commands() {
    let cli = Cli::parse_from(["capsulet", "artifacts", "list", "run_123"]);
    let Command::Artifacts(ArtifactsCommand::List { id }) = cli.command else {
        panic!("expected artifacts list command");
    };
    assert_eq!(id, "run_123");

    let cli = Cli::parse_from([
        "capsulet",
        "artifacts",
        "download",
        "run_123",
        "artifact_1",
        "--output",
        "report.txt",
    ]);
    let Command::Artifacts(ArtifactsCommand::Download {
        id,
        artifact_id,
        output,
    }) = cli.command
    else {
        panic!("expected artifacts download command");
    };
    assert_eq!(id, "run_123");
    assert_eq!(artifact_id, "artifact_1");
    assert_eq!(output, std::path::PathBuf::from("report.txt"));
}

#[test]
fn formats_run_detail() {
    let run = run("run_1", "succeeded", 1);

    assert_eq!(
        format_run_detail(&run),
        "id: run_1\njob_definition_id: job_hello_python\nstatus: succeeded\nexecution_pool: mini\nattempt_count: 1\n"
    );
}

#[test]
fn formats_runs_table() {
    let output = format_runs_table(&[
        run("run_short", "queued", 0),
        run("run_longer", "failed", 2),
    ]);

    assert_eq!(
        output,
        "ID          JOB               STATUS  POOL  ATTEMPTS\nrun_short   job_hello_python  queued  mini  0\nrun_longer  job_hello_python  failed  mini  2\n"
    );
}

#[test]
fn formats_run_status() {
    let run = run("run_1", "running", 1);

    assert_eq!(format_run_status(&run), "run_1  running  attempts=1\n");
}

#[test]
fn formats_artifacts_table() {
    let output = format_artifacts_table(&[ArtifactResponse {
        id: "artifact_1".to_string(),
        name: "report.txt".to_string(),
        content_type: "text/plain".to_string(),
        size_bytes: 6,
        kind: "artifact".to_string(),
    }]);

    assert_eq!(
        output,
        "ID          NAME        KIND      SIZE  CONTENT_TYPE\nartifact_1  report.txt  artifact  6  text/plain\n"
    );
}

#[test]
fn builds_api_urls_from_base_url_with_path() {
    let client = ApiClient::new(
        Url::parse("http://localhost:8080/api-prefix").expect("valid base URL"),
        None,
    )
    .expect("API client");

    let url = client.url(&["v1", "jobs", "runs", "run 1"]).expect("url");

    assert_eq!(url.as_str(), "http://localhost:8080/v1/jobs/runs/run%201");
}

fn run(id: &str, status: &str, attempt_count: u32) -> JobRunResponse {
    JobRunResponse {
        id: id.to_string(),
        job_definition_id: "job_hello_python".to_string(),
        status: status.to_string(),
        execution_pool: "mini".to_string(),
        attempt_count,
    }
}
