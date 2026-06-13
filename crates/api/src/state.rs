use std::{fmt, sync::Arc};

use capsulet_postgres::PostgresStore;
/// Shared API state.
#[derive(Clone)]
pub struct AppState<S, O> {
    pub(crate) store: S,
    pub(crate) object_store: O,
    pub(crate) execution_pools: Arc<Vec<String>>,
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
        }
    }

    pub(crate) fn knows_pool(&self, pool: &str) -> bool {
        self.execution_pools.iter().any(|known| known == pool)
    }
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
            .finish()
    }
}
