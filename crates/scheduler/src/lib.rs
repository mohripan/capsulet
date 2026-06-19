use std::{env, time::Duration};

use axum::{Router, extract::State, http::StatusCode, routing::get};
use capsulet_core::{ComponentDescriptor, ComponentKind};
use capsulet_postgres::PostgresStore;

const DEFAULT_POLL_SECONDS: u64 = 5;
const DEFAULT_HEALTH_ADDR: &str = "0.0.0.0:8082";

/// Runs the scheduler service from environment configuration.
///
/// # Errors
///
/// Returns an error when required environment variables are missing, database
/// setup fails, or a scheduler tick cannot be persisted.
pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let descriptor = ComponentDescriptor::new(
        ComponentKind::Scheduler,
        "creates scheduled automation workflow runs and advances workflow steps",
    );
    println!("{}", descriptor.banner());

    let database_url = env::var("CAPSULET_DATABASE_URL")
        .or_else(|_| env::var("DATABASE_URL"))
        .map_err(
            |_| "set CAPSULET_DATABASE_URL or DATABASE_URL before starting capsulet-scheduler",
        )?;
    let poll_seconds = env::var("CAPSULET_SCHEDULER_POLL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_POLL_SECONDS);
    let loop_enabled = env_bool("CAPSULET_SCHEDULER_LOOP");

    let store = PostgresStore::connect(&database_url).await?;
    store.migrate().await?;
    start_health_server(
        store.clone(),
        &env::var("CAPSULET_SCHEDULER_HEALTH_ADDR")
            .unwrap_or_else(|_| DEFAULT_HEALTH_ADDR.to_string()),
    )
    .await?;

    loop {
        let triggered = store.trigger_due_interval_automations().await?;
        let advanced = store.advance_workflow_runs().await?;
        println!("scheduler tick: triggered={triggered} advanced={advanced}");

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
            eprintln!("scheduler health server stopped: {error}");
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

fn env_bool(name: &str) -> bool {
    matches!(
        env::var(name).as_deref(),
        Ok("1" | "true" | "TRUE" | "yes" | "YES")
    )
}
