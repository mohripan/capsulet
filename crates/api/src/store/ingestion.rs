//! Ingestion connectors, their runs and what each run produced.

use async_trait::async_trait;
use capsulet_core::{
    IngestionConnector, IngestionConnectorId, IngestionRun, IngestionRunId,
    IngestionRunOutputRecord,
};
use capsulet_postgres::PostgresStore;

use super::StoreBase;

/// Ingestion connectors, their runs and what each run produced.
#[async_trait]
pub trait IngestionStore: StoreBase {
    async fn upsert_ingestion_connector(
        &self,
        connector: &IngestionConnector,
    ) -> Result<(), Self::Error>;

    async fn list_ingestion_connectors(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<IngestionConnector>, Self::Error>;

    async fn find_ingestion_connector(
        &self,
        id: &IngestionConnectorId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<IngestionConnector>, Self::Error>;

    async fn upsert_ingestion_run(&self, run: &IngestionRun) -> Result<(), Self::Error>;

    async fn list_ingestion_runs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<IngestionRun>, Self::Error>;

    async fn find_ingestion_run(
        &self,
        id: &IngestionRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<IngestionRun>, Self::Error>;

    async fn upsert_ingestion_run_output(
        &self,
        output: &IngestionRunOutputRecord,
    ) -> Result<(), Self::Error>;

    /// The outputs of a run the project owns; none for another project's run.
    async fn list_ingestion_run_outputs(
        &self,
        run_id: &IngestionRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<IngestionRunOutputRecord>, Self::Error>;
}

#[async_trait]
impl IngestionStore for PostgresStore {
    async fn upsert_ingestion_connector(
        &self,
        connector: &IngestionConnector,
    ) -> Result<(), Self::Error> {
        self.upsert_ingestion_connector(connector).await
    }

    async fn list_ingestion_connectors(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<IngestionConnector>, Self::Error> {
        self.list_ingestion_connectors(tenant_id, project_id, limit)
            .await
    }

    async fn find_ingestion_connector(
        &self,
        id: &IngestionConnectorId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<IngestionConnector>, Self::Error> {
        self.find_ingestion_connector(id, tenant_id, project_id)
            .await
    }

    async fn upsert_ingestion_run(&self, run: &IngestionRun) -> Result<(), Self::Error> {
        self.upsert_ingestion_run(run).await
    }

    async fn list_ingestion_runs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<IngestionRun>, Self::Error> {
        self.list_ingestion_runs(tenant_id, project_id, limit).await
    }

    async fn find_ingestion_run(
        &self,
        id: &IngestionRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<IngestionRun>, Self::Error> {
        self.find_ingestion_run(id, tenant_id, project_id).await
    }

    async fn upsert_ingestion_run_output(
        &self,
        output: &IngestionRunOutputRecord,
    ) -> Result<(), Self::Error> {
        self.upsert_ingestion_run_output(output).await
    }

    async fn list_ingestion_run_outputs(
        &self,
        run_id: &IngestionRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<IngestionRunOutputRecord>, Self::Error> {
        self.list_ingestion_run_outputs(run_id, tenant_id, project_id)
            .await
    }
}
