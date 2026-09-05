//! The worker as a long-running service.
//!
//! Configuration comes from the environment, the loop is the plain one — lease
//! something, advance it, go back for more — and there is no state kept between
//! passes. That last part is the whole design: killing this process loses
//! nothing, because it was never holding anything.

use std::env;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, anyhow};
use async_trait::async_trait;
use capsulet_core::{ComponentDescriptor, ComponentKind};
use capsulet_ir::loop_region::FailureKind;
use capsulet_observability as observability;
use capsulet_postgres::{PostgresPoolConfig, PostgresStore};

use crate::clock::SystemClock;
use crate::execute::{EffectOutcome, EffectRequest, Executor, NodeOutcome, NodeRequest};
use crate::runtime::{GraphWorker, Progress, WorkerConfig};

const DEFAULT_LEASE_SECONDS: i64 = 60;
const DEFAULT_IDLE_POLL_SECONDS: u64 = 2;

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

    let config = config_from_env();
    let idle = config.idle_poll;
    let worker = GraphWorker::new(
        store,
        Arc::new(UnconfiguredExecutor),
        Arc::new(SystemClock),
        config,
    );

    loop {
        match worker.advance_one().await {
            Ok(Progress::NothingToDo) => tokio::time::sleep(idle).await,
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

/// The executor this binary ships with, which performs nothing.
///
/// Node providers and effect transports arrive with the verifier protocol in
/// M4. Until they do, a deployed worker refuses the work rather than pretending
/// to have done it: a node that reports success it did not achieve is worse
/// than a node that stops.
struct UnconfiguredExecutor;

#[async_trait]
impl Executor for UnconfiguredExecutor {
    async fn run_node(&self, request: NodeRequest<'_>) -> NodeOutcome {
        NodeOutcome::Failed {
            failure: FailureKind::VerifierUnavailable,
            detail: format!(
                "no provider is configured for `{}`; node execution arrives with M4",
                request.node.id
            ),
        }
    }

    async fn perform_effect(&self, request: EffectRequest<'_>) -> EffectOutcome {
        EffectOutcome::Failed {
            failure: FailureKind::VerifierUnavailable,
            detail: format!(
                "no transport is configured for `{}`; effect execution arrives with M4",
                request.effect.id
            ),
        }
    }
}
