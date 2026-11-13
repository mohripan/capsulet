//! Durable storage for kernel certificates.

use capsulet_kernel::{Certificate, ReadingDecision, ReadingReview, Verdict};
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

/// A person's decision on one reading of a stored certificate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadingReviewRecord {
    pub tenant_id: String,
    pub project_id: String,
    pub certificate_id: String,
    pub review: ReadingReview,
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
                discharged, residuals, errors, alphabet_digest, replay_digest, model,
                reading_authority
            )
            VALUES ($1, $2, $3, $4, $5, $6::jsonb, $7::jsonb, $8::jsonb, $9::jsonb, $10, $11, $12,
                    $13)
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
        .bind(&certificate.reading_authority)
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
                   discharged, residuals, errors, alphabet_digest, replay_digest, model,
                   reading_authority
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

    /// One certificate, when it belongs to this project.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails or the row cannot be
    /// reconstructed.
    pub async fn find_certificate(
        &self,
        tenant_id: &str,
        project_id: &str,
        id: &str,
    ) -> Result<Option<CertificateRecord>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, question, verdict, goal,
                   discharged, residuals, errors, alphabet_digest, replay_digest, model,
                   reading_authority
            FROM reasoning_certificates
            WHERE tenant_id = $1 AND project_id = $2 AND id = $3
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_certificate).transpose()
    }

    /// Records a decision on one reading, unless that reading already has one.
    ///
    /// Returns whether it was recorded. A reading is decided once: the table
    /// refuses to change a decision, so a second one is reported, not written.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the insert fails, including when the
    /// certificate is not in the project the review names.
    pub async fn insert_reading_review(
        &self,
        record: &ReadingReviewRecord,
    ) -> Result<bool, PostgresStoreError> {
        let review = &record.review;
        let inserted = sqlx::query(
            r"
            INSERT INTO reasoning_reading_reviews (
                tenant_id, project_id, certificate_id, reading, decision, reviewer, authority, rationale
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (tenant_id, project_id, certificate_id, reading) DO NOTHING
            ",
        )
        .bind(&record.tenant_id)
        .bind(&record.project_id)
        .bind(&record.certificate_id)
        .bind(&review.reading)
        .bind(review.decision.as_str())
        .bind(&review.reviewer)
        .bind(&review.authority)
        .bind(&review.rationale)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(inserted == 1)
    }

    /// The decisions recorded on these certificates' readings, in this project.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails or a stored decision
    /// is not one this build knows.
    pub async fn list_reading_reviews(
        &self,
        tenant_id: &str,
        project_id: &str,
        certificate_ids: &[String],
    ) -> Result<Vec<ReadingReviewRecord>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT certificate_id, reading, decision, reviewer, authority, rationale
            FROM reasoning_reading_reviews
            WHERE tenant_id = $1 AND project_id = $2 AND certificate_id = ANY($3)
            ORDER BY created_at ASC, certificate_id ASC, reading ASC
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(certificate_ids)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|row| {
                let decision: String = row.try_get("decision")?;
                let decision = ReadingDecision::parse(&decision).ok_or_else(|| {
                    PostgresStoreError::InvalidPersistedValue(format!(
                        "unknown reading decision {decision}"
                    ))
                })?;
                Ok(ReadingReviewRecord {
                    tenant_id: tenant_id.to_string(),
                    project_id: project_id.to_string(),
                    certificate_id: row.try_get("certificate_id")?,
                    review: ReadingReview {
                        reading: row.try_get("reading")?,
                        decision,
                        reviewer: row.try_get("reviewer")?,
                        authority: row.try_get("authority")?,
                        rationale: row.try_get("rationale")?,
                    },
                })
            })
            .collect()
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
            // Not a stored column. The bound this certificate was decided
            // under is genuinely unknown here, and saying so beats filling in
            // the running build's constant as though it had been recorded.
            derivation_depth_limit: None,
            reading_authority: row.try_get("reading_authority")?,
        },
        alphabet_digest: row.try_get("alphabet_digest")?,
        model: row.try_get("model")?,
    })
}

fn from_json<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, PostgresStoreError> {
    serde_json::from_value(value)
        .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))
}
