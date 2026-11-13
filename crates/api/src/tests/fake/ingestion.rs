//! Ingestion connectors and runs.

use super::super::*;

#[async_trait::async_trait]
impl IngestionStore for FakeStore {
    async fn upsert_ingestion_connector(
        &self,
        connector: &IngestionConnector,
    ) -> Result<(), Self::Error> {
        let mut connectors = self
            .ingestion_connectors
            .lock()
            .map_err(|error| error.to_string())?;
        connectors.retain(|existing| existing.id() != connector.id());
        connectors.push(connector.clone());
        Ok(())
    }

    async fn list_ingestion_connectors(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<IngestionConnector>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .ingestion_connectors
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|connector| {
                connector.scope().tenant_id() == tenant_id
                    && connector.scope().project_id() == project_id
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_ingestion_connector(
        &self,
        id: &IngestionConnectorId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<IngestionConnector>, Self::Error> {
        Ok(self
            .ingestion_connectors
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|connector| {
                connector.id() == id
                    && connector.scope().tenant_id() == tenant_id
                    && connector.scope().project_id() == project_id
            })
            .cloned())
    }

    async fn upsert_ingestion_run(&self, run: &IngestionRun) -> Result<(), Self::Error> {
        let mut runs = self
            .ingestion_runs
            .lock()
            .map_err(|error| error.to_string())?;
        runs.retain(|existing| existing.id() != run.id());
        runs.push(run.clone());
        Ok(())
    }

    async fn list_ingestion_runs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<IngestionRun>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .ingestion_runs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|run| {
                run.scope().tenant_id() == tenant_id && run.scope().project_id() == project_id
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_ingestion_run(
        &self,
        id: &IngestionRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<IngestionRun>, Self::Error> {
        Ok(self
            .ingestion_runs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|run| {
                run.id() == id
                    && run.scope().tenant_id() == tenant_id
                    && run.scope().project_id() == project_id
            })
            .cloned())
    }

    async fn upsert_ingestion_run_output(
        &self,
        output: &IngestionRunOutputRecord,
    ) -> Result<(), Self::Error> {
        let mut outputs = self
            .ingestion_run_outputs
            .lock()
            .map_err(|error| error.to_string())?;
        outputs.retain(|existing| existing != output);
        outputs.push(output.clone());
        Ok(())
    }

    async fn list_ingestion_run_outputs(
        &self,
        run_id: &IngestionRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<IngestionRunOutputRecord>, Self::Error> {
        if self
            .find_ingestion_run(run_id, tenant_id, project_id)
            .await?
            .is_none()
        {
            return Ok(Vec::new());
        }
        Ok(self
            .ingestion_run_outputs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|output| output.run_id() == run_id)
            .cloned()
            .collect())
    }
}
