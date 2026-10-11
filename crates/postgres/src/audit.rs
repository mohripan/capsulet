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

/// Verified project context associated with an audit event.
#[derive(Debug, Clone, Copy)]
pub struct AuditScope<'a> {
    pub tenant_id: &'a str,
    pub project_id: &'a str,
}

/// Observed request metadata. Unauthenticated/unresolved scopes remain unverified.
#[derive(Debug, Clone, Copy)]
pub struct NewAuditEvent<'a> {
    pub principal: &'a str,
    pub role: &'a str,
    pub method: &'a str,
    pub path: &'a str,
    pub status_code: u16,
    pub request_id: Option<&'a str>,
    pub user_agent: Option<&'a str>,
    pub scope: Option<AuditScope<'a>>,
}

impl PostgresStore {
    /// Records an audit event with its verified context.
    ///
    /// # Errors
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn record_scoped_audit_event(
        &self,
        event: &NewAuditEvent<'_>,
    ) -> Result<(), PostgresStoreError> {
        let scope = event.scope.unwrap_or(AuditScope {
            tenant_id: "default",
            project_id: "default",
        });
        sqlx::query(
            r"INSERT INTO audit_events
               (principal, role, method, path, status_code, request_id, user_agent, tenant_id, project_id, scope_verified)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(event.principal).bind(event.role).bind(event.method).bind(event.path)
        .bind(i32::from(event.status_code)).bind(event.request_id).bind(event.user_agent)
        .bind(scope.tenant_id).bind(scope.project_id).bind(event.scope.is_some())
        .execute(&self.pool).await?;
        Ok(())
    }

    /// Lists audit events owned by one selected project.
    ///
    /// # Errors
    /// Returns [`PostgresStoreError`] when lookup fails.
    pub async fn list_audit_events_in_project(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<AuditEvent>, PostgresStoreError> {
        let rows = sqlx::query(
            r"SELECT id, principal, role, method, path, status_code, request_id, created_at::text AS created_at
               FROM audit_events WHERE tenant_id = $1 AND project_id = $2 AND scope_verified
               ORDER BY created_at DESC, id DESC LIMIT $3",
        ).bind(tenant_id).bind(project_id).bind(limit.clamp(1, 500)).fetch_all(&self.pool).await?;
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
        self.record_scoped_audit_event(&NewAuditEvent {
            principal,
            role,
            method,
            path,
            status_code,
            request_id,
            user_agent,
            scope: None,
        })
        .await
    }
}
