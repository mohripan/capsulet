use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use capsulet_core::{JobDefinition, JobRun, JobRunId};
use k8s_openapi::{
    api::{
        batch::v1::{Job, JobSpec},
        core::v1::{
            Capabilities, Container, EmptyDirVolumeSource, EnvVar, Pod, PodSecurityContext,
            PodSpec, PodTemplateSpec, ResourceRequirements, SeccompProfile, SecurityContext,
            Toleration, Volume, VolumeMount,
        },
    },
    apimachinery::pkg::{api::resource::Quantity, apis::meta::v1::ObjectMeta},
};
use kube::{
    Api, Client, ResourceExt,
    api::{DeleteParams, ListParams, LogParams, PostParams},
};
use serde::Deserialize;
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    time::{Instant, sleep},
};

const APP_LABEL: &str = "capsulet.dev/managed-by";
const RUN_LABEL: &str = "capsulet.dev/job-run-key";
const ATTEMPT_LABEL: &str = "capsulet.dev/attempt";
const DEFAULT_JOB_TIMEOUT_SECONDS: u64 = 300;
const DEFAULT_JOB_TIMEOUT_SECONDS_I64: i64 = 300;
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const ARTIFACT_MARKER: &str = "CAPSULET_ARTIFACT";
const ARTIFACT_DIR: &str = "/capsulet/artifacts";
const INPUT_DIR: &str = "/capsulet/inputs";

/// Execution result returned by a runner backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
}

/// Runner output for one execution attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    pub outcome: RunOutcome,
    pub logs: Option<String>,
    pub artifacts: Vec<CollectedArtifact>,
}

impl RunReport {
    #[must_use]
    pub fn succeeded(logs: Option<String>) -> Self {
        Self {
            outcome: RunOutcome::Succeeded,
            logs,
            artifacts: Vec::new(),
        }
    }

    #[must_use]
    pub fn succeeded_with_artifacts(
        logs: Option<String>,
        artifacts: Vec<CollectedArtifact>,
    ) -> Self {
        Self {
            outcome: RunOutcome::Succeeded,
            logs,
            artifacts,
        }
    }

    #[must_use]
    pub fn failed(logs: Option<String>) -> Self {
        Self {
            outcome: RunOutcome::Failed,
            logs,
            artifacts: Vec::new(),
        }
    }

    #[must_use]
    pub fn timed_out(logs: Option<String>) -> Self {
        Self {
            outcome: RunOutcome::TimedOut,
            logs,
            artifacts: Vec::new(),
        }
    }

    #[must_use]
    pub fn cancelled(logs: Option<String>) -> Self {
        Self {
            outcome: RunOutcome::Cancelled,
            logs,
            artifacts: Vec::new(),
        }
    }
}

/// Artifact bytes collected by a runner before persistence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectedArtifact {
    pub name: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// Artifact produced by a successful prerequisite and staged for this run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputArtifact {
    pub producer_step_id: String,
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Complete execution input for a leased job run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunExecution {
    pub run: JobRun,
    pub definition: JobDefinition,
    pub pool: ExecutionPoolConfig,
    pub input_artifacts: Vec<InputArtifact>,
}

/// Boundary for executing a leased job run.
#[async_trait]
pub trait Runner: Clone + Send + Sync + 'static {
    type Error: std::fmt::Display + Send + Sync + 'static;

    #[must_use]
    fn supports_reattachment(&self) -> bool {
        false
    }

    /// Executes a leased job run.
    ///
    /// # Errors
    ///
    /// Returns an implementation-specific error when the runner cannot produce
    /// an execution outcome.
    async fn execute<C>(
        &self,
        execution: &RunExecution,
        cancellation: &C,
    ) -> Result<RunReport, Self::Error>
    where
        C: CancellationCheck + Sync;
}

/// Read-side cancellation hook used while a runner is waiting for work.
#[async_trait]
pub trait CancellationCheck {
    type Error: std::fmt::Display + Send + Sync + 'static;

    async fn is_cancelled(&self, run_id: &JobRunId) -> Result<bool, Self::Error>;
}

/// Cancellation checker that never requests cancellation.
#[derive(Debug, Clone, Copy)]
pub struct NeverCancelled;

#[async_trait]
impl CancellationCheck for NeverCancelled {
    type Error = NeverCancelledError;

    async fn is_cancelled(&self, _run_id: &JobRunId) -> Result<bool, Self::Error> {
        Ok(false)
    }
}

#[derive(Debug, Error)]
pub enum NeverCancelledError {}

/// Deterministic runner used for tests and local failure simulation.
#[derive(Debug, Clone)]
pub struct StubRunner {
    outcome: RunOutcome,
    logs: Option<String>,
    artifact: Option<String>,
}

/// Local process runner for Docker Compose smoke tests.
#[derive(Debug, Clone, Copy)]
pub struct ProcessRunner;

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
struct WasmtimeCommandSpec {
    current_dir: Option<PathBuf>,
    parts: Vec<String>,
}

#[async_trait]
impl Runner for ProcessRunner {
    type Error = ProcessRunnerError;

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
            .map_err(|error| ProcessRunnerError::CancellationCheck(error.to_string()))?
        {
            return Ok(RunReport::cancelled(None));
        }

        validate_execution_policy(execution).map_err(ProcessRunnerError::Policy)?;
        reset_artifact_dir()?;
        materialize_local_inputs(&execution.input_artifacts)?;
        if !execution.definition.python_dependencies().is_empty() {
            let install_output =
                install_python_dependencies(execution.definition.python_dependencies()).await?;
            if !install_output.status.success() {
                return Ok(RunReport::failed(Some(combined_output_logs(
                    &install_output,
                ))));
            }
        }
        let Some((program, args)) = execution.definition.command().split_first() else {
            return Err(ProcessRunnerError::EmptyCommand);
        };
        let mut child = Command::new(local_program(program))
            .args(args)
            .env("CAPSULET_INPUT_JSON", execution.run.input_json())
            .env("PYTHONPATH", process_python_path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        loop {
            if cancellation
                .is_cancelled(execution.run.id())
                .await
                .map_err(|error| ProcessRunnerError::CancellationCheck(error.to_string()))?
            {
                child.start_kill()?;
                let output = child.wait_with_output().await?;
                let logs = combined_output_logs(&output);
                return Ok(RunReport::cancelled(Some(logs)));
            }
            if child.try_wait()?.is_some() {
                break;
            }
            sleep(POLL_INTERVAL).await;
        }
        let output = child.wait_with_output().await?;
        let logs = combined_output_logs(&output);
        let artifacts = collect_local_artifacts()?;
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
        stage_wasm_python_sandbox(sandbox.path(), execution)?;

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

async fn run_wasmtime_python(
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

async fn read_child_pipe<R>(mut pipe: R) -> Result<Vec<u8>, std::io::Error>
where
    R: AsyncRead + Unpin,
{
    let mut bytes = Vec::new();
    pipe.read_to_end(&mut bytes).await?;
    Ok(bytes)
}

fn validate_wasm_python_execution(execution: &RunExecution) -> Result<(), WasmPythonRunnerError> {
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

fn stage_wasm_python_sandbox(
    sandbox_path: &Path,
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
    Ok(())
}

fn write_wasm_python_script(path: &Path, command: &[String]) -> Result<(), WasmPythonRunnerError> {
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

fn materialize_inputs_at(root: &Path, artifacts: &[InputArtifact]) -> Result<(), std::io::Error> {
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

fn wasmtime_command_spec(
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
    let current_dir = config
        .runtime_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf);
    Ok(WasmtimeCommandSpec {
        current_dir,
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

fn process_python_path() -> String {
    match std::env::var("PYTHONPATH") {
        Ok(existing) if !existing.is_empty() => format!("/capsulet/python-site:{existing}"),
        _ => "/capsulet/python-site".to_string(),
    }
}

fn combined_output_logs(output: &std::process::Output) -> String {
    let mut logs = String::new();
    logs.push_str(&String::from_utf8_lossy(&output.stdout));
    logs.push_str(&String::from_utf8_lossy(&output.stderr));
    logs
}

fn local_program(program: &str) -> &str {
    if program == "python" {
        "python3"
    } else {
        program
    }
}

async fn install_python_dependencies(
    dependencies: &[String],
) -> Result<std::process::Output, std::io::Error> {
    Command::new(local_program("python"))
        .arg("-m")
        .arg("pip")
        .arg("install")
        .arg("--disable-pip-version-check")
        .arg("--no-input")
        .arg("--break-system-packages")
        .arg("--target")
        .arg("/capsulet/python-site")
        .args(dependencies)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
}

#[derive(Debug, Error)]
pub enum ProcessRunnerError {
    #[error("process command cannot be empty")]
    EmptyCommand,
    #[error("process execution failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("cancellation check failed: {0}")]
    CancellationCheck(String),
    #[error("execution policy rejected run: {0}")]
    Policy(String),
}

impl StubRunner {
    /// Creates a stub runner that always succeeds.
    #[must_use]
    pub const fn success() -> Self {
        Self {
            outcome: RunOutcome::Succeeded,
            logs: None,
            artifact: None,
        }
    }

    /// Creates a stub runner that always fails.
    #[must_use]
    pub const fn failure() -> Self {
        Self {
            outcome: RunOutcome::Failed,
            logs: None,
            artifact: None,
        }
    }

    /// Creates a successful stub runner that emits one text artifact.
    #[must_use]
    pub fn success_with_artifact(text: impl Into<String>) -> Self {
        Self {
            outcome: RunOutcome::Succeeded,
            logs: None,
            artifact: Some(text.into()),
        }
    }

    /// Creates a successful stub runner with deterministic logs.
    #[must_use]
    pub fn success_with_logs(logs: impl Into<String>) -> Self {
        Self {
            outcome: RunOutcome::Succeeded,
            logs: Some(logs.into()),
            artifact: None,
        }
    }

    /// Creates a successful stub runner with deterministic logs and one text artifact.
    #[must_use]
    pub fn success_with_logs_and_artifact(
        logs: impl Into<String>,
        artifact: impl Into<String>,
    ) -> Self {
        Self {
            outcome: RunOutcome::Succeeded,
            logs: Some(logs.into()),
            artifact: Some(artifact.into()),
        }
    }
}

#[async_trait]
impl Runner for StubRunner {
    type Error = StubRunnerError;

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
            .map_err(|_| StubRunnerError::CancellationCheck)?
        {
            return Ok(RunReport::cancelled(None));
        }
        let artifact = self.artifact.clone().map(|text| CollectedArtifact {
            name: "stub-artifact.txt".to_string(),
            content_type: "text/plain".to_string(),
            bytes: text.into_bytes(),
        });
        let report = match self.outcome {
            RunOutcome::Succeeded => RunReport::succeeded_with_artifacts(
                self.logs.clone(),
                artifact.into_iter().collect(),
            ),
            RunOutcome::Failed => RunReport::failed(self.logs.clone()),
            RunOutcome::TimedOut => RunReport::timed_out(self.logs.clone()),
            RunOutcome::Cancelled => RunReport::cancelled(self.logs.clone()),
        };
        Ok(report)
    }
}

/// Error type for [`StubRunner`].
#[derive(Debug, Error)]
pub enum StubRunnerError {
    #[error("cancellation check failed")]
    CancellationCheck,
}

/// Static execution pool configuration used to render Kubernetes Jobs.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPoolConfig {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub node_selector: BTreeMap<String, String>,
    #[serde(default)]
    pub tolerations: Vec<PoolToleration>,
    #[serde(default)]
    pub resources: PoolResources,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub max_concurrent_jobs: u32,
    #[serde(default)]
    pub ttl_seconds_after_finished: Option<i32>,
    #[serde(default)]
    pub runtime_class_name: Option<String>,
    #[serde(default)]
    pub service_account_name: Option<String>,
    #[serde(default)]
    pub policy: ExecutionPolicy,
    #[serde(default)]
    pub allowed_images: Vec<String>,
    #[serde(default)]
    pub require_digest_images: bool,
    #[serde(default)]
    pub max_python_dependencies: Option<usize>,
    #[serde(default)]
    pub max_python_dependency_length: Option<usize>,
    #[serde(default)]
    pub blocked_python_dependencies: Vec<String>,
}

impl Default for ExecutionPoolConfig {
    fn default() -> Self {
        Self {
            description: String::new(),
            node_selector: BTreeMap::new(),
            tolerations: Vec::new(),
            resources: PoolResources::default(),
            timeout_seconds: default_timeout_seconds(),
            max_concurrent_jobs: 0,
            ttl_seconds_after_finished: None,
            runtime_class_name: None,
            service_account_name: None,
            policy: ExecutionPolicy::default(),
            allowed_images: Vec::new(),
            require_digest_images: false,
            max_python_dependencies: None,
            max_python_dependency_length: None,
            blocked_python_dependencies: Vec::new(),
        }
    }
}

impl ExecutionPoolConfig {
    #[must_use]
    pub fn execution_policy(&self) -> ExecutionPolicy {
        let mut policy = self.policy.clone();
        if !self.allowed_images.is_empty() {
            policy.images.allowed.clone_from(&self.allowed_images);
        }
        if self.require_digest_images {
            policy.images.require_digest = true;
        }
        if self.max_python_dependencies.is_some() {
            policy.python.max_dependencies = self.max_python_dependencies;
        }
        if self.max_python_dependency_length.is_some() {
            policy.python.max_dependency_length = self.max_python_dependency_length;
        }
        if !self.blocked_python_dependencies.is_empty() {
            policy
                .python
                .blocked_dependencies
                .clone_from(&self.blocked_python_dependencies);
        }
        policy
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPolicy {
    #[serde(default)]
    pub images: ImagePolicy,
    #[serde(default)]
    pub python: PythonDependencyPolicy,
}

impl ExecutionPolicy {
    /// Validates an execution request against image and Python dependency policy.
    ///
    /// # Errors
    ///
    /// Returns an error when the runtime image is not allowed, a digest is
    /// required but missing, or the Python dependency list violates the pool
    /// policy.
    pub fn validate(
        &self,
        runtime_image: &str,
        python_dependencies: &[String],
    ) -> Result<(), String> {
        self.images.validate(runtime_image)?;
        self.python.validate(python_dependencies)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImagePolicy {
    #[serde(default)]
    pub allowed: Vec<String>,
    #[serde(default)]
    pub require_digest: bool,
}

impl ImagePolicy {
    fn validate(&self, image: &str) -> Result<(), String> {
        if self.require_digest && !image.contains("@sha256:") {
            return Err(format!("runtime image {image} must be pinned by digest"));
        }
        if !self.allowed.is_empty()
            && !self
                .allowed
                .iter()
                .any(|allowed| image_matches_policy(image, allowed))
        {
            return Err(format!("runtime image {image} is not allowed in this pool"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonDependencyPolicy {
    #[serde(default)]
    pub max_dependencies: Option<usize>,
    #[serde(default)]
    pub max_dependency_length: Option<usize>,
    #[serde(default)]
    pub blocked_dependencies: Vec<String>,
}

impl PythonDependencyPolicy {
    fn validate(&self, dependencies: &[String]) -> Result<(), String> {
        if let Some(max) = self.max_dependencies
            && dependencies.len() > max
        {
            return Err(format!(
                "python dependency count {} exceeds pool limit {max}",
                dependencies.len()
            ));
        }
        if let Some(max) = self.max_dependency_length
            && let Some(dependency) = dependencies
                .iter()
                .find(|dependency| dependency.len() > max)
        {
            return Err(format!(
                "python dependency {dependency} exceeds pool length limit {max}"
            ));
        }
        for dependency in dependencies {
            if self
                .blocked_dependencies
                .iter()
                .any(|blocked| dependency_name_matches(dependency, blocked))
            {
                return Err(format!("python dependency {dependency} is blocked"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct PoolResources {
    #[serde(default)]
    pub requests: BTreeMap<String, String>,
    #[serde(default)]
    pub limits: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct PoolToleration {
    pub key: Option<String>,
    pub operator: Option<String>,
    pub value: Option<String>,
    pub effect: Option<String>,
}

/// Execution pools document shape stored in Helm values/config.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPoolsConfig {
    pub default_pool: String,
    pub pools: BTreeMap<String, ExecutionPoolConfig>,
}

impl ExecutionPoolsConfig {
    /// Parses execution pool YAML.
    ///
    /// # Errors
    ///
    /// Returns an error when the YAML cannot be decoded.
    pub fn from_yaml(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    #[must_use]
    pub fn find(&self, name: &str) -> Option<&ExecutionPoolConfig> {
        self.pools.get(name)
    }
}

fn validate_execution_policy(execution: &RunExecution) -> Result<(), String> {
    execution.pool.execution_policy().validate(
        execution.definition.runtime_image(),
        execution.definition.python_dependencies(),
    )
}

fn image_matches_policy(image: &str, allowed: &str) -> bool {
    allowed
        .strip_suffix('*')
        .map_or_else(|| image == allowed, |prefix| image.starts_with(prefix))
}

fn dependency_name_matches(dependency: &str, blocked: &str) -> bool {
    let blocked = blocked.trim().to_ascii_lowercase();
    if blocked.is_empty() {
        return false;
    }
    let name = dependency
        .trim()
        .split(['=', '<', '>', '!', '~', '[', ';', ' '])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    name == blocked
}

/// Kubernetes-backed runner.
#[derive(Clone)]
pub struct KubernetesRunner {
    client: Client,
    namespace: String,
    log_limit_bytes: usize,
}

impl KubernetesRunner {
    #[must_use]
    pub const fn new(client: Client, namespace: String, log_limit_bytes: usize) -> Self {
        Self {
            client,
            namespace,
            log_limit_bytes,
        }
    }

    /// Builds a runner from the current Kubernetes config.
    ///
    /// # Errors
    ///
    /// Returns [`KubernetesRunnerError`] when kube config cannot be loaded.
    pub async fn from_default_config(
        namespace: String,
        log_limit_bytes: usize,
    ) -> Result<Self, KubernetesRunnerError> {
        let client = Client::try_default().await?;
        Ok(Self::new(client, namespace, log_limit_bytes))
    }

    /// Deletes Capsulet-managed Kubernetes Jobs whose run id is no longer active in storage.
    ///
    /// # Errors
    ///
    /// Returns [`KubernetesRunnerError`] when Kubernetes listing or deletion fails.
    pub async fn reconcile_orphaned_jobs(
        &self,
        active_run_ids: &[JobRunId],
    ) -> Result<u64, KubernetesRunnerError> {
        let active_labels = active_run_ids
            .iter()
            .map(run_label_value)
            .collect::<BTreeSet<_>>();
        let jobs: Api<Job> = Api::namespaced(self.client.clone(), &self.namespace);
        let list = jobs
            .list(&ListParams::default().labels(&format!("{APP_LABEL}=capsulet")))
            .await?;
        let delete_params = DeleteParams {
            propagation_policy: Some(kube::api::PropagationPolicy::Background),
            ..DeleteParams::default()
        };
        let mut deleted = 0;
        for job in list {
            let name = job.name_any();
            if name.is_empty() {
                continue;
            }
            let Some(labels) = job.metadata.labels.as_ref() else {
                continue;
            };
            let Some(run_label) = labels.get(RUN_LABEL) else {
                continue;
            };
            if active_labels.contains(run_label) {
                continue;
            }
            jobs.delete(&name, &delete_params).await?;
            deleted += 1;
        }
        Ok(deleted)
    }
}

#[async_trait]
impl Runner for KubernetesRunner {
    type Error = KubernetesRunnerError;

    fn supports_reattachment(&self) -> bool {
        true
    }

    async fn execute<C>(
        &self,
        execution: &RunExecution,
        cancellation: &C,
    ) -> Result<RunReport, Self::Error>
    where
        C: CancellationCheck + Sync,
    {
        let jobs: Api<Job> = Api::namespaced(self.client.clone(), &self.namespace);
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);
        validate_execution_policy(execution).map_err(KubernetesRunnerError::Policy)?;
        let job = build_job(execution, &self.namespace);
        let job_name = job
            .metadata
            .name
            .clone()
            .ok_or(KubernetesRunnerError::MissingJobName)?;

        match jobs.create(&PostParams::default(), &job).await {
            Ok(_) => {}
            Err(kube::Error::Api(error)) if error.code == 409 => {
                let existing = jobs.get(&job_name).await?;
                validate_job_identity(&existing, execution)?;
            }
            Err(error) => return Err(error.into()),
        }

        let outcome = wait_for_job(
            &jobs,
            &pods,
            &job_name,
            execution,
            execution.pool.timeout_seconds,
            cancellation,
        )
        .await?;
        let logs = self.collect_logs(execution).await?;
        let (logs, artifacts) = split_artifact_markers(logs);

        Ok(match outcome {
            RunOutcome::Succeeded => RunReport::succeeded_with_artifacts(logs, artifacts),
            RunOutcome::Failed => {
                let mut report = RunReport::failed(logs);
                report.artifacts = artifacts;
                report
            }
            RunOutcome::TimedOut => {
                let mut report = RunReport::timed_out(logs);
                report.artifacts = artifacts;
                report
            }
            RunOutcome::Cancelled => {
                let mut report = RunReport::cancelled(logs);
                report.artifacts = artifacts;
                report
            }
        })
    }
}

impl KubernetesRunner {
    async fn collect_logs(
        &self,
        execution: &RunExecution,
    ) -> Result<Option<String>, KubernetesRunnerError> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);
        let job_name = kubernetes_job_name(execution.run.id(), execution.run.attempt_count());
        let list = pods
            .list(
                &ListParams::default().labels(&format!("batch.kubernetes.io/job-name={job_name}")),
            )
            .await?;
        let Some(pod) = list.items.first() else {
            return Ok(None);
        };
        let pod_name = pod.name_any();
        let logs = pods
            .logs(
                &pod_name,
                &LogParams {
                    container: Some("main".to_string()),
                    ..LogParams::default()
                },
            )
            .await?;

        Ok(Some(truncate_utf8(&logs, self.log_limit_bytes)))
    }
}

async fn wait_for_job(
    jobs: &Api<Job>,
    pods: &Api<Pod>,
    job_name: &str,
    execution: &RunExecution,
    timeout_seconds: u64,
    cancellation: &(impl CancellationCheck + Sync),
) -> Result<RunOutcome, KubernetesRunnerError> {
    let deadline = Instant::now() + Duration::from_secs(timeout_seconds);
    loop {
        if cancellation
            .is_cancelled(execution.run.id())
            .await
            .map_err(|error| KubernetesRunnerError::CancellationCheck(error.to_string()))?
        {
            cleanup_cancelled_job(jobs, pods, job_name).await;
            return Ok(RunOutcome::Cancelled);
        }

        let job = jobs.get(job_name).await?;
        if let Some(status) = job.status {
            if let Some(conditions) = status.conditions.as_ref() {
                let deadline_exceeded = conditions.iter().any(|condition| {
                    condition.type_ == "Failed"
                        && condition
                            .reason
                            .as_deref()
                            .is_some_and(|reason| reason == "DeadlineExceeded")
                });
                if deadline_exceeded {
                    return Ok(RunOutcome::TimedOut);
                }
            }
            if status.succeeded.unwrap_or_default() > 0 {
                return Ok(RunOutcome::Succeeded);
            }
            if status.failed.unwrap_or_default() > 0 {
                if Instant::now() >= deadline {
                    return Ok(RunOutcome::TimedOut);
                }
                return Ok(RunOutcome::Failed);
            }
        }

        if Instant::now() >= deadline {
            return Ok(RunOutcome::TimedOut);
        }

        sleep(POLL_INTERVAL).await;
    }
}

async fn cleanup_cancelled_job(jobs: &Api<Job>, pods: &Api<Pod>, job_name: &str) {
    let delete_params = DeleteParams {
        grace_period_seconds: Some(30),
        ..DeleteParams::default()
    };
    let _ = jobs.delete(job_name, &delete_params).await;

    let Ok(list) = pods
        .list(&ListParams::default().labels(&format!("batch.kubernetes.io/job-name={job_name}")))
        .await
    else {
        return;
    };

    for pod in list.items {
        let Some(name) = pod.metadata.name else {
            continue;
        };
        let _ = pods.delete(&name, &delete_params).await;
    }
}

/// Renders the Kubernetes Job for an execution.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn build_job(execution: &RunExecution, namespace: &str) -> Job {
    let job_name = kubernetes_job_name(execution.run.id(), execution.run.attempt_count());
    let run_key = run_label_value(execution.run.id());
    let mut labels = BTreeMap::new();
    labels.insert(APP_LABEL.to_string(), "capsulet".to_string());
    labels.insert(RUN_LABEL.to_string(), run_key);
    labels.insert(
        ATTEMPT_LABEL.to_string(),
        execution.run.attempt_count().to_string(),
    );

    Job {
        metadata: ObjectMeta {
            name: Some(job_name),
            namespace: Some(namespace.to_string()),
            labels: Some(labels.clone()),
            ..ObjectMeta::default()
        },
        spec: Some(JobSpec {
            active_deadline_seconds: Some(
                i64::try_from(execution.pool.timeout_seconds)
                    .unwrap_or(DEFAULT_JOB_TIMEOUT_SECONDS_I64),
            ),
            backoff_limit: Some(0),
            ttl_seconds_after_finished: execution.pool.ttl_seconds_after_finished,
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(labels),
                    ..ObjectMeta::default()
                }),
                spec: Some(PodSpec {
                    restart_policy: Some("Never".to_string()),
                    node_selector: if execution.pool.node_selector.is_empty() {
                        None
                    } else {
                        Some(execution.pool.node_selector.clone())
                    },
                    tolerations: if execution.pool.tolerations.is_empty() {
                        None
                    } else {
                        Some(
                            execution
                                .pool
                                .tolerations
                                .iter()
                                .map(PoolToleration::to_kubernetes)
                                .collect(),
                        )
                    },
                    containers: vec![Container {
                        name: "main".to_string(),
                        image: Some(execution.definition.runtime_image().to_string()),
                        command: Some(wrapped_command(
                            execution.definition.command(),
                            execution.definition.python_dependencies(),
                            &execution.input_artifacts,
                        )),
                        env: Some(vec![EnvVar {
                            name: "CAPSULET_INPUT_JSON".to_string(),
                            value: Some(execution.run.input_json().to_string()),
                            ..EnvVar::default()
                        }]),
                        resources: Some(execution.pool.resources.to_kubernetes()),
                        security_context: Some(SecurityContext {
                            allow_privilege_escalation: Some(false),
                            capabilities: Some(Capabilities {
                                drop: Some(vec!["ALL".to_string()]),
                                ..Capabilities::default()
                            }),
                            privileged: Some(false),
                            read_only_root_filesystem: Some(true),
                            run_as_group: Some(10_001),
                            run_as_non_root: Some(true),
                            run_as_user: Some(10_001),
                            seccomp_profile: Some(SeccompProfile {
                                type_: "RuntimeDefault".to_string(),
                                ..SeccompProfile::default()
                            }),
                            ..SecurityContext::default()
                        }),
                        volume_mounts: Some(vec![
                            VolumeMount {
                                name: "capsulet-data".to_string(),
                                mount_path: "/capsulet".to_string(),
                                ..VolumeMount::default()
                            },
                            VolumeMount {
                                name: "tmp".to_string(),
                                mount_path: "/tmp".to_string(),
                                ..VolumeMount::default()
                            },
                        ]),
                        ..Container::default()
                    }],
                    automount_service_account_token: Some(false),
                    runtime_class_name: execution.pool.runtime_class_name.clone(),
                    service_account_name: execution.pool.service_account_name.clone(),
                    enable_service_links: Some(false),
                    host_ipc: Some(false),
                    host_network: Some(false),
                    host_pid: Some(false),
                    security_context: Some(PodSecurityContext {
                        fs_group: Some(10_001),
                        run_as_group: Some(10_001),
                        run_as_non_root: Some(true),
                        run_as_user: Some(10_001),
                        seccomp_profile: Some(SeccompProfile {
                            type_: "RuntimeDefault".to_string(),
                            ..SeccompProfile::default()
                        }),
                        ..PodSecurityContext::default()
                    }),
                    volumes: Some(vec![
                        Volume {
                            name: "capsulet-data".to_string(),
                            empty_dir: Some(EmptyDirVolumeSource {
                                size_limit: execution
                                    .pool
                                    .resources
                                    .limits
                                    .get("ephemeral-storage")
                                    .cloned()
                                    .map(Quantity),
                                ..EmptyDirVolumeSource::default()
                            }),
                            ..Volume::default()
                        },
                        Volume {
                            name: "tmp".to_string(),
                            empty_dir: Some(EmptyDirVolumeSource::default()),
                            ..Volume::default()
                        },
                    ]),
                    ..PodSpec::default()
                }),
            },
            ..JobSpec::default()
        }),
        ..Job::default()
    }
}

fn wrapped_command(
    command: &[String],
    python_dependencies: &[String],
    input_artifacts: &[InputArtifact],
) -> Vec<String> {
    vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        wrapper_script(command, python_dependencies, input_artifacts),
    ]
}

fn wrapper_script(
    command: &[String],
    python_dependencies: &[String],
    input_artifacts: &[InputArtifact],
) -> String {
    let command = command
        .iter()
        .map(|part| shell_quote(part))
        .collect::<Vec<_>>()
        .join(" ");
    let dependency_install = if python_dependencies.is_empty() {
        String::new()
    } else {
        format!(
            "mkdir -p /capsulet/python-site\npython -m pip install --disable-pip-version-check --no-input --break-system-packages --target /capsulet/python-site {}\nexport PYTHONPATH=\"/capsulet/python-site:${{PYTHONPATH:-}}\"",
            python_dependencies
                .iter()
                .map(|dependency| shell_quote(dependency))
                .collect::<Vec<_>>()
                .join(" ")
        )
    };
    let inputs = input_artifacts
        .iter()
        .map(|artifact| {
            let path = format!(
                "{INPUT_DIR}/{}/{}",
                artifact.producer_step_id, artifact.name
            );
            let parent = format!("{INPUT_DIR}/{}", artifact.producer_step_id);
            let alias = format!("{INPUT_DIR}/{}", artifact.name);
            let encoded = BASE64.encode(&artifact.bytes);
            format!(
                "mkdir -p {}\nprintf '%s' {} | base64 -d > {}\ncp {} {}",
                shell_quote(&parent),
                shell_quote(&encoded),
                shell_quote(&path),
                shell_quote(&path),
                shell_quote(&alias),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"mkdir -p /capsulet
printf '%s' "$CAPSULET_INPUT_JSON" > /capsulet/input.json
{inputs}
{dependency_install}
{command}
status=$?
if [ -d {ARTIFACT_DIR} ]; then
  find {ARTIFACT_DIR} -maxdepth 1 -type f | sort | while IFS= read -r file; do
    name=$(basename "$file")
    encoded=$(base64 < "$file" | tr -d '\n')
    printf '{ARTIFACT_MARKER}\t%s\tapplication/octet-stream\t%s\n' "$name" "$encoded"
  done
fi
exit $status"#
    )
}

fn input_artifact_path(artifact: &InputArtifact) -> std::path::PathBuf {
    Path::new(INPUT_DIR)
        .join(&artifact.producer_step_id)
        .join(&artifact.name)
}

fn materialize_local_inputs(artifacts: &[InputArtifact]) -> Result<(), std::io::Error> {
    let root = Path::new(INPUT_DIR);
    if root.exists() {
        fs::remove_dir_all(root)?;
    }
    for artifact in artifacts {
        let path = input_artifact_path(artifact);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, &artifact.bytes)?;
        fs::write(root.join(&artifact.name), &artifact.bytes)?;
    }
    Ok(())
}

fn collect_artifacts_from(
    path: impl AsRef<Path>,
) -> Result<Vec<CollectedArtifact>, std::io::Error> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut artifacts = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        artifacts.push(CollectedArtifact {
            name: entry.file_name().to_string_lossy().into_owned(),
            content_type: "application/octet-stream".to_string(),
            bytes: fs::read(entry.path())?,
        });
    }
    Ok(artifacts)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn split_artifact_markers(logs: Option<String>) -> (Option<String>, Vec<CollectedArtifact>) {
    let Some(logs) = logs else {
        return (None, Vec::new());
    };
    let mut cleaned = Vec::new();
    let mut artifacts = Vec::new();
    for line in logs.lines() {
        let parts = line.splitn(4, '\t').collect::<Vec<_>>();
        if parts.len() == 4 && parts[0] == ARTIFACT_MARKER {
            if let Ok(bytes) = BASE64.decode(parts[3]) {
                artifacts.push(CollectedArtifact {
                    name: parts[1].to_string(),
                    content_type: parts[2].to_string(),
                    bytes,
                });
            }
            continue;
        }
        cleaned.push(line);
    }
    let cleaned = cleaned.join("\n");
    if cleaned.is_empty() {
        (None, artifacts)
    } else {
        (Some(format!("{cleaned}\n")), artifacts)
    }
}

fn reset_artifact_dir() -> Result<(), std::io::Error> {
    let path = Path::new(ARTIFACT_DIR);
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    fs::create_dir_all(path)
}

fn collect_local_artifacts() -> Result<Vec<CollectedArtifact>, std::io::Error> {
    collect_artifacts_from(ARTIFACT_DIR)
}

impl PoolResources {
    fn to_kubernetes(&self) -> ResourceRequirements {
        ResourceRequirements {
            requests: map_quantities(&self.requests),
            limits: map_quantities(&self.limits),
            ..ResourceRequirements::default()
        }
    }
}

impl PoolToleration {
    fn to_kubernetes(&self) -> Toleration {
        Toleration {
            key: self.key.clone(),
            operator: self.operator.clone(),
            value: self.value.clone(),
            effect: self.effect.clone(),
            ..Toleration::default()
        }
    }
}

fn map_quantities(values: &BTreeMap<String, String>) -> Option<BTreeMap<String, Quantity>> {
    if values.is_empty() {
        return None;
    }
    Some(
        values
            .iter()
            .map(|(key, value)| (key.clone(), Quantity(value.clone())))
            .collect(),
    )
}

fn default_timeout_seconds() -> u64 {
    DEFAULT_JOB_TIMEOUT_SECONDS
}

fn kubernetes_job_name(run_id: &JobRunId, attempt: u32) -> String {
    let suffix = sanitize_kubernetes_segment(run_id.as_str(), 45);
    format!("capsulet-{suffix}-a{attempt}")
}

fn validate_job_identity(job: &Job, execution: &RunExecution) -> Result<(), KubernetesRunnerError> {
    let labels = job
        .metadata
        .labels
        .as_ref()
        .ok_or(KubernetesRunnerError::JobIdentityConflict)?;
    let run_key = run_label_value(execution.run.id());
    let attempt = execution.run.attempt_count().to_string();
    if labels.get(RUN_LABEL) != Some(&run_key) || labels.get(ATTEMPT_LABEL) != Some(&attempt) {
        return Err(KubernetesRunnerError::JobIdentityConflict);
    }
    Ok(())
}

fn run_label_value(run_id: &JobRunId) -> String {
    sanitize_kubernetes_segment(run_id.as_str(), 63)
}

fn sanitize_kubernetes_segment(value: &str, max_len: usize) -> String {
    let mut output = String::new();
    for character in value.chars() {
        let next = if character.is_ascii_alphanumeric() || matches!(character, '-' | '.') {
            character.to_ascii_lowercase()
        } else {
            '-'
        };
        output.push(next);
        if output.len() == max_len {
            break;
        }
    }

    while output.starts_with(['-', '.']) {
        output.remove(0);
    }
    while output.ends_with(['-', '.']) {
        output.pop();
    }

    if output.is_empty() {
        "run".to_string()
    } else {
        output
    }
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }

    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

/// Kubernetes runner error.
#[derive(Debug, Error)]
pub enum KubernetesRunnerError {
    #[error("kubernetes client error: {0}")]
    Kube(#[from] kube::Error),
    #[error("rendered Kubernetes Job is missing a name")]
    MissingJobName,
    #[error("an existing Kubernetes Job has conflicting Capsulet run identity")]
    JobIdentityConflict,
    #[error("execution policy rejected run: {0}")]
    Policy(String),
    #[error("cancellation check failed: {0}")]
    CancellationCheck(String),
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
    #[error("wasmtime failed to run Python script: {0}")]
    Io(#[from] std::io::Error),
    #[error("WASI Python execution timed out for process {process_id:?}")]
    TimedOut { process_id: Option<u32> },
    #[error("cancellation check failed: {0}")]
    CancellationCheck(String),
    #[error("execution policy rejected run: {0}")]
    Policy(String),
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
