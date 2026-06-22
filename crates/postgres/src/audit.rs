use crate::{PostgresStore, PostgresStoreError};
use sqlx::Row;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    pub id: i64,
    pub principal: String,
    pub role: String,
    pub method: String,
    pub path: String,
    pub status_code: i32,
    pub request_id: Option<String>,
    pub created_at: String,
}

impl PostgresStore {
    /// Lists the newest durable mutation audit records.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query or row decoding fails.
    pub async fn list_audit_events(
        &self,
        limit: i64,
    ) -> Result<Vec<AuditEvent>, PostgresStoreError> {
        let rows = sqlx::query(
            r"SELECT id, principal, role, method, path, status_code, request_id,
                      created_at::text AS created_at
               FROM audit_events ORDER BY created_at DESC, id DESC LIMIT $1",
        )
        .bind(limit.clamp(1, 500))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(AuditEvent {
                    id: row.try_get("id")?,
                    principal: row.try_get("principal")?,
                    role: row.try_get("role")?,
                    method: row.try_get("method")?,
                    path: row.try_get("path")?,
                    status_code: row.try_get("status_code")?,
                    request_id: row.try_get("request_id")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    /// Persists one authenticated mutation audit record.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the insert fails.
    pub async fn record_audit_event(
        &self,
        principal: &str,
        role: &str,
        method: &str,
        path: &str,
        status_code: u16,
        request_id: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"INSERT INTO audit_events
               (principal, role, method, path, status_code, request_id, user_agent)
               VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(principal)
        .bind(role)
        .bind(method)
        .bind(path)
        .bind(i32::from(status_code))
        .bind(request_id)
        .bind(user_agent)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
