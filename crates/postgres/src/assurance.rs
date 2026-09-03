//! Durable storage for platform certificates and the evidence they cite.
//!
//! The certificate's own canonical bytes are the authority. Everything else in
//! these tables — the verdict column, the obligation rows — is a projection, so
//! that "which results are still conditional, and how old are their residuals"
//! is a query rather than a scan over documents. A projection that disagreed
//! with the bytes would be a bug in the writer, not a second opinion, so the
//! bytes are stored alongside and the reader can always go back to them.
//!
//! Evidence bytes are not stored here. They live in object storage under their
//! digest, because a metadata store that also holds every scanner log stops
//! being either.

use capsulet_ir::correctness::certificate::Certificate;
use sqlx::Row;

use crate::{PostgresStore, PostgresStoreError};

/// Where a piece of evidence lives, and what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceLocation {
    pub digest: String,
    pub media_type: String,
    pub byte_length: i64,
    /// The object-storage key, derived from the digest.
    pub object_key: String,
}

impl EvidenceLocation {
    /// The object-storage key for a digest.
    ///
    /// Content-addressed, so the same bytes stored twice occupy one key and a
    /// key can never point at bytes that are not what it names.
    #[must_use]
    pub fn key_for(digest: &str) -> String {
        format!("assurance/evidence/{digest}")
    }
}

/// A stored certificate, with the columns worth querying pulled out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCertificate {
    pub id: String,
    pub tenant_id: String,
    pub project_id: String,
    pub definition_digest: String,
    pub verdict: String,
    pub mode: String,
    pub replay_digest: String,
    pub canonical_bytes: String,
}

impl StoredCertificate {
    /// Reads the certificate back from its stored bytes.
    ///
    /// Deserialization re-checks the seal, so a row whose bytes were altered
    /// does not come back as a certificate at all.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the stored bytes do not parse or the
    /// seal does not match.
    pub fn certificate(&self) -> Result<Certificate, PostgresStoreError> {
        serde_json::from_str(&self.canonical_bytes)
            .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))
    }
}

impl PostgresStore {
    /// Records a certificate and projects its obligations.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the certificate cannot be encoded or
    /// the write fails.
    pub async fn insert_assurance_certificate(
        &self,
        tenant_id: &str,
        project_id: &str,
        certificate: &Certificate,
    ) -> Result<(), PostgresStoreError> {
        let body = certificate.body();
        let bytes = capsulet_ir::to_canonical_bytes(certificate)
            .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?;
        let canonical = String::from_utf8(bytes)
            .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?;

        let mut transaction = self.pool.begin().await?;

        sqlx::query(
            r"
            INSERT INTO assurance_certificates (
                tenant_id, project_id, id, definition_digest, schema_version,
                verdict, mode, policy_version, kernel_version, replay_digest, canonical_bytes
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (tenant_id, project_id, id) DO NOTHING
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(body.id.as_str())
        .bind(body.subject.definition.to_string())
        .bind(body.schema_version.to_string())
        .bind(body.verdict.as_str())
        .bind(body.mode.as_str())
        .bind(&body.policy_version)
        .bind(&body.kernel_version)
        .bind(certificate.replay_digest().to_string())
        .bind(&canonical)
        .execute(&mut *transaction)
        .await?;

        for obligation in &body.obligations {
            sqlx::query(
                r"
                INSERT INTO assurance_obligations (
                    tenant_id, project_id, certificate_id, obligation_id, contract, state, owner
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                ON CONFLICT (tenant_id, project_id, certificate_id, obligation_id) DO NOTHING
                ",
            )
            .bind(tenant_id)
            .bind(project_id)
            .bind(body.id.as_str())
            .bind(obligation.statement.id.as_str())
            .bind(obligation.contract.as_str())
            .bind(obligation.state.as_str())
            .bind(obligation.statement.owner.as_str())
            .execute(&mut *transaction)
            .await?;
        }

        for evidence in &body.evidence {
            let digest = evidence.content.to_string();
            let byte_length = i64::try_from(evidence.byte_length).unwrap_or(i64::MAX);
            sqlx::query(
                r"
                INSERT INTO assurance_evidence (
                    tenant_id, project_id, digest, media_type, byte_length, object_key
                )
                VALUES ($1, $2, $3, $4, $5, $6)
                ON CONFLICT (tenant_id, project_id, digest) DO NOTHING
                ",
            )
            .bind(tenant_id)
            .bind(project_id)
            .bind(&digest)
            .bind(&evidence.media_type)
            .bind(byte_length)
            .bind(EvidenceLocation::key_for(&digest))
            .execute(&mut *transaction)
            .await?;
        }

        transaction.commit().await?;
        Ok(())
    }

    /// Lists certificates for one project, newest first.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails.
    pub async fn list_assurance_certificates(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<StoredCertificate>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT tenant_id, project_id, id, definition_digest, verdict, mode,
                   replay_digest, canonical_bytes
            FROM assurance_certificates
            WHERE tenant_id = $1 AND project_id = $2
            ORDER BY created_at DESC
            LIMIT $3
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(row_to_certificate).collect())
    }

    /// Reads one certificate.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails.
    pub async fn get_assurance_certificate(
        &self,
        tenant_id: &str,
        project_id: &str,
        id: &str,
    ) -> Result<Option<StoredCertificate>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT tenant_id, project_id, id, definition_digest, verdict, mode,
                   replay_digest, canonical_bytes
            FROM assurance_certificates
            WHERE tenant_id = $1 AND project_id = $2 AND id = $3
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.as_ref().map(row_to_certificate))
    }

    /// The evidence a project holds, by digest.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails.
    pub async fn get_assurance_evidence(
        &self,
        tenant_id: &str,
        project_id: &str,
        digest: &str,
    ) -> Result<Option<EvidenceLocation>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT digest, media_type, byte_length, object_key
            FROM assurance_evidence
            WHERE tenant_id = $1 AND project_id = $2 AND digest = $3
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(digest)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|row| EvidenceLocation {
            digest: row.get("digest"),
            media_type: row.get("media_type"),
            byte_length: row.get("byte_length"),
            object_key: row.get("object_key"),
        }))
    }

    /// Counts obligations by state, for the residual-age metrics the product
    /// design asks to be reported honestly.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails.
    pub async fn count_outstanding_obligations(
        &self,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<i64, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT COUNT(*) AS outstanding
            FROM assurance_obligations
            WHERE tenant_id = $1 AND project_id = $2
              AND state IN ('assumed', 'waived', 'residual')
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.get("outstanding"))
    }
}

fn row_to_certificate(row: &sqlx::postgres::PgRow) -> StoredCertificate {
    StoredCertificate {
        id: row.get("id"),
        tenant_id: row.get("tenant_id"),
        project_id: row.get("project_id"),
        definition_digest: row.get("definition_digest"),
        verdict: row.get("verdict"),
        mode: row.get("mode"),
        replay_digest: row.get("replay_digest"),
        canonical_bytes: row.get("canonical_bytes"),
    }
}
