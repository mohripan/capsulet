use std::collections::HashMap;

use sqlx::Row;

use crate::{PostgresStore, PostgresStoreError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionCandidate {
    pub job_run_id: String,
    pub object_keys: Vec<String>,
}

impl PostgresStore {
    /// Lists terminal runs whose retained objects are ready for cleanup.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query or row decoding fails.
    pub async fn list_retention_candidates(
        &self,
        retention_days: i32,
        limit: i64,
    ) -> Result<Vec<RetentionCandidate>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            WITH candidates AS (
                SELECT run.id, run.updated_at
                FROM job_runs run
                LEFT JOIN retention_cleanups cleanup ON cleanup.job_run_id = run.id
                WHERE run.status IN ('succeeded', 'failed', 'cancelled', 'timed_out')
                  AND run.updated_at < now() - make_interval(days => $1)
                  AND cleanup.job_run_id IS NULL
                ORDER BY run.updated_at, run.id
                LIMIT $2
            )
            SELECT candidate.id AS job_run_id, artifact.object_key
            FROM candidates candidate
            LEFT JOIN job_artifacts artifact ON artifact.job_run_id = candidate.id
            ORDER BY candidate.updated_at, candidate.id, artifact.id
            ",
        )
        .bind(retention_days)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let mut candidates: Vec<RetentionCandidate> = Vec::new();
        let mut positions = HashMap::new();
        for row in rows {
            let run_id: String = row.try_get("job_run_id")?;
            let position = *positions.entry(run_id.clone()).or_insert_with(|| {
                candidates.push(RetentionCandidate {
                    job_run_id: run_id,
                    object_keys: Vec::new(),
                });
                candidates.len() - 1
            });
            if let Some(key) = row.try_get::<Option<String>, _>("object_key")? {
                candidates[position].object_keys.push(key);
            }
        }
        Ok(candidates)
    }

    /// Removes retained metadata and records an idempotent cleanup marker.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the transaction fails.
    pub async fn complete_retention_cleanup(
        &self,
        job_run_id: &str,
    ) -> Result<(), PostgresStoreError> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query("DELETE FROM job_artifacts WHERE job_run_id = $1")
            .bind(job_run_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM job_run_logs WHERE job_run_id = $1")
            .bind(job_run_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "INSERT INTO retention_cleanups (job_run_id) VALUES ($1) ON CONFLICT DO NOTHING",
        )
        .bind(job_run_id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Deletes audit events older than the configured policy.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the delete fails.
    pub async fn cleanup_old_audit_events(
        &self,
        retention_days: i32,
    ) -> Result<u64, PostgresStoreError> {
        let result = sqlx::query(
            "DELETE FROM audit_events WHERE created_at < now() - make_interval(days => $1)",
        )
        .bind(retention_days)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Deletes terminal trigger events older than the configured policy.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the delete fails.
    pub async fn cleanup_old_trigger_events(
        &self,
        retention_days: i32,
    ) -> Result<u64, PostgresStoreError> {
        let result = sqlx::query(
            "DELETE FROM trigger_events WHERE status IN ('evaluated', 'failed') AND updated_at < now() - make_interval(days => $1)",
        )
        .bind(retention_days)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}
