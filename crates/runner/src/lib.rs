use std::{collections::BTreeMap, time::Duration};

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use capsulet_core::{JobDefinition, JobRun, JobRunId};
use k8s_openapi::{
    api::{
        batch::v1::{Job, JobSpec},
        core::v1::{Container, Pod, PodSpec, PodTemplateSpec, ResourceRequirements, Toleration},
    },
    apimachinery::pkg::{api::resource::Quantity, apis::meta::v1::ObjectMeta},
};
use kube::{
    Api, Client, ResourceExt,
    api::{DeleteParams, ListParams, LogParams, PostParams},
};
use serde::Deserialize;
use thiserror::Error;
use tokio::time::{Instant, sleep};

const APP_LABEL: &str = "capsulet.dev/managed-by";
const RUN_LABEL: &str = "capsulet.dev/job-run-key";
const DEFAULT_JOB_TIMEOUT_SECONDS: u64 = 300;
const DEFAULT_JOB_TIMEOUT_SECONDS_I64: i64 = 300;
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const ARTIFACT_MARKER: &str = "CAPSULET_ARTIFACT";
const ARTIFACT_DIR: &str = "/capsulet/artifacts";

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

/// Complete execution input for a leased job run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunExecution {
    pub run: JobRun,
    pub definition: JobDefinition,
    pub pool: ExecutionPoolConfig,
}

/// Boundary for executing a leased job run.
#[async_trait]
pub trait Runner: Clone + Send + Sync + 'static {
    type Error: std::fmt::Display + Send + Sync + 'static;

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
            .is_cancelled(&execution.run.id)
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
            Err(kube::Error::Api(error)) if error.code == 409 => {}
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
        let run_key = run_label_value(&execution.run.id);
        let list = pods
            .list(&ListParams::default().labels(&format!("{RUN_LABEL}={run_key}")))
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
            .is_cancelled(&execution.run.id)
            .await
            .map_err(|error| KubernetesRunnerError::CancellationCheck(error.to_string()))?
        {
            cleanup_cancelled_job(jobs, pods, job_name, &execution.run.id).await;
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

async fn cleanup_cancelled_job(
    jobs: &Api<Job>,
    pods: &Api<Pod>,
    job_name: &str,
    run_id: &JobRunId,
) {
    let _ = jobs.delete(job_name, &DeleteParams::default()).await;

    let run_key = run_label_value(run_id);
    let Ok(list) = pods
        .list(&ListParams::default().labels(&format!("{RUN_LABEL}={run_key}")))
        .await
    else {
        return;
    };

    for pod in list.items {
        let Some(name) = pod.metadata.name else {
            continue;
        };
        let _ = pods.delete(&name, &DeleteParams::default()).await;
    }
}

/// Renders the Kubernetes Job for an execution.
#[must_use]
pub fn build_job(execution: &RunExecution, namespace: &str) -> Job {
    let job_name = kubernetes_job_name(&execution.run.id);
    let run_key = run_label_value(&execution.run.id);
    let mut labels = BTreeMap::new();
    labels.insert(APP_LABEL.to_string(), "capsulet".to_string());
    labels.insert(RUN_LABEL.to_string(), run_key);

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
                        image: Some(execution.definition.runtime_image.clone()),
                        command: Some(wrapped_command(&execution.definition.command)),
                        resources: Some(execution.pool.resources.to_kubernetes()),
                        ..Container::default()
                    }],
                    ..PodSpec::default()
                }),
            },
            ..JobSpec::default()
        }),
        ..Job::default()
    }
}

fn wrapped_command(command: &[String]) -> Vec<String> {
    vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        wrapper_script(command),
    ]
}

fn wrapper_script(command: &[String]) -> String {
    let command = command
        .iter()
        .map(|part| shell_quote(part))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        r#"{command}
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

fn kubernetes_job_name(run_id: &JobRunId) -> String {
    let suffix = sanitize_kubernetes_segment(run_id.as_str(), 54);
    format!("capsulet-{suffix}")
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
    #[error("cancellation check failed: {0}")]
    CancellationCheck(String),
}

#[cfg(test)]
mod tests {
    use base64::Engine as _;
    use capsulet_core::{ExecutionPoolName, JobDefinition, JobRun, JobRunId};

    use super::{
        ExecutionPoolConfig, ExecutionPoolsConfig, PoolResources, PoolToleration, RUN_LABEL,
        RunExecution, build_job, kubernetes_job_name, run_label_value, truncate_utf8,
    };

    fn execution(pool: ExecutionPoolConfig) -> RunExecution {
        RunExecution {
            run: JobRun::new(
                JobRunId::new("run_hello_python").expect("valid run id"),
                JobDefinition::hello_python().id,
                ExecutionPoolName::new("mini").expect("valid pool"),
            ),
            definition: JobDefinition::hello_python(),
            pool,
        }
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
        }
    }

    #[test]
    fn renders_job_metadata_and_container() {
        let job = build_job(&execution(pool()), "capsulet-exec");

        assert_eq!(
            job.metadata.name.as_deref(),
            Some("capsulet-run-hello-python")
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
        let expected = run_label_value(&execution.run.id);
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
        let name = kubernetes_job_name(&JobRunId::new("Run_With Spaces").expect("valid id"));

        assert_eq!(name, "capsulet-run-with-spaces");
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
}
