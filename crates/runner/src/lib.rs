use std::{collections::BTreeMap, fs, path::Path, process::Stdio, time::Duration};

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

        reset_artifact_dir()?;
        materialize_local_inputs(&execution.input_artifacts)?;
        let Some((program, args)) = execution.definition.command().split_first() else {
            return Err(ProcessRunnerError::EmptyCommand);
        };
        let mut child = Command::new(local_program(program))
            .args(args)
            .env("CAPSULET_INPUT_JSON", execution.run.input_json())
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

#[derive(Debug, Error)]
pub enum ProcessRunnerError {
    #[error("process command cannot be empty")]
    EmptyCommand,
    #[error("process execution failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("cancellation check failed: {0}")]
    CancellationCheck(String),
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

fn wrapped_command(command: &[String], input_artifacts: &[InputArtifact]) -> Vec<String> {
    vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        wrapper_script(command, input_artifacts),
    ]
}

fn wrapper_script(command: &[String], input_artifacts: &[InputArtifact]) -> String {
    let command = command
        .iter()
        .map(|part| shell_quote(part))
        .collect::<Vec<_>>()
        .join(" ");
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
    let path = Path::new(ARTIFACT_DIR);
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
    #[error("cancellation check failed: {0}")]
    CancellationCheck(String),
}

#[cfg(test)]
mod tests;
