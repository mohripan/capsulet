use capsulet_runner::{
    contract::{CollectedArtifact, RunOutcome, RunReport},
    pools::{ExecutionPolicy, ExecutionPoolsConfig, ImagePolicy, PythonDependencyPolicy},
    process::ProcessRunner,
    stub::StubRunner,
};

#[test]
fn runner_public_modules_expose_contracts_and_adapters() {
    let report = RunReport::succeeded_with_artifacts(
        Some("ok".to_string()),
        vec![CollectedArtifact {
            name: "report.txt".to_string(),
            content_type: "text/plain".to_string(),
            bytes: b"ok".to_vec(),
        }],
    );

    assert_eq!(report.outcome, RunOutcome::Succeeded);
    assert_eq!(report.artifacts.len(), 1);
    let _ = StubRunner::success();
    let _ = ProcessRunner;
    let _ = ExecutionPolicy {
        images: ImagePolicy::default(),
        python: PythonDependencyPolicy::default(),
    };
    let _ = ExecutionPoolsConfig::from_yaml(
        r"
defaultPool: mini
pools:
  mini: {}
",
    )
    .expect("minimal pool config is valid");
}
