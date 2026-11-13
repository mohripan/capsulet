//! Sources, their bytes, and the evidence cited from them.

#![expect(
    clippy::missing_errors_doc,
    reason = "memory persistence methods all return PostgresStoreError for SQL and domain conversion failures"
)]

use capsulet_core::{Evidence, EvidenceId, EvidenceSpan, Source, SourceContent, SourceId};
use sqlx::Row;

use super::{parse_authority, scope, scope_guard};
use crate::{PostgresStore, PostgresStoreError};

impl PostgresStore {
    pub async fn upsert_memory_source(&self, source: &Source) -> Result<(), PostgresStoreError> {
        let result = sqlx::query(
            r"
            INSERT INTO memory_sources (id, tenant_id, project_id, kind, uri, title, authority, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, now())
            ON CONFLICT (id) DO UPDATE SET
                kind = EXCLUDED.kind,
                uri = EXCLUDED.uri,
                title = EXCLUDED.title,
                authority = EXCLUDED.authority,
                updated_at = now()
            WHERE memory_sources.tenant_id = EXCLUDED.tenant_id
              AND memory_sources.project_id = EXCLUDED.project_id
            ",
        )
        .bind(source.id().as_str())
        .bind(source.scope().tenant_id())
        .bind(source.scope().project_id())
        .bind(source.kind())
        .bind(source.uri())
        .bind(source.title())
        .bind(source.authority().to_string())
        .execute(&self.pool)
        .await?;
        scope_guard(&result, "memory_sources", source.id().as_str())
    }

    pub async fn list_memory_sources(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Source>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, kind, uri, title, authority
            FROM memory_sources
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
        rows.iter().map(row_to_source).collect()
    }

    pub async fn find_memory_source(
        &self,
        id: &SourceId,
    ) -> Result<Option<Source>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, kind, uri, title, authority
            FROM memory_sources
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_source).transpose()
    }

    pub async fn upsert_memory_evidence(
        &self,
        evidence: &Evidence,
    ) -> Result<(), PostgresStoreError> {
        let result = sqlx::query(
            r"
            INSERT INTO memory_evidence (id, tenant_id, project_id, source_id, locator, excerpt, observed_at, span_start, span_end, source_content_hash, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, now())
            ON CONFLICT (id) DO UPDATE SET
                source_id = EXCLUDED.source_id,
                locator = EXCLUDED.locator,
                excerpt = EXCLUDED.excerpt,
                observed_at = EXCLUDED.observed_at,
                span_start = EXCLUDED.span_start,
                span_end = EXCLUDED.span_end,
                source_content_hash = EXCLUDED.source_content_hash,
                updated_at = now()
            WHERE memory_evidence.tenant_id = EXCLUDED.tenant_id
              AND memory_evidence.project_id = EXCLUDED.project_id
            ",
        )
        .bind(evidence.id().as_str())
        .bind(evidence.scope().tenant_id())
        .bind(evidence.scope().project_id())
        .bind(evidence.source_id().as_str())
        .bind(evidence.locator())
        .bind(evidence.excerpt())
        .bind(evidence.observed_at())
        .bind(evidence.span().map(|span| i64::try_from(span.start()).unwrap_or(i64::MAX)))
        .bind(evidence.span().map(|span| i64::try_from(span.end()).unwrap_or(i64::MAX)))
        .bind(evidence.span().map(EvidenceSpan::source_content_hash))
        .execute(&self.pool)
        .await?;
        scope_guard(&result, "memory_evidence", evidence.id().as_str())
    }

    /// Stores the immutable text a source's spans resolve against.
    ///
    /// Content is keyed by its own digest, so re-storing identical text is a
    /// no-op and storing changed text adds a version rather than replacing one.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the insert fails.
    pub async fn insert_memory_source_content(
        &self,
        content: &SourceContent,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO memory_source_contents (source_id, content_hash, content, byte_length)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (source_id, content_hash) DO NOTHING
            ",
        )
        .bind(content.source_id().as_str())
        .bind(content.content_hash())
        .bind(content.text())
        .bind(i64::try_from(content.byte_length()).unwrap_or(i64::MAX))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Loads one stored version of a source's text by its digest.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails or the stored row
    /// cannot be reconstructed.
    pub async fn find_memory_source_content(
        &self,
        source_id: &SourceId,
        content_hash: &str,
    ) -> Result<Option<SourceContent>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT source_id, content
            FROM memory_source_contents
            WHERE source_id = $1 AND content_hash = $2
            ",
        )
        .bind(source_id.as_str())
        .bind(content_hash)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_source_content).transpose()
    }

    /// Loads the most recently stored version of a source's text.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails or the stored row
    /// cannot be reconstructed.
    pub async fn find_latest_memory_source_content(
        &self,
        source_id: &SourceId,
    ) -> Result<Option<SourceContent>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT source_id, content
            FROM memory_source_contents
            WHERE source_id = $1
            ORDER BY created_at DESC
            LIMIT 1
            ",
        )
        .bind(source_id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_source_content).transpose()
    }

    pub async fn list_memory_evidence(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Evidence>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, source_id, locator, excerpt, observed_at, span_start, span_end, source_content_hash
            FROM memory_evidence
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
        rows.iter().map(row_to_evidence).collect()
    }

    pub async fn find_memory_evidence(
        &self,
        id: &EvidenceId,
    ) -> Result<Option<Evidence>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, source_id, locator, excerpt, observed_at, span_start, span_end, source_content_hash
            FROM memory_evidence
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_evidence).transpose()
    }

    /// The same lookup, within one project: another project's record matches no row.
    pub async fn find_memory_source_in_project(
        &self,
        id: &SourceId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Source>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, kind, uri, title, authority
            FROM memory_sources
            WHERE id = $1 AND tenant_id = $2 AND project_id = $3
            ",
        )
        .bind(id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_source).transpose()
    }

    /// The same lookup, within one project: another project's record matches no row.
    pub async fn find_memory_evidence_in_project(
        &self,
        id: &EvidenceId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Evidence>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, source_id, locator, excerpt, observed_at, span_start, span_end, source_content_hash
            FROM memory_evidence
            WHERE id = $1 AND tenant_id = $2 AND project_id = $3
            ",
        )
        .bind(id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_evidence).transpose()
    }
}

pub(super) fn row_to_source(row: &sqlx::postgres::PgRow) -> Result<Source, PostgresStoreError> {
    Source::new(
        SourceId::new(row.try_get::<String, _>("id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        scope(row)?,
        row.try_get::<String, _>("kind")?,
        row.try_get::<Option<String>, _>("uri")?,
        row.try_get::<String, _>("title")?,
        parse_authority(&row.try_get::<String, _>("authority")?)?,
    )
    .map_err(PostgresStoreError::Memory)
}

pub(super) fn row_to_evidence(row: &sqlx::postgres::PgRow) -> Result<Evidence, PostgresStoreError> {
    let evidence = Evidence::new(
        EvidenceId::new(row.try_get::<String, _>("id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        scope(row)?,
        SourceId::new(row.try_get::<String, _>("source_id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        row.try_get::<String, _>("locator")?,
        row.try_get::<String, _>("excerpt")?,
        row.try_get::<String, _>("observed_at")?,
    )
    .map_err(PostgresStoreError::Memory)?;
    Ok(evidence.with_optional_span(row_to_evidence_span(row)?))
}

/// Reads the optional span columns. A check constraint keeps the three columns
/// all-present or all-absent, so a partial span means the row was written around
/// the schema and is treated as no span rather than a guess.
pub(super) fn row_to_evidence_span(
    row: &sqlx::postgres::PgRow,
) -> Result<Option<EvidenceSpan>, PostgresStoreError> {
    let (Some(start), Some(end), Some(hash)) = (
        row.try_get::<Option<i64>, _>("span_start")?,
        row.try_get::<Option<i64>, _>("span_end")?,
        row.try_get::<Option<String>, _>("source_content_hash")?,
    ) else {
        return Ok(None);
    };
    let start = usize::try_from(start).map_err(|_| {
        PostgresStoreError::InvalidPersistedValue(format!("evidence span start {start} is invalid"))
    })?;
    let end = usize::try_from(end).map_err(|_| {
        PostgresStoreError::InvalidPersistedValue(format!("evidence span end {end} is invalid"))
    })?;
    EvidenceSpan::new(start, end, hash)
        .map(Some)
        .map_err(PostgresStoreError::Memory)
}

pub(super) fn row_to_source_content(
    row: &sqlx::postgres::PgRow,
) -> Result<SourceContent, PostgresStoreError> {
    SourceContent::new(
        SourceId::new(row.try_get::<String, _>("source_id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        row.try_get::<String, _>("content")?,
    )
    .map_err(PostgresStoreError::Memory)
}
