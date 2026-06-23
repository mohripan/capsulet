use std::{env, net::SocketAddr, time::Duration};

use axum::{Router, http::StatusCode, response::IntoResponse, routing::get};
use capsulet_core::{ComponentDescriptor, ComponentKind};
use capsulet_observability as observability;
use capsulet_postgres::{PostgresPoolConfig, PostgresStore};
use capsulet_storage::{ConfiguredObjectStore, ObjectStore};

use crate::{Evaluator, SqlConnections};

const DEFAULT_POLL_SECONDS: u64 = 2;
const DEFAULT_HEALTH_ADDR: &str = "0.0.0.0:8083";
const DEFAULT_OBJECT_STORAGE_PATH: &str = "./data/objects";
const DEFAULT_RETENTION_DAYS: i32 = 30;
const DEFAULT_AUDIT_RETENTION_DAYS: i32 = 365;
const DEFAULT_RETENTION_INTERVAL_SECONDS: u64 = 3600;

#[allow(clippy::too_many_lines)]
/// Runs trigger evaluation, health/metrics, and retention until shutdown.
///
/// # Errors
///
/// Returns an error when configuration is invalid or a dependency cannot be initialized.
pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    observability::init("capsulet-evaluator")?;
    let descriptor = ComponentDescriptor::new(
        ComponentKind::Evaluator,
        "evaluates durable automation trigger conditions and creates workflow runs",
    );
    observability::tracing::info!(component = "evaluator", banner = %descriptor.banner());
    let database_url = env::var("CAPSULET_DATABASE_URL")
        .or_else(|_| env::var("DATABASE_URL"))
        .map_err(|_| "set CAPSULET_DATABASE_URL or DATABASE_URL")?;
    let store =
        PostgresStore::connect_with_config(&database_url, PostgresPoolConfig::from_env()?).await?;
    store.migrate().await?;
    let owner = env::var("CAPSULET_EVALUATOR_ID")
        .unwrap_or_else(|_| format!("evaluator-{}", std::process::id()));
    let poll_seconds = env::var("CAPSULET_EVALUATOR_POLL_SECONDS")
        .ok()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(DEFAULT_POLL_SECONDS);
    let health_addr: SocketAddr = env::var("CAPSULET_EVALUATOR_HEALTH_ADDR")
        .unwrap_or_else(|_| DEFAULT_HEALTH_ADDR.to_string())
        .parse()?;
    let health_store = store.clone();
    let metrics_store = store.clone();
    tokio::spawn(async move {
        let app = Router::new()
            .route("/livez", get(|| async { StatusCode::OK }))
            .route(
                "/readyz",
                get(move || {
                    let store = health_store.clone();
                    async move {
                        if store.ping().await.is_ok() {
                            StatusCode::OK
                        } else {
                            StatusCode::SERVICE_UNAVAILABLE
                        }
                    }
                }),
            )
            .route(
                "/metrics",
                get(move || {
                    let store = metrics_store.clone();
                    async move {
                        match store.prometheus_metrics().await {
                            Ok(db_body) => {
                                let mut body = observability::render_metrics();
                                if !body.is_empty() && !body.ends_with('\n') {
                                    body.push('\n');
                                }
                                body.push_str(&db_body);
                                ([("content-type", "text/plain; version=0.0.4")], body)
                                    .into_response()
                            }
                            Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
                        }
                    }
                }),
            );
        match tokio::net::TcpListener::bind(health_addr).await {
            Ok(listener) => {
                if let Err(error) = axum::serve(listener, app).await {
                    observability::tracing::warn!(%error, "evaluator health server stopped");
                }
            }
            Err(error) => observability::tracing::warn!(%error, "evaluator health listener failed"),
        }
    });
    let sql_connections = match env::var("CAPSULET_SQL_CONNECTIONS") {
        Ok(value) => SqlConnections::from_json(&value).await?,
        Err(_) => SqlConnections::default(),
    };
    let retention_store = store.clone();
    let object_store = load_object_store()?;
    let retention_days = env_i32("CAPSULET_RETENTION_DAYS", DEFAULT_RETENTION_DAYS)?;
    let audit_retention_days = env_i32(
        "CAPSULET_AUDIT_RETENTION_DAYS",
        DEFAULT_AUDIT_RETENTION_DAYS,
    )?;
    let retention_interval = env::var("CAPSULET_RETENTION_INTERVAL_SECONDS")
        .ok()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(DEFAULT_RETENTION_INTERVAL_SECONDS);
    tokio::spawn(async move {
        loop {
            if let Err(error) = run_retention(
                &retention_store,
                &object_store,
                retention_days,
                audit_retention_days,
            )
            .await
            {
                observability::tracing::warn!(%error, "retention cleanup failed");
            }
            tokio::time::sleep(Duration::from_secs(retention_interval.max(60))).await;
        }
    });
    let evaluator = Evaluator::new(store, owner).with_sql_connections(sql_connections);
    loop {
        let started = std::time::Instant::now();
        match evaluator.tick().await {
            Ok(true) => {
                observability::record_service_tick("evaluator", "work", started.elapsed());
            }
            Ok(false) => {
                observability::record_service_tick("evaluator", "idle", started.elapsed());
                tokio::time::sleep(Duration::from_secs(poll_seconds)).await;
            }
            Err(error) => {
                observability::record_service_tick("evaluator", "error", started.elapsed());
                observability::tracing::warn!(%error, "trigger evaluation failed");
                tokio::time::sleep(Duration::from_secs(poll_seconds)).await;
            }
        }
    }
}

async fn run_retention(
    store: &PostgresStore,
    object_store: &ConfiguredObjectStore,
    retention_days: i32,
    audit_retention_days: i32,
) -> Result<(), Box<dyn std::error::Error>> {
    for candidate in store.list_retention_candidates(retention_days, 100).await? {
        for key in &candidate.object_keys {
            object_store.delete(key).await?;
        }
        store
            .complete_retention_cleanup(&candidate.job_run_id)
            .await?;
    }
    store.cleanup_old_audit_events(audit_retention_days).await?;
    store.cleanup_old_trigger_events(retention_days).await?;
    Ok(())
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

fn env_i32(name: &str, default: i32) -> Result<i32, Box<dyn std::error::Error>> {
    let value = env::var(name)
        .ok()
        .map(|value| value.parse::<i32>())
        .transpose()?
        .unwrap_or(default);
    if value < 1 {
        return Err(format!("{name} must be at least 1").into());
    }
    Ok(value)
}

fn env_bool(name: &str) -> bool {
    matches!(
        env::var(name).as_deref(),
        Ok("1" | "true" | "TRUE" | "yes" | "YES")
    )
}
