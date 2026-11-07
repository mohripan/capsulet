//! The worker as a long-running service.
//!
//! Configuration comes from the environment, the loop is the plain one — lease
//! something, advance it, go back for more — and there is no state kept between
//! passes. That last part is the whole design: killing this process loses
//! nothing, because it was never holding anything.

use std::env;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, anyhow};
use axum::{Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};
use capsulet_core::{ComponentDescriptor, ComponentKind};
use capsulet_observability as observability;
use capsulet_postgres::{PostgresPoolConfig, PostgresStore};

use crate::agent::AgentExecutor;
use crate::clock::SystemClock;
use crate::model::{ModelClient, OllamaClient};
use crate::runtime::{GraphWorker, Progress, WorkerConfig};

const DEFAULT_LEASE_SECONDS: i64 = 60;
const DEFAULT_IDLE_POLL_SECONDS: u64 = 2;
const DEFAULT_HEALTH_ADDR: &str = "0.0.0.0:8084";

/// Runs the graph worker from environment configuration.
///
/// # Errors
///
/// Returns an error when required environment variables are missing, database
/// setup fails, or a pass over a run cannot be persisted.
pub async fn run() -> anyhow::Result<()> {
    observability::init("capsulet-graph-worker")
        .map_err(|error| anyhow!("{error}"))
        .context("initialize graph worker observability")?;
    let descriptor = ComponentDescriptor::new(
        ComponentKind::Worker,
        "advances verified-computation IR runs durably",
    );
    observability::tracing::info!(component = "graph-worker", banner = %descriptor.banner());

    let database_url = env::var("CAPSULET_DATABASE_URL")
        .or_else(|_| env::var("DATABASE_URL"))
        .context("set CAPSULET_DATABASE_URL or DATABASE_URL before starting the graph worker")?;

    let store = PostgresStore::connect_with_config_and_retry(
        &database_url,
        PostgresPoolConfig::from_env()?,
    )
    .await
    .context("connect graph worker to Postgres")?;
    store
        .migrate()
        .await
        .context("run graph worker migrations")?;

    start_health_server(
        store.clone(),
        &env::var("CAPSULET_GRAPH_WORKER_HEALTH_ADDR")
            .unwrap_or_else(|_| DEFAULT_HEALTH_ADDR.to_string()),
    )
    .await?;

    let config = config_from_env();
    let idle = config.idle_poll;
    let model = model_from_env();
    if model.is_none() {
        observability::tracing::warn!(
            component = "graph-worker",
            "CAPSULET_OLLAMA_URL is not set; model nodes will fail and say so"
        );
    }
    let worker = GraphWorker::new(
        store.clone(),
        Arc::new(AgentExecutor::new(store, model)),
        Arc::new(SystemClock),
        config,
    );

    loop {
        let started = Instant::now();
        let result = worker.advance_one().await;
        observability::record_service_tick(
            "graph-worker",
            match &result {
                Ok(Progress::NothingToDo) => "idle",
                Ok(_) => "advanced",
                Err(_) => "failed",
            },
            started.elapsed(),
        );
        match result {
            // Nothing to do, or a run still going with nothing it could do this pass.
            // Going straight back would turn the second into a busy loop.
            Ok(Progress::NothingToDo | Progress::StillRunning) => {
                tokio::time::sleep(idle).await;
            }
            Ok(progress) => {
                observability::tracing::info!(component = "graph-worker", ?progress);
            }
            Err(error) => {
                // A pass that failed is not a reason to abandon the queue; the
                // run keeps its log and somebody will lease it again.
                observability::tracing::error!(component = "graph-worker", %error);
                tokio::time::sleep(idle).await;
            }
        }
    }
}

fn config_from_env() -> WorkerConfig {
    WorkerConfig {
        worker_id: env::var("CAPSULET_GRAPH_WORKER_ID")
            .unwrap_or_else(|_| "graph-worker".to_string()),
        tenant: env::var("CAPSULET_GRAPH_WORKER_TENANT").ok(),
        lease_seconds: env::var("CAPSULET_GRAPH_WORKER_LEASE_SECONDS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_LEASE_SECONDS),
        idle_poll: Duration::from_secs(
            env::var("CAPSULET_GRAPH_WORKER_POLL_SECONDS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(DEFAULT_IDLE_POLL_SECONDS),
        ),
        ..WorkerConfig::default()
    }
}

/// The model client, when a deployment has configured one.
fn model_from_env() -> Option<Arc<dyn ModelClient>> {
    env::var("CAPSULET_OLLAMA_URL")
        .ok()
        .filter(|url| !url.trim().is_empty())
        .map(|url| Arc::new(OllamaClient::new(url)) as Arc<dyn ModelClient>)
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
        .with_context(|| format!("bind graph worker health listener on {address}"))?;
    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            observability::tracing::warn!(%error, "graph worker health server stopped");
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

async fn metrics() -> axum::response::Response {
    (
        [("content-type", "text/plain; version=0.0.4")],
        observability::render_metrics(),
    )
        .into_response()
}
