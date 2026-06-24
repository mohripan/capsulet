use sqlx::Row;

use crate::{PostgresStore, PostgresStoreError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRecord {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMembershipRecord {
    pub id: String,
    pub tenant_id: String,
    pub project_id: String,
    pub principal_kind: String,
    pub principal_name: String,
    pub role: String,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewProjectMembership {
    pub id: String,
    pub tenant_id: String,
    pub project_id: String,
    pub principal_kind: String,
    pub principal_name: String,
    pub role: String,
    pub created_by: String,
}

impl PostgresStore {
    /// Lists all projects in one tenant.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails.
    pub async fn list_all_projects(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<ProjectRecord>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, tenant_id, name
            FROM projects
            WHERE tenant_id = $1
            ORDER BY name ASC, id ASC
            ",
        )
        .bind(tenant_id)
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

    /// Lists project memberships for one project.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails.
    pub async fn list_project_memberships(
        &self,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<ProjectMembershipRecord>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, principal_kind, principal_name, role,
                   created_by, created_at::text AS created_at, updated_at::text AS updated_at
            FROM project_memberships
            WHERE tenant_id = $1 AND project_id = $2
            ORDER BY principal_kind ASC, principal_name ASC
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(row_to_project_membership).collect()
    }

    /// Lists memberships assigned to one principal.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails.
    pub async fn list_principal_project_memberships(
        &self,
        tenant_id: &str,
        principal_name: &str,
    ) -> Result<Vec<ProjectMembershipRecord>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, principal_kind, principal_name, role,
                   created_by, created_at::text AS created_at, updated_at::text AS updated_at
            FROM project_memberships
            WHERE tenant_id = $1
              AND principal_name = $2
              AND principal_kind IN ('user', 'service_account')
            ORDER BY project_id ASC, principal_kind ASC
            ",
        )
        .bind(tenant_id)
        .bind(principal_name)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(row_to_project_membership).collect()
    }

    /// Inserts or updates one project membership.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn upsert_project_membership(
        &self,
        membership: &NewProjectMembership,
    ) -> Result<ProjectMembershipRecord, PostgresStoreError> {
        let row = sqlx::query(
            r"
            INSERT INTO project_memberships (
                id, tenant_id, project_id, principal_kind, principal_name, role, created_by, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, now())
            ON CONFLICT (tenant_id, project_id, principal_kind, principal_name) DO UPDATE SET
                role = EXCLUDED.role,
                updated_at = now()
            RETURNING id, tenant_id, project_id, principal_kind, principal_name, role,
                      created_by, created_at::text AS created_at, updated_at::text AS updated_at
            ",
        )
        .bind(&membership.id)
        .bind(&membership.tenant_id)
        .bind(&membership.project_id)
        .bind(&membership.principal_kind)
        .bind(&membership.principal_name)
        .bind(&membership.role)
        .bind(&membership.created_by)
        .fetch_one(&self.pool)
        .await?;

        row_to_project_membership(&row)
    }

    /// Deletes one project membership.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when deletion fails.
    pub async fn delete_project_membership(
        &self,
        tenant_id: &str,
        project_id: &str,
        principal_kind: &str,
        principal_name: &str,
    ) -> Result<bool, PostgresStoreError> {
        let result = sqlx::query(
            r"
            DELETE FROM project_memberships
            WHERE tenant_id = $1 AND project_id = $2 AND principal_kind = $3 AND principal_name = $4
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(principal_kind)
        .bind(principal_name)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Finds the tenant/project owner for a resource.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails.
    pub async fn resource_project(
        &self,
        resource: &str,
        id: &str,
    ) -> Result<Option<(String, String)>, PostgresStoreError> {
        let table = resource_table(resource).ok_or_else(|| {
            PostgresStoreError::InvalidPersistedValue(format!("unknown resource: {resource}"))
        })?;
        let row = sqlx::query(&format!(
            "SELECT tenant_id, project_id FROM {table} WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| Ok((row.try_get("tenant_id")?, row.try_get("project_id")?)))
            .transpose()
    }

    /// Assigns tenant/project ownership to a resource.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn set_resource_project(
        &self,
        resource: &str,
        id: &str,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), PostgresStoreError> {
        let table = resource_table(resource).ok_or_else(|| {
            PostgresStoreError::InvalidPersistedValue(format!("unknown resource: {resource}"))
        })?;
        sqlx::query(&format!(
            "UPDATE {table} SET tenant_id = $2, project_id = $3 WHERE id = $1"
        ))
        .bind(id)
        .bind(tenant_id)
        .bind(project_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn row_to_project_membership(
    row: &sqlx::postgres::PgRow,
) -> Result<ProjectMembershipRecord, PostgresStoreError> {
    Ok(ProjectMembershipRecord {
        id: row.try_get("id")?,
        tenant_id: row.try_get("tenant_id")?,
        project_id: row.try_get("project_id")?,
        principal_kind: row.try_get("principal_kind")?,
        principal_name: row.try_get("principal_name")?,
        role: row.try_get("role")?,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn resource_table(resource: &str) -> Option<&'static str> {
    match resource {
        "job_definitions" => Some("job_definitions"),
        "workflows" => Some("workflow_definitions"),
        "automations" => Some("automations"),
        "trigger_plugins" => Some("custom_trigger_plugins"),
        "job_runs" => Some("job_runs"),
        "workflow_runs" => Some("workflow_runs"),
        _ => None,
    }
}
