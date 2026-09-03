//! Durable storage for verified-computation IR definitions.
//!
//! Definitions are content-addressed and append-only. Registering the same
//! bytes twice is idempotent — the digest is the identity, so there is nothing
//! to conflict about — and registering different bytes creates a new version
//! rather than replacing anything. A run that says it executed a version can
//! therefore always point at the exact bytes of that version.
//!
//! Every read and write takes tenant and project. They are not a filter applied
//! after the fact; they are part of the key.

use capsulet_ir::admission::AdmissionRecord;
use capsulet_ir::definition::Definition;
use capsulet_ir::digest::Digest;
use sqlx::Row;

use crate::{PostgresStore, PostgresStoreError};

/// One stored version of a definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrDefinitionVersion {
    pub tenant_id: String,
    pub project_id: String,
    pub definition_id: String,
    pub name: String,
    pub version: String,
    pub digest: String,
    pub schema_version: String,
    /// The exact bytes that were digested.
    pub canonical_bytes: String,
}

impl IrDefinitionVersion {
    /// Reads the definition back from its stored bytes.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the stored bytes do not parse, which
    /// would mean the row was written by an incompatible build.
    pub fn definition(&self) -> Result<Definition, PostgresStoreError> {
        serde_json::from_str(&self.canonical_bytes)
            .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))
    }
}

impl PostgresStore {
    /// Registers a definition version.
    ///
    /// Idempotent by digest: the same bytes registered twice leave one row.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the definition cannot be encoded or
    /// the write fails.
    pub async fn insert_ir_definition_version(
        &self,
        tenant_id: &str,
        project_id: &str,
        definition: &Definition,
        admission: &AdmissionRecord,
    ) -> Result<Digest, PostgresStoreError> {
        let bytes = capsulet_ir::to_canonical_bytes(definition)
            .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?;
        let digest = Digest::of(&bytes);
        let canonical = String::from_utf8(bytes)
            .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?;
        let admission_json = serde_json::to_value(admission)
            .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?;

        let mut transaction = self.pool.begin().await?;

        // The definition row and its first version are created together. A
        // definition with no versions would be a name with nothing behind it.
        sqlx::query(
            r"
            INSERT INTO ir_definitions (id, tenant_id, project_id, name)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (tenant_id, project_id, id) DO NOTHING
            ",
        )
        .bind(definition.id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .bind(&definition.name)
        .execute(&mut *transaction)
        .await?;

        sqlx::query(
            r"
            INSERT INTO ir_definition_versions (
                tenant_id, project_id, definition_id, digest, version,
                schema_version, canonical_bytes, admission
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8::jsonb)
            ON CONFLICT (tenant_id, project_id, digest) DO NOTHING
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(definition.id.as_str())
        .bind(digest.to_string())
        .bind(&definition.version)
        .bind(definition.schema_version.to_string())
        .bind(&canonical)
        .bind(admission_json)
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;
        Ok(digest)
    }

    /// Lists the definitions in one project, newest version first.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails.
    pub async fn list_ir_definition_versions(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<IrDefinitionVersion>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT v.tenant_id, v.project_id, v.definition_id, d.name, v.version,
                   v.digest, v.schema_version, v.canonical_bytes
            FROM ir_definition_versions v
            JOIN ir_definitions d
              ON d.tenant_id = v.tenant_id
             AND d.project_id = v.project_id
             AND d.id = v.definition_id
            WHERE v.tenant_id = $1 AND v.project_id = $2
            ORDER BY v.created_at DESC
            LIMIT $3
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(row_to_version).collect())
    }

    /// Reads one version by digest.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails.
    pub async fn get_ir_definition_version(
        &self,
        tenant_id: &str,
        project_id: &str,
        digest: &str,
    ) -> Result<Option<IrDefinitionVersion>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT v.tenant_id, v.project_id, v.definition_id, d.name, v.version,
                   v.digest, v.schema_version, v.canonical_bytes
            FROM ir_definition_versions v
            JOIN ir_definitions d
              ON d.tenant_id = v.tenant_id
             AND d.project_id = v.project_id
             AND d.id = v.definition_id
            WHERE v.tenant_id = $1 AND v.project_id = $2 AND v.digest = $3
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(digest)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.as_ref().map(row_to_version))
    }
}

fn row_to_version(row: &sqlx::postgres::PgRow) -> IrDefinitionVersion {
    IrDefinitionVersion {
        tenant_id: row.get("tenant_id"),
        project_id: row.get("project_id"),
        definition_id: row.get("definition_id"),
        name: row.get("name"),
        version: row.get("version"),
        digest: row.get("digest"),
        schema_version: row.get("schema_version"),
        canonical_bytes: row.get("canonical_bytes"),
    }
}
