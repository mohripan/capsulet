use std::{env, net::SocketAddr, time::Duration};

use axum::{Router, http::StatusCode, routing::get};
use capsulet_core::{ComponentDescriptor, ComponentKind};
use capsulet_postgres::PostgresStore;

use crate::{Evaluator, SqlConnections};

const DEFAULT_POLL_SECONDS: u64 = 2;
const DEFAULT_HEALTH_ADDR: &str = "0.0.0.0:8083";

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let descriptor = ComponentDescriptor::new(
        ComponentKind::Evaluator,
        "evaluates durable automation trigger conditions and creates workflow runs",
    );
    println!("{}", descriptor.banner());
    let database_url = env::var("CAPSULET_DATABASE_URL")
        .or_else(|_| env::var("DATABASE_URL"))
        .map_err(|_| "set CAPSULET_DATABASE_URL or DATABASE_URL")?;
    let store = PostgresStore::connect(&database_url).await?;
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
            );
        match tokio::net::TcpListener::bind(health_addr).await {
            Ok(listener) => {
                if let Err(error) = axum::serve(listener, app).await {
                    eprintln!("evaluator health server stopped: {error}");
                }
            }
            Err(error) => eprintln!("evaluator health listener failed: {error}"),
        }
    });
    let sql_connections = match env::var("CAPSULET_SQL_CONNECTIONS") {
        Ok(value) => SqlConnections::from_json(&value).await?,
        Err(_) => SqlConnections::default(),
    };
    let evaluator = Evaluator::new(store, owner).with_sql_connections(sql_connections);
    loop {
        match evaluator.tick().await {
            Ok(true) => continue,
            Ok(false) => tokio::time::sleep(Duration::from_secs(poll_seconds)).await,
            Err(error) => {
                eprintln!("trigger evaluation failed: {error}");
                tokio::time::sleep(Duration::from_secs(poll_seconds)).await;
            }
        }
    }
}
