//! What the HTTP API needs from storage, one trait per domain.
//!
//! [`ApiStore`] is all of them together, and anything that implements every one
//! is one: `PostgresStore` in production, and an in-memory fake in the API's
//! tests. A handler asks for `S: ApiStore` and calls what it needs; each domain's
//! methods, and the Postgres implementation of them, live together in its module.

mod agents;
mod automations;
mod iam;
mod ingestion;
mod ir;
mod jobs;
mod memory;
mod memory_graph;
mod packs;
mod reasoning;
mod workflows;

use std::fmt::Display;

use async_trait::async_trait;
use capsulet_postgres::{AdmissionSnapshot, PostgresStore, PostgresStoreError};

pub use self::{
    agents::AgentStore, automations::AutomationStore, iam::IamStore, ingestion::IngestionStore,
    ir::IrStore, jobs::JobStore, memory::MemoryStore, memory_graph::MemoryGraphStore,
    packs::PackStore, reasoning::ReasoningStore, workflows::WorkflowStore,
};

/// What every store has, whatever else it holds: its error, and whether it is up.
#[async_trait]
pub trait StoreBase: Clone + Send + Sync + 'static {
    type Error: Display + Send + Sync + 'static;

    fn is_scope_mismatch(_error: &Self::Error) -> bool {
        false
    }

    fn is_already_exists(_error: &Self::Error) -> bool {
        false
    }

    async fn ping(&self) -> Result<(), Self::Error>;

    async fn prometheus_metrics(&self) -> Result<String, Self::Error> {
        Ok(String::new())
    }

    async fn admission_snapshot(
        &self,
        _execution_pool: &str,
    ) -> Result<AdmissionSnapshot, Self::Error> {
        Ok(AdmissionSnapshot::default())
    }
}

/// Storage operations required by the HTTP API: every domain's.
pub trait ApiStore:
    StoreBase
    + IamStore
    + JobStore
    + WorkflowStore
    + AutomationStore
    + AgentStore
    + MemoryStore
    + MemoryGraphStore
    + IngestionStore
    + IrStore
    + ReasoningStore
    + PackStore
{
}

impl<T> ApiStore for T where
    T: StoreBase
        + IamStore
        + JobStore
        + WorkflowStore
        + AutomationStore
        + AgentStore
        + MemoryStore
        + MemoryGraphStore
        + IngestionStore
        + IrStore
        + ReasoningStore
        + PackStore
{
}

#[async_trait]
impl StoreBase for PostgresStore {
    fn is_scope_mismatch(error: &Self::Error) -> bool {
        matches!(error, PostgresStoreError::ScopeMismatch)
    }

    fn is_already_exists(error: &Self::Error) -> bool {
        matches!(error, PostgresStoreError::AlreadyExists(_))
    }

    type Error = PostgresStoreError;

    async fn ping(&self) -> Result<(), Self::Error> {
        self.ping().await
    }

    async fn prometheus_metrics(&self) -> Result<String, Self::Error> {
        PostgresStore::prometheus_metrics(self).await
    }

    async fn admission_snapshot(
        &self,
        execution_pool: &str,
    ) -> Result<AdmissionSnapshot, Self::Error> {
        PostgresStore::admission_snapshot(self, execution_pool).await
    }
}
