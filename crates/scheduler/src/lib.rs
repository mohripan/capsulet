use std::{env, time::Duration};

use capsulet_core::{ComponentDescriptor, ComponentKind};
use capsulet_postgres::PostgresStore;

const DEFAULT_POLL_SECONDS: u64 = 5;

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

fn env_bool(name: &str) -> bool {
    matches!(
        env::var(name).as_deref(),
        Ok("1" | "true" | "TRUE" | "yes" | "YES")
    )
}
