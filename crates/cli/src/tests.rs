use clap::Parser;
use reqwest::Url;

use super::{
    AgentRunsCommand, ApiClient, ArtifactResponse, ArtifactsCommand, Cli, Command, JobRunResponse,
    RunCommand, checked_bundle, format_artifacts_table, format_run_detail, format_run_status,
    format_runs_table,
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

#[test]
fn parses_agent_run_bundle_command_with_a_project() {
    let cli = Cli::try_parse_from([
        "capsulet",
        "--project",
        "alpha",
        "agent-runs",
        "bundle",
        "agent_run_1",
        "--output",
        "run.json",
    ])
    .expect("parses");
    assert_eq!(cli.project.as_deref(), Some("alpha"));
    match cli.command {
        Command::AgentRuns(AgentRunsCommand::Bundle { id, output }) => {
            assert_eq!(id, "agent_run_1");
            assert_eq!(output, std::path::PathBuf::from("run.json"));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

fn fixture_bundle() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../replay/tests/fixtures/run-succeeded.json"),
    )
    .expect("the replay fixture")
}

#[test]
fn a_bundle_whose_digest_checks_is_accepted_and_summarised() {
    let (sealed, summary) = checked_bundle(&fixture_bundle()).expect("checks");
    assert!(summary.contains("22 events"), "{summary}");
    assert!(summary.contains(&sealed.digest), "{summary}");
}

#[test]
fn a_bundle_whose_body_was_changed_is_refused_before_anything_is_written() {
    let tampered = fixture_bundle().replacen("completed", "cancelled", 1);
    let error = checked_bundle(&tampered).expect_err("refused");
    assert!(
        error.to_string().contains("does not match its digest"),
        "{error}"
    );
}

#[test]
fn verifier_test_reports_each_case_and_fails_when_one_does() {
    let directory =
        std::env::temp_dir().join(format!("capsulet-cli-verifier-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("a directory");
    let registration = directory.join("registration.json");
    let cases = directory.join("cases.json");
    // A replayable tool granted the network is refused before anything runs, so
    // this needs no validator program at all.
    std::fs::write(
        &registration,
        r#"{"name": "networked-tool", "version": "1", "basis": {"kind": "replayable_tool"},
            "environment": {"kind": "process", "program": "unused"}, "capabilities": ["network"]}"#,
    )
    .expect("registration");
    std::fs::write(
        &cases,
        r#"{"cases": [
            {"name": "refused", "obligations": [{"id": "builds", "contract": "patch", "statement": "holds"}],
             "expect_failure": "registration_refused"},
            {"name": "expected to run", "obligations": [{"id": "builds", "contract": "patch", "statement": "holds"}],
             "expect": {"builds": "holds"}}
        ]}"#,
    )
    .expect("cases");

    let failed = super::verifier_test(&registration, &cases).expect_err("one case fails");
    let message = failed.to_string();
    assert!(
        message.contains("pass refused: failed as expected"),
        "{message}"
    );
    assert!(
        message.contains("FAIL expected to run: failed: registration_refused"),
        "{message}"
    );
    assert!(message.contains("1 of 2 cases passed"), "{message}");
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn a_pack_signed_with_a_new_key_verifies_against_the_trust_store_it_was_added_to() {
    let directory = tempfile_dir("capsulet-cli-pack");
    let pack = directory.join("sample");
    std::fs::create_dir_all(pack.join("contracts")).expect("dir");
    std::fs::write(
        pack.join("contracts/sample.contract"),
        "entity Organization:\n  fields:\n    name: string\n",
    )
    .expect("contract");
    std::fs::write(
        pack.join("pack.json"),
        r#"{"schema_version": "capsulet.pack/v1", "name": "sample", "version": "1.0.0",
            "contracts": ["contracts/sample.contract"], "validators": [], "files": []}"#,
    )
    .expect("manifest");
    let key = directory.join("publisher.key.json");
    let trust = directory.join("trust.json");

    super::pack(super::PackCommand::Keygen {
        key: key.clone(),
        trust: trust.clone(),
        packs: vec!["sample".to_string()],
    })
    .expect("a key");
    assert!(
        super::pack(super::PackCommand::Keygen {
            key: key.clone(),
            trust: trust.clone(),
            packs: vec!["sample".to_string()],
        })
        .is_err(),
        "a key is never overwritten"
    );
    let listed =
        super::pack(super::PackCommand::Refresh { dir: pack.clone() }).expect("lists its files");
    assert!(listed.contains("manifest sha256:"), "{listed}");
    let unsigned = super::pack(super::PackCommand::Verify {
        dir: pack.clone(),
        trust: trust.clone(),
    })
    .expect_err("an unsigned pack does not verify");
    assert!(unsigned.to_string().contains("(unsigned)"), "{unsigned}");

    super::pack(super::PackCommand::Sign {
        dir: pack.clone(),
        key,
    })
    .expect("signs");
    let verified = super::pack(super::PackCommand::Verify {
        dir: pack.clone(),
        trust: trust.clone(),
    })
    .expect("verifies");
    assert!(
        verified.starts_with("verified sample 1.0.0: manifest sha256:"),
        "{verified}"
    );

    std::fs::write(
        pack.join("contracts/sample.contract"),
        "entity Changed:\n  fields:\n    name: string\n",
    )
    .expect("tamper");
    let tampered = super::pack(super::PackCommand::Verify { dir: pack, trust })
        .expect_err("a changed file does not verify");
    assert!(
        tampered.to_string().contains("(tampered_file)"),
        "{tampered}"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

fn tempfile_dir(prefix: &str) -> std::path::PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos())
    ));
    std::fs::create_dir_all(&directory).expect("a directory");
    directory
}
