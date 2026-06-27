//! Scheduler service runtime.

use std::{env, time::Duration};

use anyhow::{Context as _, anyhow};
use axum::{Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};
use capsulet_core::{ComponentDescriptor, ComponentKind};
use capsulet_observability::{self as observability, tracing::Instrument};
use capsulet_postgres::{PostgresPoolConfig, PostgresStore};

const DEFAULT_POLL_SECONDS: u64 = 5;
const DEFAULT_HEALTH_ADDR: &str = "0.0.0.0:8082";

/// Runs the scheduler service from environment configuration.
///
/// # Errors
///
/// Returns an error when required environment variables are missing, database
/// setup fails, or a scheduler tick cannot be persisted.
pub async fn run() -> anyhow::Result<()> {
    observability::init("capsulet-scheduler")
        .map_err(|error| anyhow!("{error}"))
        .context("initialize scheduler observability")?;
    let descriptor = ComponentDescriptor::new(
        ComponentKind::Scheduler,
        "creates scheduled automation workflow runs and advances workflow steps",
    );
    observability::tracing::info!(component = "scheduler", banner = %descriptor.banner());

    let database_url = env::var("CAPSULET_DATABASE_URL")
        .or_else(|_| env::var("DATABASE_URL"))
        .context("set CAPSULET_DATABASE_URL or DATABASE_URL before starting capsulet-scheduler")?;
    let poll_seconds = env::var("CAPSULET_SCHEDULER_POLL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_POLL_SECONDS);
    let loop_enabled = env_bool("CAPSULET_SCHEDULER_LOOP");

    let store = PostgresStore::connect_with_config_and_retry(
        &database_url,
        PostgresPoolConfig::from_env()?,
    )
    .await
    .context("connect scheduler to Postgres")?;
    store.migrate().await.context("run scheduler migrations")?;
    start_health_server(
        store.clone(),
        &env::var("CAPSULET_SCHEDULER_HEALTH_ADDR")
            .unwrap_or_else(|_| DEFAULT_HEALTH_ADDR.to_string()),
    )
    .await?;

    loop {
        let started = std::time::Instant::now();
        let span = observability::tracing::info_span!(
            "scheduler.tick",
            triggered = observability::tracing::field::Empty,
            advanced = observability::tracing::field::Empty,
            outcome = observability::tracing::field::Empty,
            error = observability::tracing::field::Empty,
        );
        let (triggered, advanced) = async {
            let result = async {
                let triggered = store
                    .trigger_due_interval_automations()
                    .await
                    .context("trigger due interval automations")?;
                let advanced = store
                    .advance_workflow_runs()
                    .await
                    .context("advance workflow runs")?;
                Ok::<_, anyhow::Error>((triggered, advanced))
            }
            .await;
            match &result {
                Ok((triggered, advanced)) => {
                    observability::tracing::Span::current()
                        .record("triggered", *triggered)
                        .record("advanced", *advanced)
                        .record("outcome", "success");
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
        .await?;
        observability::record_service_tick("scheduler", "success", started.elapsed());
        observability::tracing::info!(triggered, advanced, "scheduler tick");

        if !loop_enabled {
            break;
        }
        tokio::time::sleep(Duration::from_secs(poll_seconds)).await;
    }

    Ok(())
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
        .with_context(|| format!("bind scheduler health listener on {address}"))?;
    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            observability::tracing::warn!(%error, "scheduler health server stopped");
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

fn env_bool(name: &str) -> bool {
    matches!(
        env::var(name).as_deref(),
        Ok("1" | "true" | "TRUE" | "yes" | "YES")
    )
}
