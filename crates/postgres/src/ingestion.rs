use capsulet_core::{
    Authority, IngestionConnector, IngestionConnectorConfig, IngestionConnectorId,
    IngestionConnectorKind, IngestionRun, IngestionRunId, IngestionRunOutputRecord,
    IngestionRunStatus, MemoryScope,
};
use serde_json::{Value, json};
use sqlx::Row;

use crate::{PostgresStore, PostgresStoreError};

impl PostgresStore {
    /// Inserts or updates an ingestion connector.
    ///
    /// # Errors
    ///
    /// Returns an error when `PostgreSQL` rejects or cannot execute the query.
    pub async fn upsert_ingestion_connector(
        &self,
        connector: &IngestionConnector,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO ingestion_connectors
                (id, tenant_id, project_id, name, kind, config, enabled, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, now())
            ON CONFLICT (id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                project_id = EXCLUDED.project_id,
                name = EXCLUDED.name,
                kind = EXCLUDED.kind,
                config = EXCLUDED.config,
                enabled = EXCLUDED.enabled,
                updated_at = now()
            ",
        )
        .bind(connector.id().as_str())
        .bind(connector.scope().tenant_id())
        .bind(connector.scope().project_id())
        .bind(connector.name())
        .bind(connector.kind().to_string())
        .bind(connector_config_json(connector.config()))
        .bind(connector.enabled())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Lists ingestion connectors for a tenant/project scope.
    ///
    /// # Errors
    ///
    /// Returns an error when `PostgreSQL` cannot execute the query or a persisted row cannot be
    /// rehydrated into a domain connector.
    pub async fn list_ingestion_connectors(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<IngestionConnector>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, name, kind, config, enabled
            FROM ingestion_connectors
            WHERE tenant_id = $1 AND project_id = $2
            ORDER BY updated_at DESC, id ASC
            LIMIT $3
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_ingestion_connector).collect()
    }

    /// Finds one ingestion connector by identifier.
    ///
    /// # Errors
    ///
    /// Returns an error when `PostgreSQL` cannot execute the query or the persisted row cannot be
    /// rehydrated into a domain connector.
    pub async fn find_ingestion_connector(
        &self,
        id: &IngestionConnectorId,
    ) -> Result<Option<IngestionConnector>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, name, kind, config, enabled
            FROM ingestion_connectors
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_ingestion_connector).transpose()
    }

    /// Inserts or updates an ingestion run snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error when counters cannot be converted for storage or `PostgreSQL` rejects or
    /// cannot execute the query.
    pub async fn upsert_ingestion_run(&self, run: &IngestionRun) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO ingestion_runs
                (
                    id, tenant_id, project_id, connector_id, status, error,
                    source_count, evidence_count, entity_count, claim_count,
                    event_count, relationship_count, finished_at, updated_at
                )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                CASE WHEN $5 IN ('succeeded', 'failed') THEN now() ELSE NULL END,
                now()
            )
            ON CONFLICT (id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                project_id = EXCLUDED.project_id,
                connector_id = EXCLUDED.connector_id,
                status = EXCLUDED.status,
                error = EXCLUDED.error,
                source_count = EXCLUDED.source_count,
                evidence_count = EXCLUDED.evidence_count,
                entity_count = EXCLUDED.entity_count,
                claim_count = EXCLUDED.claim_count,
                event_count = EXCLUDED.event_count,
                relationship_count = EXCLUDED.relationship_count,
                finished_at = EXCLUDED.finished_at,
                updated_at = now()
            ",
        )
        .bind(run.id().as_str())
        .bind(run.scope().tenant_id())
        .bind(run.scope().project_id())
        .bind(run.connector_id().as_str())
        .bind(run.status().to_string())
        .bind(run.error())
        .bind(
            i32::try_from(run.source_count())
                .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?,
        )
        .bind(
            i32::try_from(run.evidence_count())
                .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?,
        )
        .bind(
            i32::try_from(run.entity_count())
                .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?,
        )
        .bind(
            i32::try_from(run.claim_count())
                .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?,
        )
        .bind(
            i32::try_from(run.event_count())
                .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?,
        )
        .bind(
            i32::try_from(run.relationship_count())
                .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Lists ingestion runs for a tenant/project scope.
    ///
    /// # Errors
    ///
    /// Returns an error when `PostgreSQL` cannot execute the query or a persisted row cannot be
    /// rehydrated into a domain run.
    pub async fn list_ingestion_runs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<IngestionRun>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT
                id, tenant_id, project_id, connector_id, status, error,
                source_count, evidence_count, entity_count, claim_count,
                event_count, relationship_count
            FROM ingestion_runs
            WHERE tenant_id = $1 AND project_id = $2
            ORDER BY started_at DESC, id ASC
            LIMIT $3
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_ingestion_run).collect()
    }

    /// Finds one ingestion run by identifier.
    ///
    /// # Errors
    ///
    /// Returns an error when `PostgreSQL` cannot execute the query or the persisted row cannot be
    /// rehydrated into a domain run.
    pub async fn find_ingestion_run(
        &self,
        id: &IngestionRunId,
    ) -> Result<Option<IngestionRun>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT
                id, tenant_id, project_id, connector_id, status, error,
                source_count, evidence_count, entity_count, claim_count,
                event_count, relationship_count
            FROM ingestion_runs
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_ingestion_run).transpose()
    }

    /// Inserts an ingestion run output reference.
    ///
    /// # Errors
    ///
    /// Returns an error when `PostgreSQL` rejects or cannot execute the query.
    pub async fn upsert_ingestion_run_output(
        &self,
        output: &IngestionRunOutputRecord,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO ingestion_run_outputs (run_id, kind, memory_id)
            VALUES ($1, $2, $3)
            ON CONFLICT (run_id, kind, memory_id) DO NOTHING
            ",
        )
        .bind(output.run_id().as_str())
        .bind(output.kind())
        .bind(output.memory_id())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Lists generated memory references for an ingestion run.
    ///
    /// # Errors
    ///
    /// Returns an error when `PostgreSQL` cannot execute the query or a persisted row cannot be
    /// rehydrated into a domain output record.
    pub async fn list_ingestion_run_outputs(
        &self,
        run_id: &IngestionRunId,
    ) -> Result<Vec<IngestionRunOutputRecord>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT run_id, kind, memory_id
            FROM ingestion_run_outputs
            WHERE run_id = $1
            ORDER BY kind ASC, memory_id ASC
            ",
        )
        .bind(run_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_ingestion_run_output).collect()
    }
}

fn connector_config_json(config: &IngestionConnectorConfig) -> Value {
    json!({
        "title": config.title(),
        "content": config.content(),
        "content_type": config.content_type(),
        "uri": config.uri(),
        "authority": config.authority().to_string(),
    })
}

fn row_to_ingestion_connector(
    row: &sqlx::postgres::PgRow,
) -> Result<IngestionConnector, PostgresStoreError> {
    let id: String = row.try_get("id")?;
    let tenant_id: String = row.try_get("tenant_id")?;
    let project_id: String = row.try_get("project_id")?;
    let name: String = row.try_get("name")?;
    let kind: String = row.try_get("kind")?;
    let config: Value = row.try_get("config")?;
    let enabled: bool = row.try_get("enabled")?;
    let kind = parse_ingestion_connector_kind(&kind)?;
    let config = parse_connector_config(&config)?;
    IngestionConnector::new(
        IngestionConnectorId::new(id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        MemoryScope::new(tenant_id, project_id)?,
        name,
        kind,
        config,
        enabled,
    )
    .map_err(PostgresStoreError::Ingestion)
}

fn row_to_ingestion_run(row: &sqlx::postgres::PgRow) -> Result<IngestionRun, PostgresStoreError> {
    let id: String = row.try_get("id")?;
    let tenant_id: String = row.try_get("tenant_id")?;
    let project_id: String = row.try_get("project_id")?;
    let connector_id: String = row.try_get("connector_id")?;
    let status: String = row.try_get("status")?;
    let error: Option<String> = row.try_get("error")?;
    Ok(IngestionRun::recorded(
        IngestionRunId::new(id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        MemoryScope::new(tenant_id, project_id)?,
        IngestionConnectorId::new(connector_id)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        parse_ingestion_run_status(&status)?,
        error,
        positive_i32(row.try_get("source_count")?, "source count")?,
        positive_i32(row.try_get("evidence_count")?, "evidence count")?,
        positive_i32(row.try_get("entity_count")?, "entity count")?,
        positive_i32(row.try_get("claim_count")?, "claim count")?,
        positive_i32(row.try_get("event_count")?, "event count")?,
        positive_i32(row.try_get("relationship_count")?, "relationship count")?,
    ))
}

fn row_to_ingestion_run_output(
    row: &sqlx::postgres::PgRow,
) -> Result<IngestionRunOutputRecord, PostgresStoreError> {
    let run_id: String = row.try_get("run_id")?;
    let kind: String = row.try_get("kind")?;
    let memory_id: String = row.try_get("memory_id")?;
    IngestionRunOutputRecord::new(
        IngestionRunId::new(run_id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        kind,
        memory_id,
    )
    .map_err(PostgresStoreError::Ingestion)
}

fn parse_connector_config(value: &Value) -> Result<IngestionConnectorConfig, PostgresStoreError> {
    Ok(IngestionConnectorConfig::local_text(
        required_string(value, "title")?,
        required_string(value, "content")?,
        required_string(value, "content_type")?,
        optional_string(value, "uri")?,
        parse_authority(&required_string(value, "authority")?)?,
    ))
}

fn required_string(value: &Value, key: &str) -> Result<String, PostgresStoreError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| PostgresStoreError::InvalidPersistedValue(format!("missing {key}")))
}

fn optional_string(value: &Value, key: &str) -> Result<Option<String>, PostgresStoreError> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value.as_str().map(str::to_string).map(Some).ok_or_else(|| {
            PostgresStoreError::InvalidPersistedValue(format!("{key} must be a string"))
        }),
    }
}

fn parse_ingestion_connector_kind(
    value: &str,
) -> Result<IngestionConnectorKind, PostgresStoreError> {
    match value {
        "local_text" => Ok(IngestionConnectorKind::LocalText),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown ingestion connector kind: {value}"
        ))),
    }
}

fn parse_ingestion_run_status(value: &str) -> Result<IngestionRunStatus, PostgresStoreError> {
    match value {
        "queued" => Ok(IngestionRunStatus::Queued),
        "running" => Ok(IngestionRunStatus::Running),
        "succeeded" => Ok(IngestionRunStatus::Succeeded),
        "failed" => Ok(IngestionRunStatus::Failed),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown ingestion run status: {value}"
        ))),
    }
}

fn parse_authority(value: &str) -> Result<Authority, PostgresStoreError> {
    match value {
        "low" => Ok(Authority::Low),
        "medium" => Ok(Authority::Medium),
        "high" => Ok(Authority::High),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown authority: {value}"
        ))),
    }
}

fn positive_i32(value: i32, label: &str) -> Result<u32, PostgresStoreError> {
    if value < 0 {
        return Err(PostgresStoreError::InvalidPersistedValue(format!(
            "negative {label}"
        )));
    }
    u32::try_from(value)
        .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))
}
