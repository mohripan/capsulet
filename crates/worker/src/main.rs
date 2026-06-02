use std::env;

use capsulet_core::{ComponentDescriptor, ComponentKind};
use capsulet_postgres::PostgresStore;
use capsulet_runner::StubRunner;
use capsulet_worker::execute_one_queued_run;

const DEFAULT_WORKER_ID: &str = "worker-local";
const DEFAULT_LEASE_SECONDS: i64 = 60;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    let runner = match env::var("CAPSULET_STUB_RUNNER_RESULT").as_deref() {
        Ok("failed" | "failure") => StubRunner::failure(),
        _ => StubRunner::success(),
    };

    let store = PostgresStore::connect(&database_url).await?;
    store.migrate().await?;

    let outcome = execute_one_queued_run(&store, &runner, &worker_id, lease_seconds).await?;
    println!("worker tick outcome: {outcome:?}");

    Ok(())
}
