use std::{env, net::SocketAddr};

use capsulet_api::{AppState, router};
use capsulet_core::{ComponentDescriptor, ComponentKind};
use capsulet_postgres::PostgresStore;

const DEFAULT_ADDR: &str = "127.0.0.1:8080";
const DEFAULT_EXECUTION_POOLS: &str = "mini,large";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let descriptor = ComponentDescriptor::new(
        ComponentKind::Api,
        "control plane api for automations, jobs, logs, and artifacts",
    );
    println!("{}", descriptor.banner());

    let database_url = env::var("CAPSULET_DATABASE_URL")
        .or_else(|_| env::var("DATABASE_URL"))
        .map_err(|_| "set CAPSULET_DATABASE_URL or DATABASE_URL before starting capsulet-api")?;
    let addr = env::var("CAPSULET_API_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
    let addr: SocketAddr = addr.parse()?;
    let execution_pools = env::var("CAPSULET_EXECUTION_POOLS")
        .unwrap_or_else(|_| DEFAULT_EXECUTION_POOLS.to_string())
        .split(',')
        .map(str::to_string)
        .collect::<Vec<_>>();

    let store = PostgresStore::connect(&database_url).await?;
    store.migrate().await?;
    if env::var("CAPSULET_SEED_EXAMPLES").is_ok_and(|value| value == "true") {
        store.seed_hello_python_job_definition().await?;
        println!("seeded example job definition: job_hello_python");
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("capsulet-api listening on http://{addr}");

    axum::serve(listener, router(AppState::new(store, execution_pools))).await?;

    Ok(())
}
