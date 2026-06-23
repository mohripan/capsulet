use crate::{PostgresStore, PostgresStoreError};
use sqlx::Row;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceAccountRecord {
    pub id: String,
    pub name: String,
    pub tenant_id: String,
    pub project_id: String,
    pub role: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<String>,
    pub revoked_at: Option<String>,
    pub last_used_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct NewServiceAccount {
    pub id: String,
    pub name: String,
    pub tenant_id: String,
    pub project_id: String,
    pub role: String,
    pub scopes: Vec<String>,
    pub token_hash: [u8; 32],
    pub expires_at_unix: Option<i64>,
}

impl PostgresStore {
    /// Creates a database-backed service account.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the insert fails.
    pub async fn create_service_account(
        &self,
        account: &NewServiceAccount,
    ) -> Result<ServiceAccountRecord, PostgresStoreError> {
        let row = sqlx::query(
            r"
            INSERT INTO service_accounts (
                id, name, tenant_id, project_id, role, scopes, token_hash, expires_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7,
                CASE WHEN $8::bigint IS NULL THEN NULL ELSE to_timestamp($8::bigint) END
            )
            RETURNING id, name, tenant_id, project_id, role, scopes,
                      expires_at::text AS expires_at,
                      revoked_at::text AS revoked_at,
                      last_used_at::text AS last_used_at,
                      created_at::text AS created_at
            ",
        )
        .bind(&account.id)
        .bind(&account.name)
        .bind(&account.tenant_id)
        .bind(&account.project_id)
        .bind(&account.role)
        .bind(&account.scopes)
        .bind(account.token_hash.as_slice())
        .bind(account.expires_at_unix)
        .fetch_one(&self.pool)
        .await?;

        row_to_service_account(&row)
    }

    /// Lists service accounts for administrative review.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails.
    pub async fn list_service_accounts(
        &self,
        limit: i64,
    ) -> Result<Vec<ServiceAccountRecord>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, name, tenant_id, project_id, role, scopes,
                   expires_at::text AS expires_at,
                   revoked_at::text AS revoked_at,
                   last_used_at::text AS last_used_at,
                   created_at::text AS created_at
            FROM service_accounts
            ORDER BY created_at DESC, id ASC
            LIMIT $1
            ",
        )
        .bind(limit.clamp(1, 500))
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(row_to_service_account).collect()
    }

    /// Finds an active service account by token hash and updates last-used time.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails.
    pub async fn authenticate_service_account_hash(
        &self,
        token_hash: &[u8; 32],
    ) -> Result<Option<ServiceAccountRecord>, PostgresStoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            r"
            SELECT id, name, tenant_id, project_id, role, scopes,
                   expires_at::text AS expires_at,
                   revoked_at::text AS revoked_at,
                   last_used_at::text AS last_used_at,
                   created_at::text AS created_at
            FROM service_accounts
            WHERE token_hash = $1
              AND revoked_at IS NULL
              AND (expires_at IS NULL OR expires_at > now())
            FOR UPDATE
            ",
        )
        .bind(token_hash.as_slice())
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        let record = row_to_service_account(&row)?;
        sqlx::query(
            "UPDATE service_accounts SET last_used_at = now(), updated_at = now() WHERE id = $1",
        )
        .bind(&record.id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some(record))
    }

    /// Revokes a service account.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the update fails.
    pub async fn revoke_service_account(&self, id: &str) -> Result<bool, PostgresStoreError> {
        let result = sqlx::query(
            "UPDATE service_accounts SET revoked_at = COALESCE(revoked_at, now()), updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }
}

fn row_to_service_account(
    row: &sqlx::postgres::PgRow,
) -> Result<ServiceAccountRecord, PostgresStoreError> {
    Ok(ServiceAccountRecord {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        tenant_id: row.try_get("tenant_id")?,
        project_id: row.try_get("project_id")?,
        role: row.try_get("role")?,
        scopes: row.try_get("scopes")?,
        expires_at: row.try_get("expires_at")?,
        revoked_at: row.try_get("revoked_at")?,
        last_used_at: row.try_get("last_used_at")?,
        created_at: row.try_get("created_at")?,
    })
}
