use async_trait::async_trait;
use capsulet_core::{
    ExecutionPoolName, JobDefinition, JobDefinitionId, JobRun, JobRunId, RetryPolicy,
};
use capsulet_runner::{
    CancellationCheck, ExecutionPoolConfig, NeverCancelledError, RunExecution, RunOutcome, Runner,
    WasmPythonConfig, WasmPythonRunner,
};

#[tokio::test]
async fn wasm_python_runner_executes_script_and_collects_artifact() {
    let Some(runtime_path) = std::env::var_os("CAPSULET_WASM_RUNTIME_PATH") else {
        eprintln!("skipping WASI Python integration test; CAPSULET_WASM_RUNTIME_PATH is not set");
        return;
    };
    let wasmtime_bin =
        std::env::var("CAPSULET_WASMTIME_BIN").unwrap_or_else(|_| "wasmtime".to_string());
    let runner =
        WasmPythonRunner::new(WasmPythonConfig::new(runtime_path).with_wasmtime_bin(wasmtime_bin));
    let execution = RunExecution {
        run: JobRun::new(
            JobRunId::new("run_wasm_python").expect("run id"),
            JobDefinitionId::new("job_wasm_python").expect("job id"),
            ExecutionPoolName::new("mini").expect("pool"),
        )
        .with_input(r#"{"message":"hello from input"}"#)
        .expect("input"),
        definition: JobDefinition::new(
            JobDefinitionId::new("job_wasm_python").expect("job id"),
            "WASI Python",
            "python-wasi",
            vec![
                "python".to_string(),
                "-c".to_string(),
                r#"
import json
import os
from pathlib import Path

payload = json.loads(os.environ["CAPSULET_INPUT_JSON"])
print("wasm saw " + payload["message"])
Path("/capsulet/artifacts").mkdir(parents=True, exist_ok=True)
Path("/capsulet/artifacts/report.txt").write_text("artifact from wasm\n")
"#
                .to_string(),
            ],
            Vec::new(),
            "bundles/job_wasm_python.tar.gz",
            "{}",
            RetryPolicy::no_retry(),
        )
        .expect("definition"),
        pool: ExecutionPoolConfig {
            timeout_seconds: 30,
            ..ExecutionPoolConfig::default()
        },
        input_artifacts: Vec::new(),
    };

    let report = runner
        .execute(&execution, &NeverCancelled)
        .await
        .expect("wasm python execution");

    assert_eq!(
        report.outcome,
        RunOutcome::Succeeded,
        "logs: {:?}",
        report.logs
    );
    assert!(
        report
            .logs
            .as_deref()
            .is_some_and(|logs| logs.contains("wasm saw hello from input")),
        "{:?}",
        report.logs
    );
    assert_eq!(report.artifacts.len(), 1);
    assert_eq!(report.artifacts[0].name, "report.txt");
    assert_eq!(report.artifacts[0].bytes, b"artifact from wasm\n");
}

#[derive(Debug, Clone, Copy)]
struct NeverCancelled;

#[async_trait]
impl CancellationCheck for NeverCancelled {
    type Error = NeverCancelledError;

    async fn is_cancelled(&self, _run_id: &JobRunId) -> Result<bool, Self::Error> {
        Ok(false)
    }
}
