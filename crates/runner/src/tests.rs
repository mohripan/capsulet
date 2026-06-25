use base64::Engine as _;
use capsulet_core::{ExecutionPoolName, JobDefinition, JobRun, JobRunId};

use super::{
    ExecutionPoolConfig, ExecutionPoolsConfig, InputArtifact, PoolResources, PoolToleration,
    RUN_LABEL, RunExecution, WasmPythonConfig, build_job, kubernetes_job_name, run_label_value,
    truncate_utf8,
};

fn execution(pool: ExecutionPoolConfig) -> RunExecution {
    RunExecution {
        run: JobRun::new(
            JobRunId::new("run_hello_python").expect("valid run id"),
            JobDefinition::hello_python().id().clone(),
            ExecutionPoolName::new("mini").expect("valid pool"),
        ),
        definition: JobDefinition::hello_python(),
        pool,
        input_artifacts: Vec::new(),
    }
}

#[test]
fn renders_input_artifacts_in_kubernetes_wrapper() {
    let mut execution = execution(pool());
    execution.input_artifacts.push(InputArtifact {
        producer_step_id: "generate-csv".to_string(),
        name: "customers.csv".to_string(),
        bytes: b"name,total\nAda,3\n".to_vec(),
    });

    let job = build_job(&execution, "capsulet-exec");
    let pod_spec = job.spec.expect("job spec").template.spec.expect("pod spec");
    let command = &pod_spec.containers[0].command.as_ref().expect("command")[2];

    assert!(
        command.contains("/capsulet/inputs/generate-csv/customers.csv"),
        "{command}"
    );
    assert!(command.contains("/capsulet/inputs/customers.csv"));
    assert!(command.contains("bmFtZSx0b3RhbApBZGEsMwo="));
}

fn pool() -> ExecutionPoolConfig {
    let mut requests = std::collections::BTreeMap::new();
    requests.insert("cpu".to_string(), "100m".to_string());
    requests.insert("memory".to_string(), "128Mi".to_string());
    let mut limits = std::collections::BTreeMap::new();
    limits.insert("cpu".to_string(), "500m".to_string());
    limits.insert("memory".to_string(), "512Mi".to_string());
    let mut node_selector = std::collections::BTreeMap::new();
    node_selector.insert("capsulet.dev/pool".to_string(), "mini".to_string());

    ExecutionPoolConfig {
        description: "Lightweight".to_string(),
        node_selector,
        tolerations: vec![PoolToleration {
            key: Some("capsulet.dev/pool".to_string()),
            operator: Some("Equal".to_string()),
            value: Some("mini".to_string()),
            effect: Some("NoSchedule".to_string()),
        }],
        resources: PoolResources { requests, limits },
        timeout_seconds: 120,
        max_concurrent_jobs: 50,
        ttl_seconds_after_finished: Some(300),
        runtime_class_name: Some("gvisor".to_string()),
        service_account_name: Some("capsulet-execution".to_string()),
        ..ExecutionPoolConfig::default()
    }
}

#[test]
fn renders_job_metadata_and_container() {
    let job = build_job(&execution(pool()), "capsulet-exec");
    let pod_spec = job.spec.as_ref().unwrap().template.spec.as_ref().unwrap();
    assert_eq!(pod_spec.runtime_class_name.as_deref(), Some("gvisor"));
    assert_eq!(
        pod_spec.service_account_name.as_deref(),
        Some("capsulet-execution")
    );

    assert_eq!(
        job.metadata.name.as_deref(),
        Some("capsulet-run-hello-python-a0")
    );
    assert_eq!(job.metadata.namespace.as_deref(), Some("capsulet-exec"));
    assert_eq!(
        job.spec
            .as_ref()
            .expect("job spec")
            .ttl_seconds_after_finished,
        Some(300)
    );

    let pod_spec = job.spec.expect("job spec").template.spec.expect("pod spec");
    assert_eq!(pod_spec.restart_policy.as_deref(), Some("Never"));
    assert_eq!(pod_spec.automount_service_account_token, Some(false));
    assert_eq!(pod_spec.host_network, Some(false));
    assert_eq!(
        pod_spec
            .security_context
            .as_ref()
            .expect("pod security")
            .run_as_non_root,
        Some(true)
    );
    assert_eq!(
        pod_spec.node_selector.expect("node selector")["capsulet.dev/pool"],
        "mini"
    );
    assert_eq!(
        pod_spec.tolerations.expect("tolerations")[0]
            .value
            .as_deref(),
        Some("mini")
    );

    let container = &pod_spec.containers[0];
    assert_eq!(container.name, "main");
    assert_eq!(container.image.as_deref(), Some("python:3.12-slim"));
    assert_eq!(container.command.as_ref().expect("command")[0], "/bin/sh");
    assert!(container.command.as_ref().expect("command")[2].contains("hello from capsulet"));
    let security = container
        .security_context
        .as_ref()
        .expect("container security");
    assert_eq!(security.read_only_root_filesystem, Some(true));
    assert_eq!(security.allow_privilege_escalation, Some(false));
    let resources = container.resources.as_ref().expect("resources");
    assert_eq!(
        resources.requests.as_ref().expect("requests")["cpu"].0,
        "100m"
    );
    assert_eq!(
        resources.limits.as_ref().expect("limits")["memory"].0,
        "512Mi"
    );
}

#[test]
fn renders_run_label_on_job_and_pod_template() {
    let execution = execution(pool());
    let expected = run_label_value(execution.run.id());
    let job = build_job(&execution, "capsulet-exec");

    assert_eq!(
        job.metadata.labels.as_ref().expect("job labels")[RUN_LABEL],
        expected
    );
    assert_eq!(
        job.spec
            .as_ref()
            .expect("job spec")
            .template
            .metadata
            .as_ref()
            .expect("pod template metadata")
            .labels
            .as_ref()
            .expect("pod template labels")[RUN_LABEL],
        expected
    );
}

#[test]
fn parses_execution_pool_yaml() {
    let yaml = r"
defaultPool: mini
pools:
  mini:
    description: Lightweight
    nodeSelector:
      capsulet.dev/pool: mini
    tolerations: []
    resources:
      requests:
        cpu: 100m
      limits:
        memory: 512Mi
    timeoutSeconds: 120
    maxConcurrentJobs: 50
    ttlSecondsAfterFinished: 300
";

    let pools = ExecutionPoolsConfig::from_yaml(yaml).expect("pool yaml");

    assert_eq!(pools.default_pool, "mini");
    assert_eq!(pools.find("mini").expect("mini").timeout_seconds, 120);
    assert_eq!(
        pools.find("mini").expect("mini").ttl_seconds_after_finished,
        Some(300)
    );
}

#[test]
fn sanitizes_job_name() {
    let name = kubernetes_job_name(&JobRunId::new("Run_With Spaces").expect("valid id"), 0);

    assert_eq!(name, "capsulet-run-with-spaces-a0");
}

#[test]
fn truncates_logs_on_utf8_boundary() {
    assert_eq!(truncate_utf8("abcd", 3), "abc");
    assert_eq!(truncate_utf8("éé", 3), "é");
}

#[test]
fn parses_artifact_markers_from_logs() {
    let encoded = base64::engine::general_purpose::STANDARD.encode("hello");
    let (logs, artifacts) = super::split_artifact_markers(Some(format!(
        "before\nCAPSULET_ARTIFACT\treport.txt\ttext/plain\t{encoded}\nafter\n"
    )));

    assert_eq!(logs.as_deref(), Some("before\nafter\n"));
    assert_eq!(artifacts[0].name, "report.txt");
    assert_eq!(artifacts[0].bytes, b"hello");
}

#[test]
fn renders_wasmtime_command_with_sandbox_preopen_before_runtime() {
    let config = WasmPythonConfig::new("runtime/python.wasm").with_wasmtime_bin("wasmtime");
    let spec =
        super::wasmtime_command_spec(&config, std::path::Path::new("sandbox")).expect("command");

    assert_eq!(
        spec.current_dir.as_deref(),
        Some(std::path::Path::new("runtime"))
    );
    assert_eq!(spec.parts[0], "wasmtime");
    assert_eq!(&spec.parts[1..3], ["--dir", "."]);
    assert_eq!(spec.parts[3], "--dir");
    assert_eq!(
        spec.parts[4],
        format!(
            "{}::/capsulet",
            std::path::Path::new("sandbox").join("capsulet").display()
        )
    );
    assert_eq!(
        &spec.parts[5..],
        [
            "--env",
            "CAPSULET_INPUT_JSON",
            "python.wasm",
            "/capsulet/workspace/main.py"
        ]
    );
}

#[test]
fn wasm_python_runner_rejects_python_dependencies() {
    let mut execution = execution(pool());
    execution.definition = JobDefinition::new(
        execution.definition.id().clone(),
        execution.definition.name(),
        execution.definition.runtime_image(),
        execution.definition.command().to_vec(),
        vec!["requests==2.32.5".to_string()],
        execution.definition.bundle_object_key(),
        execution.definition.input_schema(),
        capsulet_core::RetryPolicy::no_retry(),
    )
    .expect("dependency definition");

    let error = super::validate_wasm_python_execution(&execution)
        .expect_err("dependencies should be rejected");

    assert_eq!(
        error.to_string(),
        "WASI Python runner does not support python_dependencies yet"
    );
}
