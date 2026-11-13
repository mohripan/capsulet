//! Memory contracts.

#![expect(
    clippy::missing_errors_doc,
    reason = "memory persistence methods all return PostgresStoreError for SQL and domain conversion failures"
)]

use capsulet_core::{MemoryContract, MemoryContractId};
use sqlx::Row;

use super::{scope, scope_guard};
use crate::{PostgresStore, PostgresStoreError};

impl PostgresStore {
    pub async fn upsert_memory_contract(
        &self,
        contract: &MemoryContract,
    ) -> Result<(), PostgresStoreError> {
        let result = sqlx::query(
            r"
            INSERT INTO memory_contracts (id, tenant_id, project_id, name, source, updated_at)
            VALUES ($1, $2, $3, $4, $5, now())
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                source = EXCLUDED.source,
                updated_at = now()
            WHERE memory_contracts.tenant_id = EXCLUDED.tenant_id
              AND memory_contracts.project_id = EXCLUDED.project_id
            ",
        )
        .bind(contract.id().as_str())
        .bind(contract.scope().tenant_id())
        .bind(contract.scope().project_id())
        .bind(contract.name())
        .bind(contract.source())
        .execute(&self.pool)
        .await?;
        scope_guard(&result, "memory_contracts", contract.id().as_str())
    }

    pub async fn list_memory_contracts(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<MemoryContract>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, name, source
            FROM memory_contracts
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
        rows.iter().map(row_to_contract).collect()
    }

    pub async fn find_memory_contract(
        &self,
        id: &MemoryContractId,
    ) -> Result<Option<MemoryContract>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, name, source
            FROM memory_contracts
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_contract).transpose()
    }

    /// The same lookup, within one project: another project's record matches no row.
    pub async fn find_memory_contract_in_project(
        &self,
        id: &MemoryContractId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<MemoryContract>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, name, source
            FROM memory_contracts
            WHERE id = $1 AND tenant_id = $2 AND project_id = $3
            ",
        )
        .bind(id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_contract).transpose()
    }
}

pub(super) fn row_to_contract(
    row: &sqlx::postgres::PgRow,
) -> Result<MemoryContract, PostgresStoreError> {
    MemoryContract::parse_scoped(
        MemoryContractId::new(row.try_get::<String, _>("id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        scope(row)?,
        row.try_get::<String, _>("name")?,
        row.try_get::<String, _>("source")?,
    )
    .map_err(PostgresStoreError::MemoryContract)
}
