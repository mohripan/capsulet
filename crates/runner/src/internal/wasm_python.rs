//! Python in a WASI sandbox, run with wasmtime.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use async_trait::async_trait;
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};

use super::artifacts::collect_artifacts_from;
use super::contract::{
    CancellationCheck, InputArtifact, RunExecution, RunOutcome, RunReport, Runner,
};
use super::pools::validate_execution_policy;
use super::process::combined_output_logs;

pub(super) const RUNTIME_STAGE_DIR: &str = "runtime";

/// Configuration for running Python scripts through a WASI Python runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmPythonConfig {
    wasmtime_bin: String,
    runtime_path: PathBuf,
}

impl WasmPythonConfig {
    /// Creates a WASI Python runtime configuration.
    #[must_use]
    pub fn new(runtime_path: impl Into<PathBuf>) -> Self {
        Self {
            wasmtime_bin: "wasmtime".to_string(),
            runtime_path: runtime_path.into(),
        }
    }

    /// Sets the `wasmtime` executable path.
    #[must_use]
    pub fn with_wasmtime_bin(mut self, wasmtime_bin: impl Into<String>) -> Self {
        self.wasmtime_bin = wasmtime_bin.into();
        self
    }
}

/// Runner that executes Python scripts inside an operator-provided WASI Python runtime.
#[derive(Debug, Clone)]
pub struct WasmPythonRunner {
    config: WasmPythonConfig,
}

impl WasmPythonRunner {
    #[must_use]
    pub const fn new(config: WasmPythonConfig) -> Self {
        Self { config }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WasmtimeCommandSpec {
    pub(super) current_dir: Option<PathBuf>,
    pub(super) parts: Vec<String>,
}

#[async_trait]
impl Runner for WasmPythonRunner {
    type Error = WasmPythonRunnerError;

    async fn execute<C>(
        &self,
        execution: &RunExecution,
        cancellation: &C,
    ) -> Result<RunReport, Self::Error>
    where
        C: CancellationCheck + Sync,
    {
        if cancellation
            .is_cancelled(execution.run.id())
            .await
            .map_err(|error| WasmPythonRunnerError::CancellationCheck(error.to_string()))?
        {
            return Ok(RunReport::cancelled(None));
        }

        validate_execution_policy(execution).map_err(WasmPythonRunnerError::Policy)?;
        validate_wasm_python_execution(execution)?;

        let sandbox = tempfile::Builder::new()
            .prefix("capsulet-wasm-python-")
            .tempdir()
            .map_err(WasmPythonRunnerError::Io)?;
        stage_wasm_python_sandbox(sandbox.path(), &self.config.runtime_path, execution)?;

        let output = run_wasmtime_python(&self.config, sandbox.path(), execution).await?;
        let logs = combined_output_logs(&output);
        let artifacts = collect_artifacts_from(sandbox.path().join("capsulet").join("artifacts"))?;

        if output.status.success() {
            Ok(RunReport::succeeded_with_artifacts(Some(logs), artifacts))
        } else {
            Ok(RunReport {
                outcome: RunOutcome::Failed,
                logs: Some(logs),
                artifacts,
            })
        }
    }
}

pub(super) async fn run_wasmtime_python(
    config: &WasmPythonConfig,
    sandbox_path: &Path,
    execution: &RunExecution,
) -> Result<std::process::Output, WasmPythonRunnerError> {
    let spec = wasmtime_command_spec(config, sandbox_path)?;
    let Some((program, args)) = spec.parts.split_first() else {
        return Err(WasmPythonRunnerError::InvalidCommand);
    };
    let mut command = Command::new(program);
    command.args(args);
    if let Some(current_dir) = spec.current_dir {
        command.current_dir(current_dir);
    }
    let mut child = command
        .env("CAPSULET_INPUT_JSON", execution.run.input_json())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let child_id = child.id();
    let stdout = child
        .stdout
        .take()
        .ok_or(WasmPythonRunnerError::InvalidCommand)?;
    let stderr = child
        .stderr
        .take()
        .ok_or(WasmPythonRunnerError::InvalidCommand)?;
    let stdout_task = tokio::spawn(read_child_pipe(stdout));
    let stderr_task = tokio::spawn(read_child_pipe(stderr));
    let timeout = tokio::time::sleep(Duration::from_secs(execution.pool.timeout_seconds));
    tokio::pin!(timeout);

    let status = tokio::select! {
        status = child.wait() => status?,
        () = &mut timeout => {
            child.start_kill()?;
            let _ = child.wait().await;
            return Err(WasmPythonRunnerError::TimedOut { process_id: child_id });
        }
    };

    let stdout = stdout_task
        .await
        .map_err(|error| WasmPythonRunnerError::Io(std::io::Error::other(error)))??;
    let stderr = stderr_task
        .await
        .map_err(|error| WasmPythonRunnerError::Io(std::io::Error::other(error)))??;
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

pub(super) async fn read_child_pipe<R>(mut pipe: R) -> Result<Vec<u8>, std::io::Error>
where
    R: AsyncRead + Unpin,
{
    let mut bytes = Vec::new();
    pipe.read_to_end(&mut bytes).await?;
    Ok(bytes)
}

pub(super) fn validate_wasm_python_execution(
    execution: &RunExecution,
) -> Result<(), WasmPythonRunnerError> {
    if !execution.definition.python_dependencies().is_empty() {
        return Err(WasmPythonRunnerError::UnsupportedPythonDependencies);
    }
    let command = execution.definition.command();
    if command.len() == 3 && command[0] == "python" && command[1] == "-c" {
        return Ok(());
    }
    if command.len() == 2 && command[0] == "python" {
        return Ok(());
    }
    Err(WasmPythonRunnerError::UnsupportedCommand(command.join(" ")))
}

pub(super) fn stage_wasm_python_sandbox(
    sandbox_path: &Path,
    runtime_path: &Path,
    execution: &RunExecution,
) -> Result<(), WasmPythonRunnerError> {
    let capsulet_root = sandbox_path.join("capsulet");
    let workspace = capsulet_root.join("workspace");
    let artifacts = capsulet_root.join("artifacts");
    let inputs = capsulet_root.join("inputs");
    fs::create_dir_all(&workspace)?;
    fs::create_dir_all(&artifacts)?;
    fs::create_dir_all(&inputs)?;
    fs::write(capsulet_root.join("input.json"), execution.run.input_json())?;
    write_wasm_python_script(&workspace.join("main.py"), execution.definition.command())?;
    materialize_inputs_at(&inputs, &execution.input_artifacts)?;
    stage_wasm_runtime(runtime_path, &staged_runtime_dir(sandbox_path))?;
    Ok(())
}

pub(super) fn staged_runtime_dir(sandbox_path: &Path) -> PathBuf {
    sandbox_path.join(RUNTIME_STAGE_DIR)
}

/// Copies the runtime and its standard library into the per-run sandbox.
///
/// `wasmtime --dir` preopens grant write access, so the guest must never see the
/// installed runtime itself: a job that appended to the stdlib there would run in
/// every later job on this worker.
pub(super) fn stage_wasm_runtime(
    runtime_path: &Path,
    staged: &Path,
) -> Result<(), WasmPythonRunnerError> {
    let source = runtime_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| {
            WasmPythonRunnerError::RuntimeDirectoryRequired(runtime_path.display().to_string())
        })?;
    copy_dir_recursive(source, staged)?;
    Ok(())
}

pub(super) fn copy_dir_recursive(source: &Path, target: &Path) -> Result<(), std::io::Error> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let destination = target.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &destination)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &destination)?;
        }
    }
    Ok(())
}

pub(super) fn write_wasm_python_script(
    path: &Path,
    command: &[String],
) -> Result<(), WasmPythonRunnerError> {
    match command {
        [program, flag, script] if program == "python" && flag == "-c" => {
            fs::write(path, script)?;
            Ok(())
        }
        [program, script_path] if program == "python" => {
            let script = fs::read_to_string(script_path)?;
            fs::write(path, script)?;
            Ok(())
        }
        _ => Err(WasmPythonRunnerError::UnsupportedCommand(command.join(" "))),
    }
}

pub(super) fn materialize_inputs_at(
    root: &Path,
    artifacts: &[InputArtifact],
) -> Result<(), std::io::Error> {
    for artifact in artifacts {
        let producer_path = root.join(&artifact.producer_step_id).join(&artifact.name);
        if let Some(parent) = producer_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&producer_path, &artifact.bytes)?;
        fs::write(root.join(&artifact.name), &artifact.bytes)?;
    }
    Ok(())
}

pub(super) fn wasmtime_command_spec(
    config: &WasmPythonConfig,
    sandbox_path: &Path,
) -> Result<WasmtimeCommandSpec, WasmPythonRunnerError> {
    let capsulet_root = sandbox_path.join("capsulet");
    let mapped_root = capsulet_root
        .to_str()
        .ok_or(WasmPythonRunnerError::NonUtf8Path)?;
    let runtime_name = config
        .runtime_path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or(WasmPythonRunnerError::NonUtf8Path)?;
    Ok(WasmtimeCommandSpec {
        // `--dir .` resolves against the process directory, which is the sandbox
        // copy of the runtime rather than the installed one.
        current_dir: Some(staged_runtime_dir(sandbox_path)),
        parts: vec![
            config.wasmtime_bin.clone(),
            "--dir".to_string(),
            ".".to_string(),
            "--dir".to_string(),
            format!("{mapped_root}::/capsulet"),
            "--env".to_string(),
            "CAPSULET_INPUT_JSON".to_string(),
            runtime_name.to_string(),
            "/capsulet/workspace/main.py".to_string(),
        ],
    })
}

/// WASI Python runner error.
#[derive(Debug, Error)]
pub enum WasmPythonRunnerError {
    #[error("WASI Python runner does not support python_dependencies yet")]
    UnsupportedPythonDependencies,
    #[error("WASI Python runner only supports python -c scripts or python script files: {0}")]
    UnsupportedCommand(String),
    #[error("wasmtime command could not be rendered")]
    InvalidCommand,
    #[error("wasmtime command path is not valid UTF-8")]
    NonUtf8Path,
    #[error("WASI Python runtime path {0} must include a directory component")]
    RuntimeDirectoryRequired(String),
    #[error("wasmtime failed to run Python script: {0}")]
    Io(#[from] std::io::Error),
    #[error("WASI Python execution timed out for process {process_id:?}")]
    TimedOut { process_id: Option<u32> },
    #[error("cancellation check failed: {0}")]
    CancellationCheck(String),
    #[error("execution policy rejected run: {0}")]
    Policy(String),
}
