use std::{env, net::SocketAddr, time::Duration};

use anyhow::{Context as _, anyhow, bail};
use axum::{Router, http::StatusCode, response::IntoResponse, routing::get};
use capsulet_core::{ComponentDescriptor, ComponentKind};
use capsulet_observability::{self as observability, tracing::Instrument};
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
pub async fn run() -> anyhow::Result<()> {
    observability::init("capsulet-evaluator")
        .map_err(|error| anyhow!("{error}"))
        .context("initialize evaluator observability")?;
    let descriptor = ComponentDescriptor::new(
        ComponentKind::Evaluator,
        "evaluates durable automation trigger conditions and creates workflow runs",
    );
    observability::tracing::info!(component = "evaluator", banner = %descriptor.banner());
    let database_url = env::var("CAPSULET_DATABASE_URL")
        .or_else(|_| env::var("DATABASE_URL"))
        .context("set CAPSULET_DATABASE_URL or DATABASE_URL")?;
    let store = PostgresStore::connect_with_config(&database_url, PostgresPoolConfig::from_env()?)
        .await
        .context("connect evaluator to Postgres")?;
    store.migrate().await.context("run evaluator migrations")?;
    let owner = env::var("CAPSULET_EVALUATOR_ID")
        .unwrap_or_else(|_| format!("evaluator-{}", std::process::id()));
    let poll_seconds = env::var("CAPSULET_EVALUATOR_POLL_SECONDS")
        .ok()
        .map(|value| {
            value
                .parse()
                .context("parse CAPSULET_EVALUATOR_POLL_SECONDS")
        })
        .transpose()?
        .unwrap_or(DEFAULT_POLL_SECONDS);
    let health_addr: SocketAddr = env::var("CAPSULET_EVALUATOR_HEALTH_ADDR")
        .unwrap_or_else(|_| DEFAULT_HEALTH_ADDR.to_string())
        .parse()
        .context("parse CAPSULET_EVALUATOR_HEALTH_ADDR")?;
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
        Ok(value) => SqlConnections::from_json(&value)
            .await
            .context("parse CAPSULET_SQL_CONNECTIONS")?,
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
        .map(|value| {
            value
                .parse()
                .context("parse CAPSULET_RETENTION_INTERVAL_SECONDS")
        })
        .transpose()?
        .unwrap_or(DEFAULT_RETENTION_INTERVAL_SECONDS);
    tokio::spawn(async move {
        loop {
            let span = observability::tracing::info_span!(
                "evaluator.retention_cleanup",
                retention.days = retention_days,
                audit_retention.days = audit_retention_days,
                outcome = observability::tracing::field::Empty,
                error = observability::tracing::field::Empty,
            );
            async {
                let result = run_retention(
                    &retention_store,
                    &object_store,
                    retention_days,
                    audit_retention_days,
                )
                .await;
                match &result {
                    Ok(()) => {
                        observability::tracing::Span::current().record("outcome", "success");
                    }
                    Err(error) => {
                        observability::tracing::Span::current()
                            .record("outcome", "error")
                            .record("error", observability::tracing::field::display(error));
                        observability::tracing::warn!(%error, "retention cleanup failed");
                    }
                }
            }
            .instrument(span)
            .await;
            tokio::time::sleep(Duration::from_secs(retention_interval.max(60))).await;
        }
    });
    let evaluator = Evaluator::new(store, owner).with_sql_connections(sql_connections);
    loop {
        let started = std::time::Instant::now();
        let span = observability::tracing::info_span!(
            "evaluator.tick",
            outcome = observability::tracing::field::Empty,
            error = observability::tracing::field::Empty,
        );
        let tick = async {
            let result = evaluator.tick().await;
            match &result {
                Ok(true) => {
                    observability::tracing::Span::current().record("outcome", "work");
                }
                Ok(false) => {
                    observability::tracing::Span::current().record("outcome", "idle");
                }
                Err(error) => {
                    observability::tracing::Span::current()
                        .record("outcome", "error")
                        .record("error", observability::tracing::field::display(error));
                }
            }
            result
        }
        .instrument(span)
        .await;
        match tick {
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
) -> anyhow::Result<()> {
    for candidate in store.list_retention_candidates(retention_days, 100).await? {
        for key in &candidate.object_keys {
            object_store
                .delete(key)
                .await
                .with_context(|| format!("delete retained object {key}"))?;
        }
        store
            .complete_retention_cleanup(&candidate.job_run_id)
            .await
            .with_context(|| {
                format!(
                    "mark retention cleanup complete for {}",
                    candidate.job_run_id
                )
            })?;
    }
    store
        .cleanup_old_audit_events(audit_retention_days)
        .await
        .context("cleanup old audit events")?;
    store
        .cleanup_old_trigger_events(retention_days)
        .await
        .context("cleanup old trigger events")?;
    Ok(())
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

fn env_i32(name: &str, default: i32) -> anyhow::Result<i32> {
    parse_positive_i32_setting(name, env::var(name).ok().as_deref(), default)
}

fn parse_positive_i32_setting(
    name: &str,
    value: Option<&str>,
    default: i32,
) -> anyhow::Result<i32> {
    let value = value
        .map(|value| {
            value
                .parse::<i32>()
                .with_context(|| format!("parse {name}"))
        })
        .transpose()?
        .unwrap_or(default);
    if value < 1 {
        bail!("{name} must be at least 1");
    }
    Ok(value)
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
    fn positive_i32_setting_should_error_when_value_is_zero() {
        let error = parse_positive_i32_setting("CAPSULET_RETENTION_DAYS", Some("0"), 30)
            .expect_err("zero should be rejected");

        assert_eq!(
            error.to_string(),
            "CAPSULET_RETENTION_DAYS must be at least 1"
        );
    }
}
