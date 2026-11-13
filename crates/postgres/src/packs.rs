//! Domain packs a project installed.

use serde_json::Value;
use sqlx::Row;

use crate::{PostgresStore, PostgresStoreError};

/// One installed pack version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainPackRecord {
    pub tenant_id: String,
    pub project_id: String,
    pub name: String,
    pub version: String,
    /// What a definition pins and a certificate names.
    pub manifest_digest: String,
    /// The key that signed it.
    pub signed_by: String,
    /// The whole pack, as it arrived.
    pub bundle: Value,
    pub installed_by: String,
}

impl PostgresStore {
    /// Records an installed pack version, unless that version is already held.
    ///
    /// Returns whether it was recorded. A version is held once and never
    /// changes, so a second install of it is reported, not written.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the insert fails.
    pub async fn insert_domain_pack(
        &self,
        record: &DomainPackRecord,
    ) -> Result<bool, PostgresStoreError> {
        let inserted = sqlx::query(
            r"
            INSERT INTO domain_packs (
                tenant_id, project_id, name, version, manifest_digest, signed_by, bundle, installed_by
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (tenant_id, project_id, name, version) DO NOTHING
            ",
        )
        .bind(&record.tenant_id)
        .bind(&record.project_id)
        .bind(&record.name)
        .bind(&record.version)
        .bind(&record.manifest_digest)
        .bind(&record.signed_by)
        .bind(&record.bundle)
        .bind(&record.installed_by)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(inserted == 1)
    }

    /// Every pack version this project installed, oldest first.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails.
    pub async fn list_domain_packs(
        &self,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<DomainPackRecord>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT tenant_id, project_id, name, version, manifest_digest, signed_by, bundle, installed_by
            FROM domain_packs
            WHERE tenant_id = $1 AND project_id = $2
            ORDER BY installed_at ASC, name ASC, version ASC
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_pack).collect()
    }

    /// One installed pack version, when this project holds it.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails.
    pub async fn find_domain_pack(
        &self,
        tenant_id: &str,
        project_id: &str,
        name: &str,
        version: &str,
    ) -> Result<Option<DomainPackRecord>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT tenant_id, project_id, name, version, manifest_digest, signed_by, bundle, installed_by
            FROM domain_packs
            WHERE tenant_id = $1 AND project_id = $2 AND name = $3 AND version = $4
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(name)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_pack).transpose()
    }
}

fn row_to_pack(row: &sqlx::postgres::PgRow) -> Result<DomainPackRecord, PostgresStoreError> {
    Ok(DomainPackRecord {
        tenant_id: row.try_get("tenant_id")?,
        project_id: row.try_get("project_id")?,
        name: row.try_get("name")?,
        version: row.try_get("version")?,
        manifest_digest: row.try_get("manifest_digest")?,
        signed_by: row.try_get("signed_by")?,
        bundle: row.try_get("bundle")?,
        installed_by: row.try_get("installed_by")?,
    })
}
