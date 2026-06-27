use std::{env, fmt, sync::Arc};

use crate::{AuthConfig, WebhookSecrets};
use capsulet_postgres::PostgresStore;
/// API admission backpressure settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AdmissionConfig {
    pub max_queued_runs: Option<u64>,
    pub max_queued_runs_per_pool: Option<u64>,
    pub max_queued_workflow_runs: Option<u64>,
}

/// Shared API state.
#[derive(Clone)]
pub struct AppState<S, O> {
    pub(crate) store: S,
    pub(crate) object_store: O,
    pub(crate) execution_pools: Arc<Vec<String>>,
    pub(crate) auth: AuthConfig,
    pub(crate) webhook_secrets: WebhookSecrets,
    pub(crate) admission: AdmissionConfig,
}

impl<S, O> AppState<S, O> {
    /// Creates API state.
    #[must_use]
    pub fn new(
        store: S,
        object_store: O,
        execution_pools: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            store,
            object_store,
            execution_pools: Arc::new(
                execution_pools
                    .into_iter()
                    .map(|pool| pool.trim().to_string())
                    .filter(|pool| !pool.is_empty())
                    .collect(),
            ),
            auth: AuthConfig::disabled(),
            webhook_secrets: WebhookSecrets::default(),
            admission: AdmissionConfig::default(),
        }
    }

    #[must_use]
    pub fn with_webhook_secrets(mut self, webhook_secrets: WebhookSecrets) -> Self {
        self.webhook_secrets = webhook_secrets;
        self
    }

    #[must_use]
    pub const fn with_admission(mut self, admission: AdmissionConfig) -> Self {
        self.admission = admission;
        self
    }

    #[must_use]
    pub fn with_auth(mut self, auth: AuthConfig) -> Self {
        self.auth = auth;
        self
    }

    pub(crate) fn knows_pool(&self, pool: &str) -> bool {
        self.execution_pools.iter().any(|known| known == pool)
    }
}

impl AdmissionConfig {
    /// Loads admission settings from environment variables.
    ///
    /// Zero or missing values disable the corresponding limit.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            max_queued_runs: env_optional_u64("CAPSULET_ADMISSION_MAX_QUEUED_RUNS"),
            max_queued_runs_per_pool: env_optional_u64(
                "CAPSULET_ADMISSION_MAX_QUEUED_RUNS_PER_POOL",
            )
            .or_else(|| env_optional_u64("CAPSULET_ADMISSION_MAX_QUEUED_PER_POOL")),
            max_queued_workflow_runs: env_optional_u64(
                "CAPSULET_ADMISSION_MAX_QUEUED_WORKFLOW_RUNS",
            ),
        }
    }
}

fn env_optional_u64(name: &str) -> Option<u64> {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
}

impl<O> fmt::Debug for AppState<PostgresStore, O>
where
    O: fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AppState")
            .field("store", &self.store)
            .field("object_store", &self.object_store)
            .field("execution_pools", &self.execution_pools)
            .field("auth", &self.auth)
            .field("webhook_secrets", &self.webhook_secrets)
            .finish()
    }
}
