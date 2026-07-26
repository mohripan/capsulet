//! Durable storage for kernel certificates.

use capsulet_kernel::{Certificate, Verdict};
use serde_json::Value;
use sqlx::Row;

use crate::{PostgresStore, PostgresStoreError};

/// A certificate together with the context needed to audit it later.
#[derive(Debug, Clone, PartialEq)]
pub struct CertificateRecord {
    pub id: String,
    pub tenant_id: String,
    pub project_id: String,
    pub question: String,
    pub certificate: Certificate,
    pub alphabet_digest: String,
    pub model: String,
}

impl PostgresStore {
    /// Records one kernel decision.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the insert or serialization fails.
    pub async fn insert_certificate(
        &self,
        record: &CertificateRecord,
    ) -> Result<(), PostgresStoreError> {
        let certificate = &record.certificate;
        sqlx::query(
            r"
            INSERT INTO reasoning_certificates (
                id, tenant_id, project_id, question, verdict, goal,
                discharged, residuals, errors, alphabet_digest, replay_digest, model
            )
            VALUES ($1, $2, $3, $4, $5, $6::jsonb, $7::jsonb, $8::jsonb, $9::jsonb, $10, $11, $12)
            ON CONFLICT (id) DO NOTHING
            ",
        )
        .bind(&record.id)
        .bind(&record.tenant_id)
        .bind(&record.project_id)
        .bind(&record.question)
        .bind(certificate.verdict.as_str())
        .bind(to_json(&certificate.goal)?)
        .bind(to_json(&certificate.discharged)?)
        .bind(to_json(&certificate.residuals)?)
        .bind(to_json(&certificate.errors)?)
        .bind(&record.alphabet_digest)
        .bind(&certificate.replay_digest)
        .bind(&record.model)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Lists recent certificates for one project.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails or a stored row
    /// cannot be reconstructed.
    pub async fn list_certificates(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<CertificateRecord>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, question, verdict, goal,
                   discharged, residuals, errors, alphabet_digest, replay_digest, model
            FROM reasoning_certificates
            WHERE tenant_id = $1 AND project_id = $2
            ORDER BY created_at DESC, id ASC
            LIMIT $3
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_certificate).collect()
    }
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<Value, PostgresStoreError> {
    serde_json::to_value(value)
        .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))
}

fn row_to_certificate(
    row: &sqlx::postgres::PgRow,
) -> Result<CertificateRecord, PostgresStoreError> {
    let verdict = match row.try_get::<String, _>("verdict")?.as_str() {
        "accepted" => Verdict::Accepted,
        "conditional" => Verdict::Conditional,
        "rejected" => Verdict::Rejected,
        other => {
            return Err(PostgresStoreError::InvalidPersistedValue(format!(
                "unknown certificate verdict {other}"
            )));
        }
    };
    Ok(CertificateRecord {
        id: row.try_get("id")?,
        tenant_id: row.try_get("tenant_id")?,
        project_id: row.try_get("project_id")?,
        question: row.try_get("question")?,
        certificate: Certificate {
            verdict,
            goal: from_json(row.try_get("goal")?)?,
            discharged: from_json(row.try_get("discharged")?)?,
            residuals: from_json(row.try_get("residuals")?)?,
            errors: from_json(row.try_get("errors")?)?,
            replay_digest: row.try_get("replay_digest")?,
        },
        alphabet_digest: row.try_get("alphabet_digest")?,
        model: row.try_get("model")?,
    })
}

fn from_json<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, PostgresStoreError> {
    serde_json::from_value(value)
        .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))
}
