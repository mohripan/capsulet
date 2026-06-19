use std::{env, fs, time::Duration};

use axum::{Router, extract::State, http::StatusCode, routing::get};
use capsulet_core::{ComponentDescriptor, ComponentKind};
use capsulet_postgres::PostgresStore;
use capsulet_runner::{ExecutionPoolsConfig, KubernetesRunner, ProcessRunner, StubRunner};
use capsulet_storage::ConfiguredObjectStore;

use crate::execute_one_queued_run;

const DEFAULT_WORKER_ID: &str = "worker-local";
const DEFAULT_LEASE_SECONDS: i64 = 60;
const DEFAULT_POLL_SECONDS: u64 = 5;
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

/// Runs the worker service from environment configuration.
///
/// # Errors
///
/// Returns an error when required environment variables are missing, database
/// setup fails, object storage or execution pools cannot be configured, or a
/// worker tick fails.
pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let descriptor = ComponentDescriptor::new(
        ComponentKind::Worker,
        "leases queued job runs and coordinates execution",
    );
    println!("{}", descriptor.banner());

    let database_url = env::var("CAPSULET_DATABASE_URL")
        .or_else(|_| env::var("DATABASE_URL"))
        .map_err(|_| "set CAPSULET_DATABASE_URL or DATABASE_URL before starting capsulet-worker")?;
    let worker_id =
        env::var("CAPSULET_WORKER_ID").unwrap_or_else(|_| DEFAULT_WORKER_ID.to_string());
    let lease_seconds = env::var("CAPSULET_WORKER_LEASE_SECONDS")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(DEFAULT_LEASE_SECONDS);
    let pools = load_execution_pools()?;

    let store = PostgresStore::connect(&database_url).await?;
    store.migrate().await?;
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

    loop {
        let outcome = run_once(
            &store,
            &object_store,
            &pools,
            &worker_id,
            lease_seconds,
            runner_mode.as_str(),
        )
        .await?;
        println!("worker tick outcome: {outcome:?}");

        if !loop_enabled {
            break;
        }

        tokio::time::sleep(Duration::from_secs(poll_seconds)).await;
    }

    Ok(())
}

async fn start_health_server(
    store: PostgresStore,
    address: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = Router::new()
        .route("/livez", get(|| async { StatusCode::OK }))
        .route("/healthz", get(ready))
        .route("/readyz", get(ready))
        .with_state(store);
    let listener = tokio::net::TcpListener::bind(address).await?;
    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            eprintln!("worker health server stopped: {error}");
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

async fn run_once(
    store: &PostgresStore,
    object_store: &ConfiguredObjectStore,
    pools: &ExecutionPoolsConfig,
    worker_id: &str,
    lease_seconds: i64,
    runner_mode: &str,
) -> Result<crate::WorkerTickOutcome, Box<dyn std::error::Error>> {
    let outcome = match runner_mode {
        "kubernetes" | "k8s" => {
            let namespace = env::var("CAPSULET_EXECUTION_NAMESPACE")
                .unwrap_or_else(|_| DEFAULT_EXECUTION_NAMESPACE.to_string());
            let log_limit_bytes = env::var("CAPSULET_LOG_LIMIT_BYTES")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(DEFAULT_LOG_LIMIT_BYTES);
            let runner = KubernetesRunner::from_default_config(namespace, log_limit_bytes).await?;
            execute_one_queued_run(
                store,
                &runner,
                object_store,
                pools,
                worker_id,
                lease_seconds,
            )
            .await?
        }
        "stub" => {
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
            execute_one_queued_run(
                store,
                &runner,
                object_store,
                pools,
                worker_id,
                lease_seconds,
            )
            .await?
        }
        "process" | "local" => {
            execute_one_queued_run(
                store,
                &ProcessRunner,
                object_store,
                pools,
                worker_id,
                lease_seconds,
            )
            .await?
        }
        value => {
            return Err(format!(
                "unsupported CAPSULET_RUNNER_MODE {value}; expected stub, process, or kubernetes"
            )
            .into());
        }
    };
    Ok(outcome)
}

fn load_execution_pools() -> Result<ExecutionPoolsConfig, Box<dyn std::error::Error>> {
    let yaml = if let Ok(value) = env::var("CAPSULET_EXECUTION_POOLS_YAML") {
        value
    } else if let Ok(path) = env::var("CAPSULET_EXECUTION_POOLS_FILE") {
        fs::read_to_string(path)?
    } else {
        DEFAULT_EXECUTION_POOLS_YAML.to_string()
    };

    Ok(ExecutionPoolsConfig::from_yaml(&yaml)?)
}

fn load_object_store() -> Result<ConfiguredObjectStore, Box<dyn std::error::Error>> {
    match env::var("CAPSULET_OBJECT_STORAGE_MODE")
        .unwrap_or_else(|_| "filesystem".to_string())
        .as_str()
    {
        "s3" => Ok(ConfiguredObjectStore::s3(
            &env::var("CAPSULET_OBJECT_STORAGE_BUCKET")
                .unwrap_or_else(|_| "capsulet-artifacts".to_string()),
            env::var("CAPSULET_OBJECT_STORAGE_ENDPOINT").ok().as_deref(),
            &env::var("CAPSULET_OBJECT_STORAGE_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
            &env::var("CAPSULET_OBJECT_STORAGE_ACCESS_KEY_ID")?,
            &env::var("CAPSULET_OBJECT_STORAGE_SECRET_ACCESS_KEY")?,
            env_bool("CAPSULET_OBJECT_STORAGE_PATH_STYLE"),
        )?),
        "filesystem" => Ok(ConfiguredObjectStore::filesystem(
            env::var("CAPSULET_OBJECT_STORAGE_PATH")
                .unwrap_or_else(|_| DEFAULT_OBJECT_STORAGE_PATH.to_string()),
        )),
        value => Err(format!("unsupported CAPSULET_OBJECT_STORAGE_MODE {value}").into()),
    }
}

fn env_bool(name: &str) -> bool {
    matches!(
        env::var(name).as_deref(),
        Ok("1" | "true" | "TRUE" | "yes" | "YES")
    )
}
