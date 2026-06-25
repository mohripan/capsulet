use std::{env, net::SocketAddr};

use anyhow::{Context as _, anyhow, bail};
use capsulet_core::{ComponentDescriptor, ComponentKind};
use capsulet_observability as observability;
use capsulet_postgres::{PostgresPoolConfig, PostgresStore};
use capsulet_storage::ConfiguredObjectStore;

use jsonwebtoken::jwk::JwkSet;

use crate::{AppState, AuthConfig, WebhookSecrets, router};

const DEFAULT_ADDR: &str = "127.0.0.1:8080";
const DEFAULT_EXECUTION_POOLS: &str = "mini,large";
const DEFAULT_OBJECT_STORAGE_PATH: &str = ".capsulet-objects";

/// Runs the API service from environment configuration.
///
/// # Errors
///
/// Returns an error when required environment variables are missing, database
/// setup fails, object storage cannot be configured, or the HTTP server exits
/// with an error.
pub async fn run() -> anyhow::Result<()> {
    observability::init("capsulet-api")
        .map_err(|error| anyhow!("{error}"))
        .context("initialize API observability")?;
    let descriptor = ComponentDescriptor::new(
        ComponentKind::Api,
        "control plane api for automations, jobs, logs, and artifacts",
    );
    observability::tracing::info!(component = "api", banner = %descriptor.banner());

    let database_url = env::var("CAPSULET_DATABASE_URL")
        .or_else(|_| env::var("DATABASE_URL"))
        .context("set CAPSULET_DATABASE_URL or DATABASE_URL before starting capsulet-api")?;
    let addr = env::var("CAPSULET_API_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
    let addr: SocketAddr = addr.parse().context("parse CAPSULET_API_ADDR")?;
    let execution_pools = env::var("CAPSULET_EXECUTION_POOLS")
        .unwrap_or_else(|_| DEFAULT_EXECUTION_POOLS.to_string())
        .split(',')
        .map(str::to_string)
        .collect::<Vec<_>>();

    let store = PostgresStore::connect_with_config(&database_url, PostgresPoolConfig::from_env()?)
        .await
        .context("connect API to Postgres")?;
    store.migrate().await.context("run API migrations")?;
    if env::var("CAPSULET_SEED_EXAMPLES").is_ok_and(|value| value == "true") {
        store
            .seed_example_job_definitions()
            .await
            .context("seed example job definitions")?;
        observability::tracing::info!("seeded example job definitions");
    }
    if env_bool("CAPSULET_MIGRATE_ONLY") {
        observability::tracing::info!(
            "database migrations complete; exiting because CAPSULET_MIGRATE_ONLY is set"
        );
        return Ok(());
    }

    let object_store = load_object_store()?;
    let auth = load_auth_config().await?;
    let webhook_secrets = env::var("CAPSULET_WEBHOOK_SECRETS")
        .ok()
        .map(|value| WebhookSecrets::from_json(&value))
        .transpose()
        .map_err(|error| anyhow!("{error}"))
        .context("parse CAPSULET_WEBHOOK_SECRETS")?
        .unwrap_or_default();

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind API listener on {addr}"))?;
    observability::tracing::info!(%addr, "capsulet-api listening");

    axum::serve(
        listener,
        router(
            AppState::new(store, object_store, execution_pools)
                .with_auth(auth)
                .with_webhook_secrets(webhook_secrets),
        ),
    )
    .await
    .context("serve API HTTP traffic")?;
    observability::shutdown();

    Ok(())
}

async fn load_auth_config() -> anyhow::Result<AuthConfig> {
    if env_bool("CAPSULET_AUTH_DISABLED") {
        observability::tracing::warn!("API authentication is explicitly disabled");
        return Ok(AuthConfig::disabled());
    }
    let value = env::var("CAPSULET_API_TOKENS")
        .context("set CAPSULET_API_TOKENS or explicitly set CAPSULET_AUTH_DISABLED=true")?;
    let mut config = AuthConfig::from_json(&value)
        .map_err(|error| anyhow!("{error}"))
        .context("parse CAPSULET_API_TOKENS")?;
    if let (Ok(issuer), Ok(audience), Ok(jwks_url)) = (
        env::var("CAPSULET_OIDC_ISSUER"),
        env::var("CAPSULET_OIDC_AUDIENCE"),
        env::var("CAPSULET_OIDC_JWKS_URL"),
    ) {
        let jwks = load_jwks_with_retry(&jwks_url)
            .await
            .with_context(|| format!("load OIDC JWKS from {jwks_url}"))?;
        config = config.with_oidc(issuer, audience, &jwks);
        observability::tracing::info!(%jwks_url, "loaded OIDC authentication metadata");
    }
    Ok(config)
}

async fn load_jwks_with_retry(jwks_url: &str) -> anyhow::Result<JwkSet> {
    let mut last_error = None;
    for _ in 0..30 {
        match reqwest::get(jwks_url).await {
            Ok(response) if response.status().is_success() => {
                return response
                    .json::<JwkSet>()
                    .await
                    .context("decode JWKS response");
            }
            Ok(response) => {
                last_error = Some(format!("JWKS request returned {}", response.status()));
            }
            Err(error) => {
                last_error = Some(error.to_string());
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    bail!(
        "{}",
        last_error.unwrap_or_else(|| "JWKS metadata was unavailable".to_string())
    )
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

fn env_bool(name: &str) -> bool {
    matches!(
        env::var(name).as_deref(),
        Ok("1" | "true" | "TRUE" | "yes" | "YES")
    )
}
