use sqlx::Row;

use crate::{PostgresStore, PostgresStoreError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRecord {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
}

impl PostgresStore {
    /// Lists projects visible through explicit project ids.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails.
    pub async fn list_projects(
        &self,
        tenant_id: &str,
        project_ids: &[String],
    ) -> Result<Vec<ProjectRecord>, PostgresStoreError> {
        if project_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            r"
            SELECT id, tenant_id, name
            FROM projects
            WHERE tenant_id = $1 AND id = ANY($2)
            ORDER BY name ASC, id ASC
            ",
        )
        .bind(tenant_id)
        .bind(project_ids)
        .fetch_all(&self.pool)
        .await?;

        rows.iter()
            .map(|row| {
                Ok(ProjectRecord {
                    id: row.try_get("id")?,
                    tenant_id: row.try_get("tenant_id")?,
                    name: row.try_get("name")?,
                })
            })
            .collect()
    }
}
