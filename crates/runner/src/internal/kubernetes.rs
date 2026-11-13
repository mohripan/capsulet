//! A runner that submits each run as a Kubernetes Job and waits for it.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::OnceLock,
    time::Duration,
};

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use capsulet_core::JobRunId;
use k8s_openapi::{
    api::{
        batch::v1::{Job, JobSpec},
        core::v1::{
            Capabilities, Container, EmptyDirVolumeSource, EnvVar, Pod, PodSecurityContext,
            PodSpec, PodTemplateSpec, SeccompProfile, SecurityContext, Volume, VolumeMount,
        },
    },
    apimachinery::pkg::{api::resource::Quantity, apis::meta::v1::ObjectMeta},
    jiff::Timestamp,
};
use kube::{
    Api, Client, ResourceExt,
    api::{DeleteParams, ListParams, LogParams, PostParams},
};
use thiserror::Error;
use tokio::time::{Instant, sleep};

use super::artifacts::{ARTIFACT_DIR, ARTIFACT_MARKER, INPUT_DIR, split_artifact_markers};
use super::contract::{
    CancellationCheck, InputArtifact, RunExecution, RunOutcome, RunReport, Runner,
};
use super::pools::{PoolToleration, validate_execution_policy};

pub(super) const APP_LABEL: &str = "capsulet.dev/managed-by";

pub(super) const RUN_LABEL: &str = "capsulet.dev/job-run-key";

pub(super) const ATTEMPT_LABEL: &str = "capsulet.dev/attempt";

pub(super) const COMPONENT_LABEL: &str = "capsulet.dev/component";

pub(super) const DEFAULT_COMPONENT: &str = "worker";

pub(super) const DEFAULT_JOB_TIMEOUT_SECONDS_I64: i64 = 300;

pub(super) const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Jobs younger than this are never reclaimed: another worker may have created
/// one after the caller took its snapshot of the active runs.
pub(super) const ORPHAN_GRACE_SECONDS: i64 = 120;

/// Ceiling on the pod log fetch so a chatty job cannot drive worker memory.
pub(super) const MAX_LOG_FETCH_BYTES: usize = 32 * 1024 * 1024;

/// Input artifacts are inlined into a single `/bin/sh -c` argument, which Linux
/// caps at `MAX_ARG_STRLEN` (128 KiB); the remaining headroom carries the script.
pub(super) const MAX_INLINE_INPUT_ARTIFACT_BYTES: usize = 96 * 1024;

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
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let client = Client::try_default().await?;
        Ok(Self::new(client, namespace, log_limit_bytes))
    }

    /// Deletes Kubernetes Jobs created by this component whose run id is no longer
    /// active in storage.
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
            .list(&ListParams::default().labels(&format!(
                "{APP_LABEL}=capsulet,{COMPONENT_LABEL}={}",
                component_label_value()
            )))
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
            if active_labels.contains(run_label) || job_within_grace_period(&job) {
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
        validate_input_artifact_budget(&execution.input_artifacts)?;
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
        let logs = logs.map(|logs| truncate_utf8(&logs, self.log_limit_bytes));

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
                    limit_bytes: Some(log_fetch_limit(self.log_limit_bytes)),
                    ..LogParams::default()
                },
            )
            .await?;

        // Artifacts are multiplexed into this stream, so the log budget is applied
        // only after the markers have been split out.
        Ok(Some(logs))
    }
}

pub(super) fn log_fetch_limit(log_limit_bytes: usize) -> i64 {
    i64::try_from(log_limit_bytes.saturating_add(MAX_LOG_FETCH_BYTES)).unwrap_or(i64::MAX)
}

pub(super) fn validate_input_artifact_budget(
    artifacts: &[InputArtifact],
) -> Result<(), KubernetesRunnerError> {
    let encoded_bytes = artifacts.iter().fold(0_usize, |total, artifact| {
        total.saturating_add(base64_encoded_len(artifact.bytes.len()))
    });
    if encoded_bytes > MAX_INLINE_INPUT_ARTIFACT_BYTES {
        return Err(KubernetesRunnerError::InputArtifactsTooLarge {
            encoded_bytes,
            limit_bytes: MAX_INLINE_INPUT_ARTIFACT_BYTES,
        });
    }
    Ok(())
}

pub(super) const fn base64_encoded_len(bytes: usize) -> usize {
    bytes.div_ceil(3).saturating_mul(4)
}

pub(super) async fn wait_for_job(
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

pub(super) async fn cleanup_cancelled_job(jobs: &Api<Job>, pods: &Api<Pod>, job_name: &str) {
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
pub fn build_job(execution: &RunExecution, namespace: &str) -> Job {
    let job_name = kubernetes_job_name(execution.run.id(), execution.run.attempt_count());
    let labels = job_labels(execution);
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
                spec: Some(job_pod_spec(execution)),
            },
            ..JobSpec::default()
        }),
        ..Job::default()
    }
}

/// The labels a run's Job and its pod carry, which is how the runner finds them again.
pub(super) fn job_labels(execution: &RunExecution) -> BTreeMap<String, String> {
    let mut labels = BTreeMap::new();
    labels.insert(APP_LABEL.to_string(), "capsulet".to_string());
    labels.insert(
        COMPONENT_LABEL.to_string(),
        component_label_value().to_string(),
    );
    labels.insert(RUN_LABEL.to_string(), run_label_value(execution.run.id()));
    labels.insert(
        ATTEMPT_LABEL.to_string(),
        execution.run.attempt_count().to_string(),
    );
    labels
}

/// The pod a run's Job starts: placed by its pool, with no host access, no
/// service-account token, and only the scratch volumes its container mounts.
pub(super) fn job_pod_spec(execution: &RunExecution) -> PodSpec {
    let pool = &execution.pool;
    PodSpec {
        restart_policy: Some("Never".to_string()),
        node_selector: (!pool.node_selector.is_empty()).then(|| pool.node_selector.clone()),
        tolerations: (!pool.tolerations.is_empty()).then(|| {
            pool.tolerations
                .iter()
                .map(PoolToleration::to_kubernetes)
                .collect()
        }),
        containers: vec![job_container(execution)],
        automount_service_account_token: Some(false),
        runtime_class_name: pool.runtime_class_name.clone(),
        service_account_name: pool.service_account_name.clone(),
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
                    size_limit: pool
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
    }
}

/// The run's one container: its image and wrapped command, run unprivileged
/// as a fixed non-root user on a read-only root filesystem.
pub(super) fn job_container(execution: &RunExecution) -> Container {
    Container {
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
    }
}

pub(super) fn wrapped_command(
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

pub(super) fn wrapper_script(
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

pub(super) fn input_artifact_path(artifact: &InputArtifact) -> std::path::PathBuf {
    Path::new(INPUT_DIR)
        .join(&artifact.producer_step_id)
        .join(&artifact.name)
}

pub(super) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

pub(super) fn map_quantities(
    values: &BTreeMap<String, String>,
) -> Option<BTreeMap<String, Quantity>> {
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

pub(super) fn kubernetes_job_name(run_id: &JobRunId, attempt: u32) -> String {
    let suffix = sanitize_kubernetes_segment(run_id.as_str(), 45);
    format!("capsulet-{suffix}-a{attempt}")
}

pub(super) fn validate_job_identity(
    job: &Job,
    execution: &RunExecution,
) -> Result<(), KubernetesRunnerError> {
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

pub(super) fn run_label_value(run_id: &JobRunId) -> String {
    sanitize_kubernetes_segment(run_id.as_str(), 63)
}

/// Identity of the process rendering a Job, so that a reconciler only reclaims
/// Jobs its own component created. The evaluator runs custom trigger plugins
/// through the same renderer, but their runs are never persisted and would
/// otherwise look orphaned to the worker.
pub(super) fn component_label_value() -> &'static str {
    static COMPONENT: OnceLock<String> = OnceLock::new();
    COMPONENT.get_or_init(|| {
        let name = std::env::var("CAPSULET_COMPONENT")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                std::env::current_exe().ok().and_then(|path| {
                    path.file_stem()
                        .and_then(std::ffi::OsStr::to_str)
                        .map(str::to_string)
                })
            })
            .unwrap_or_else(|| DEFAULT_COMPONENT.to_string());
        sanitize_kubernetes_segment(&name, 63)
    })
}

pub(super) fn job_within_grace_period(job: &Job) -> bool {
    job.metadata
        .creation_timestamp
        .as_ref()
        .is_some_and(|created| {
            Timestamp::now().duration_since(created.0).as_secs() < ORPHAN_GRACE_SECONDS
        })
}

pub(super) fn sanitize_kubernetes_segment(value: &str, max_len: usize) -> String {
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

pub(super) fn truncate_utf8(value: &str, max_bytes: usize) -> String {
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
    #[error(
        "input artifacts need {encoded_bytes} encoded bytes in the job command, which exceeds the {limit_bytes} byte limit"
    )]
    InputArtifactsTooLarge {
        encoded_bytes: usize,
        limit_bytes: usize,
    },
}

#[cfg(test)]
mod untrusted_input_tests {
    use super::super::artifacts::split_artifact_markers;
    use super::super::contract::{CollectedArtifact, InputArtifact};
    use super::{MAX_INLINE_INPUT_ARTIFACT_BYTES, validate_input_artifact_budget};
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

    fn marker(name: &str, payload: &str) -> String {
        format!(
            "CAPSULET_ARTIFACT\t{name}\ttext/plain\t{}\n",
            BASE64.encode(payload)
        )
    }

    #[test]
    fn rejects_artifact_names_that_are_not_plain_file_names() {
        for name in ["..", ".", "reports/out.txt", "a\\b.txt", "", "a\u{7}b"] {
            let (logs, artifacts) = split_artifact_markers(Some(marker(name, "hi")));

            assert!(artifacts.is_empty(), "{name} should be rejected");
            assert!(
                logs.unwrap_or_default().contains("dropped artifact marker"),
                "{name} rejection should be reported in the logs"
            );
        }
    }

    #[test]
    fn keeps_plain_artifact_names() {
        let (_, artifacts) = split_artifact_markers(Some(marker("report.txt", "hi")));

        assert_eq!(
            artifacts,
            vec![CollectedArtifact {
                name: "report.txt".to_string(),
                content_type: "text/plain".to_string(),
                bytes: b"hi".to_vec(),
            }]
        );
    }

    #[test]
    fn drops_artifact_whose_payload_lost_its_line_terminator() {
        let complete = marker("report.txt", "hello world");
        let cut = complete.trim_end_matches('\n');

        let (logs, artifacts) = split_artifact_markers(Some(cut.to_string()));

        assert_eq!(artifacts, vec![]);
        assert!(
            logs.unwrap_or_default()
                .contains("cut off by the log limit")
        );
    }

    #[test]
    fn rejects_input_artifacts_that_overflow_the_job_command() {
        let artifacts = vec![InputArtifact {
            producer_step_id: "generate".to_string(),
            name: "big.csv".to_string(),
            bytes: vec![0; MAX_INLINE_INPUT_ARTIFACT_BYTES],
        }];

        let error =
            validate_input_artifact_budget(&artifacts).expect_err("oversized inputs are rejected");

        assert!(error.to_string().contains("exceeds"), "{error}");
    }

    #[test]
    fn accepts_input_artifacts_within_the_job_command_budget() {
        let artifacts = vec![InputArtifact {
            producer_step_id: "generate".to_string(),
            name: "small.csv".to_string(),
            bytes: vec![0; 1024],
        }];

        assert!(validate_input_artifact_budget(&artifacts).is_ok());
    }
}
