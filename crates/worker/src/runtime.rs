use std::{
    collections::hash_map::DefaultHasher,
    env, fs,
    hash::{Hash, Hasher},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context as _, anyhow, bail};
use axum::{Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};
use capsulet_core::{ComponentDescriptor, ComponentKind};
use capsulet_observability as observability;
use capsulet_postgres::{PostgresPoolConfig, PostgresStore};
use capsulet_runner::{ExecutionPoolsConfig, KubernetesRunner, ProcessRunner, Runner, StubRunner};
use capsulet_storage::ConfiguredObjectStore;
use tokio::task::JoinSet;

use crate::execute_one_queued_run;

const DEFAULT_WORKER_ID: &str = "worker-local";
const DEFAULT_LEASE_SECONDS: i64 = 60;
const DEFAULT_POLL_SECONDS: u64 = 5;
const DEFAULT_MAX_CONCURRENT_RUNS: usize = 1;
const DEFAULT_K8S_RECONCILE_SECONDS: u64 = 60;
const DEFAULT_RUNNER_MODE: &str = "stub";
const DEFAULT_EXECUTION_NAMESPACE: &str = "default";
const DEFAULT_LOG_LIMIT_BYTES: usize = 64 * 1024;
const DEFAULT_OBJECT_STORAGE_PATH: &str = ".capsulet-objects";
const DEFAULT_HEALTH_ADDR: &str = "0.0.0.0:8081";
const DEFAULT_EXECUTION_POOLS_YAML: &str = r#"
defaultPool: mini
pools:
  mini:
    description: Lightweight jobs such as email, webhooks, and small scripts
    nodeSelector: {}
    tolerations: []
    resources:
      requests:
        cpu: 100m
        memory: 128Mi
      limits:
        cpu: 500m
        memory: 512Mi
    timeoutSeconds: 120
    maxConcurrentJobs: 50
    ttlSecondsAfterFinished: 300
  large:
    description: Compute-heavy jobs such as model inference and batch processing
    nodeSelector: {}
    tolerations: []
    resources:
      requests:
        cpu: "2"
        memory: 4Gi
      limits:
        cpu: "8"
        memory: 16Gi
    timeoutSeconds: 3600
    maxConcurrentJobs: 10
    ttlSecondsAfterFinished: 300
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunnerMode {
    Kubernetes,
    Stub,
    Process,
}

/// Runs the worker service from environment configuration.
///
/// # Errors
///
/// Returns an error when required environment variables are missing, database
/// setup fails, object storage or execution pools cannot be configured, or a
/// worker tick fails.
#[allow(clippy::too_many_lines)]
pub async fn run() -> anyhow::Result<()> {
    observability::init("capsulet-worker")
        .map_err(|error| anyhow!("{error}"))
        .context("initialize worker observability")?;
    let descriptor = ComponentDescriptor::new(
        ComponentKind::Worker,
        "leases queued job runs and coordinates execution",
    );
    observability::tracing::info!(component = "worker", banner = %descriptor.banner());

    let database_url = env::var("CAPSULET_DATABASE_URL")
        .or_else(|_| env::var("DATABASE_URL"))
        .context("set CAPSULET_DATABASE_URL or DATABASE_URL before starting capsulet-worker")?;
    let worker_id =
        env::var("CAPSULET_WORKER_ID").unwrap_or_else(|_| DEFAULT_WORKER_ID.to_string());
    let lease_seconds = env::var("CAPSULET_WORKER_LEASE_SECONDS")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(DEFAULT_LEASE_SECONDS);
    let pools = load_execution_pools()?;

    let store = PostgresStore::connect_with_config(&database_url, PostgresPoolConfig::from_env()?)
        .await
        .context("connect worker to Postgres")?;
    store.migrate().await.context("run worker migrations")?;
    start_health_server(
        store.clone(),
        &env::var("CAPSULET_WORKER_HEALTH_ADDR")
            .unwrap_or_else(|_| DEFAULT_HEALTH_ADDR.to_string()),
    )
    .await?;
    let object_store = load_object_store()?;

    let runner_mode =
        env::var("CAPSULET_RUNNER_MODE").unwrap_or_else(|_| DEFAULT_RUNNER_MODE.to_string());
    let loop_enabled = env_bool("CAPSULET_WORKER_LOOP");
    let poll_seconds = env::var("CAPSULET_WORKER_POLL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_POLL_SECONDS);
    let max_concurrent_runs = env::var("CAPSULET_WORKER_MAX_CONCURRENT_RUNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_CONCURRENT_RUNS);

    match parse_runner_mode(&runner_mode)? {
        RunnerMode::Kubernetes => {
            let namespace = env::var("CAPSULET_EXECUTION_NAMESPACE")
                .unwrap_or_else(|_| DEFAULT_EXECUTION_NAMESPACE.to_string());
            let log_limit_bytes = env::var("CAPSULET_LOG_LIMIT_BYTES")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(DEFAULT_LOG_LIMIT_BYTES);
            let runner = KubernetesRunner::from_default_config(namespace, log_limit_bytes)
                .await
                .context("initialize Kubernetes runner")?;
            start_kubernetes_reconciler(store.clone(), runner.clone(), &worker_id);
            run_loop(
                store,
                object_store,
                pools,
                worker_id,
                lease_seconds,
                poll_seconds,
                max_concurrent_runs,
                loop_enabled,
                runner,
            )
            .await?;
        }
        RunnerMode::Stub => {
            let runner = match env::var("CAPSULET_STUB_RUNNER_RESULT").as_deref() {
                Ok("failed" | "failure") => StubRunner::failure(),
                _ => match (
                    env::var("CAPSULET_STUB_RUNNER_LOGS").ok(),
                    env::var("CAPSULET_STUB_ARTIFACT_TEXT").ok(),
                ) {
                    (Some(logs), Some(artifact)) => {
                        StubRunner::success_with_logs_and_artifact(logs, artifact)
                    }
                    (Some(logs), None) => StubRunner::success_with_logs(logs),
                    (None, Some(artifact)) => StubRunner::success_with_artifact(artifact),
                    (None, None) => StubRunner::success(),
                },
            };
            run_loop(
                store,
                object_store,
                pools,
                worker_id,
                lease_seconds,
                poll_seconds,
                max_concurrent_runs,
                loop_enabled,
                runner,
            )
            .await?;
        }
        RunnerMode::Process => {
            run_loop(
                store,
                object_store,
                pools,
                worker_id,
                lease_seconds,
                poll_seconds,
                max_concurrent_runs,
                loop_enabled,
                ProcessRunner,
            )
            .await?;
        }
    }

    Ok(())
}

fn start_kubernetes_reconciler(store: PostgresStore, runner: KubernetesRunner, worker_id: &str) {
    let interval_seconds = env::var("CAPSULET_K8S_RECONCILE_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_K8S_RECONCILE_SECONDS);
    let worker_id = worker_id.to_string();
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(jittered_poll_duration(&worker_id, interval_seconds));
        loop {
            interval.tick().await;
            match store.active_leased_run_ids().await {
                Ok(active_run_ids) => match runner.reconcile_orphaned_jobs(&active_run_ids).await {
                    Ok(deleted) if deleted > 0 => {
                        observability::tracing::info!(
                            deleted,
                            "kubernetes reconciler deleted orphaned jobs"
                        );
                    }
                    Ok(_) => {}
                    Err(error) => {
                        observability::tracing::warn!(%error, "kubernetes reconciler failed");
                    }
                },
                Err(error) => {
                    observability::tracing::warn!(
                        %error,
                        "kubernetes reconciler could not list active runs"
                    );
                }
            }
        }
    });
}

async fn start_health_server(store: PostgresStore, address: &str) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/livez", get(|| async { StatusCode::OK }))
        .route("/healthz", get(ready))
        .route("/readyz", get(ready))
        .route("/metrics", get(metrics))
        .with_state(store);
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("bind worker health listener on {address}"))?;
    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            observability::tracing::warn!(%error, "worker health server stopped");
        }
    });
    Ok(())
}

async fn ready(State(store): State<PostgresStore>) -> StatusCode {
    match store.ping().await {
        Ok(()) => StatusCode::OK,
        Err(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

async fn metrics(State(store): State<PostgresStore>) -> axum::response::Response {
    match store.prometheus_metrics().await {
        Ok(db_body) => {
            let mut body = observability::render_metrics();
            if !body.is_empty() && !body.ends_with('\n') {
                body.push('\n');
            }
            body.push_str(&db_body);
            ([("content-type", "text/plain; version=0.0.4")], body).into_response()
        }
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_loop<R>(
    store: PostgresStore,
    object_store: ConfiguredObjectStore,
    pools: ExecutionPoolsConfig,
    worker_id: String,
    lease_seconds: i64,
    poll_seconds: u64,
    max_concurrent_runs: usize,
    loop_enabled: bool,
    runner: R,
) -> anyhow::Result<()>
where
    R: Runner,
{
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);
    let mut shutting_down = false;

    loop {
        drain_available_runs(
            &store,
            &object_store,
            &pools,
            &worker_id,
            lease_seconds,
            max_concurrent_runs,
            runner.clone(),
            &mut shutdown,
            &mut shutting_down,
        )
        .await?;

        if !loop_enabled || shutting_down {
            break;
        }

        let sleep = tokio::time::sleep(jittered_poll_duration(&worker_id, poll_seconds));
        tokio::pin!(sleep);
        tokio::select! {
            () = &mut sleep => {}
            () = &mut shutdown => {
                shutting_down = true;
                observability::tracing::info!(
                    "worker shutdown requested; no more runs will be claimed"
                );
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn drain_available_runs<R>(
    store: &PostgresStore,
    object_store: &ConfiguredObjectStore,
    pools: &ExecutionPoolsConfig,
    worker_id: &str,
    lease_seconds: i64,
    max_concurrent_runs: usize,
    runner: R,
    shutdown: &mut (impl std::future::Future<Output = ()> + Unpin),
    shutting_down: &mut bool,
) -> anyhow::Result<()>
where
    R: Runner,
{
    let started = std::time::Instant::now();
    let mut tasks = JoinSet::new();
    let mut accepting = !*shutting_down;

    loop {
        while accepting && tasks.len() < max_concurrent_runs {
            let task_store = store.clone();
            let task_object_store = object_store.clone();
            let task_pools = pools.clone();
            let task_worker_id = worker_id.to_string();
            let task_runner = runner.clone();
            tasks.spawn(async move {
                execute_one_queued_run(
                    &task_store,
                    &task_runner,
                    &task_object_store,
                    &task_pools,
                    &task_worker_id,
                    lease_seconds,
                )
                .await
            });
        }

        if tasks.is_empty() {
            return Ok(());
        }

        tokio::select! {
            biased;
            () = &mut *shutdown, if !*shutting_down => {
                *shutting_down = true;
                accepting = false;
                observability::tracing::info!(
                    in_flight_runs = tasks.len(),
                    "worker shutdown requested; waiting for in-flight runs"
                );
            }
            result = tasks.join_next() => {
                let Some(result) = result else {
                    return Ok(());
                };
                match result {
                    Ok(Ok(crate::WorkerTickOutcome::NoRunAvailable)) => {
                        accepting = false;
                        observability::record_service_tick(
                            "worker",
                            "no_run_available",
                            started.elapsed(),
                        );
                        observability::tracing::info!(
                            outcome = "NoRunAvailable",
                            "worker tick outcome"
                        );
                    }
                    Ok(Ok(outcome)) => {
                        observability::record_service_tick(
                            "worker",
                            "run_processed",
                            started.elapsed(),
                        );
                        observability::tracing::info!(?outcome, "worker tick outcome");
                    }
                    Ok(Err(error)) => return Err(anyhow!(error)),
                    Err(error) => return Err(error).context("worker task join failed"),
                }
            }
        }
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

fn jittered_poll_duration(worker_id: &str, poll_seconds: u64) -> Duration {
    let base = Duration::from_secs(poll_seconds);
    if poll_seconds == 0 {
        return base;
    }
    let now_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut hasher = DefaultHasher::new();
    worker_id.hash(&mut hasher);
    now_nanos.hash(&mut hasher);
    let max_jitter_ms = poll_seconds.saturating_mul(1_000).min(1_000);
    let jitter_ms = hasher.finish() % (max_jitter_ms + 1);
    base + Duration::from_millis(jitter_ms)
}

fn load_execution_pools() -> anyhow::Result<ExecutionPoolsConfig> {
    let yaml = if let Ok(value) = env::var("CAPSULET_EXECUTION_POOLS_YAML") {
        value
    } else if let Ok(path) = env::var("CAPSULET_EXECUTION_POOLS_FILE") {
        fs::read_to_string(&path)
            .with_context(|| format!("read CAPSULET_EXECUTION_POOLS_FILE {path}"))?
    } else {
        DEFAULT_EXECUTION_POOLS_YAML.to_string()
    };

    let mut pools = ExecutionPoolsConfig::from_yaml(&yaml).context("parse execution pools YAML")?;
    for pool in pools.pools.values_mut() {
        if pool.runtime_class_name.is_none() {
            pool.runtime_class_name = env::var("CAPSULET_EXECUTION_RUNTIME_CLASS").ok();
        }
        if pool.service_account_name.is_none() {
            pool.service_account_name = env::var("CAPSULET_EXECUTION_SERVICE_ACCOUNT").ok();
        }
    }
    Ok(pools)
}

fn load_object_store() -> anyhow::Result<ConfiguredObjectStore> {
    match env::var("CAPSULET_OBJECT_STORAGE_MODE")
        .unwrap_or_else(|_| "filesystem".to_string())
        .as_str()
    {
        "s3" => Ok(ConfiguredObjectStore::s3(
            &env::var("CAPSULET_OBJECT_STORAGE_BUCKET")
                .unwrap_or_else(|_| "capsulet-artifacts".to_string()),
            env::var("CAPSULET_OBJECT_STORAGE_ENDPOINT").ok().as_deref(),
            &env::var("CAPSULET_OBJECT_STORAGE_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
            &env::var("CAPSULET_OBJECT_STORAGE_ACCESS_KEY_ID")
                .context("set CAPSULET_OBJECT_STORAGE_ACCESS_KEY_ID for s3 object storage")?,
            &env::var("CAPSULET_OBJECT_STORAGE_SECRET_ACCESS_KEY")
                .context("set CAPSULET_OBJECT_STORAGE_SECRET_ACCESS_KEY for s3 object storage")?,
            env_bool("CAPSULET_OBJECT_STORAGE_PATH_STYLE"),
        )
        .context("configure s3 object storage")?),
        "filesystem" => Ok(ConfiguredObjectStore::filesystem(
            env::var("CAPSULET_OBJECT_STORAGE_PATH")
                .unwrap_or_else(|_| DEFAULT_OBJECT_STORAGE_PATH.to_string()),
        )),
        value => bail!("unsupported CAPSULET_OBJECT_STORAGE_MODE {value}"),
    }
}

fn parse_runner_mode(value: &str) -> anyhow::Result<RunnerMode> {
    match value {
        "kubernetes" | "k8s" => Ok(RunnerMode::Kubernetes),
        "stub" => Ok(RunnerMode::Stub),
        "process" | "local" => Ok(RunnerMode::Process),
        value => {
            bail!("unsupported CAPSULET_RUNNER_MODE {value}; expected stub, process, or kubernetes")
        }
    }
}

fn env_bool(name: &str) -> bool {
    matches!(
        env::var(name).as_deref(),
        Ok("1" | "true" | "TRUE" | "yes" | "YES")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_runner_mode_should_reject_unknown_values() {
        let error = parse_runner_mode("docker").expect_err("unknown runner mode should fail");

        assert_eq!(
            error.to_string(),
            "unsupported CAPSULET_RUNNER_MODE docker; expected stub, process, or kubernetes"
        );
    }
}
