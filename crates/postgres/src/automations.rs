use capsulet_core::{
    Automation, AutomationId, AutomationStatus, AutomationTrigger, CustomTriggerPlugin,
};

use crate::{
    PostgresStore, PostgresStoreError,
    rows::{row_to_automation, row_to_automation_trigger, row_to_custom_trigger_plugin},
};
impl PostgresStore {
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn upsert_automation(
        &self,
        automation: &Automation,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO automations (
                id, name, description, workflow_id, job_input, status, next_fire_at, updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5::jsonb, $6, NULL,
                now()
            )
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                workflow_id = EXCLUDED.workflow_id,
                job_input = EXCLUDED.job_input,
                status = EXCLUDED.status,
                updated_at = now()
            ",
        )
        .bind(automation.id().as_str())
        .bind(automation.name())
        .bind(automation.description())
        .bind(automation.workflow_id().as_str())
        .bind(automation.job_input_json())
        .bind(automation.status().to_string())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Saves an automation, its triggers and its owner, all or nothing.
    ///
    /// The workflow it runs and every plugin its triggers use must belong to the same
    /// project, and stay locked until the write commits. The header write is the
    /// ownership check: an id another project holds matches no row, and nothing else
    /// is touched.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError::ScopeMismatch`] when the id, the workflow or a
    /// plugin belongs to another project, and [`PostgresStoreError`] when persistence
    /// fails.
    pub async fn save_automation_in_project(
        &self,
        automation: &Automation,
        triggers: &[AutomationTrigger],
        condition_json: &str,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), PostgresStoreError> {
        let mut tx = self.pool.begin().await?;

        let workflow_owned = sqlx::query_scalar::<_, i32>(
            r"
            SELECT 1 FROM workflow_definitions
            WHERE id = $1 AND tenant_id = $2 AND project_id = $3
            FOR SHARE
            ",
        )
        .bind(automation.workflow_id().as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&mut *tx)
        .await?;
        if workflow_owned.is_none() {
            return Err(PostgresStoreError::ScopeMismatch);
        }

        for plugin_id in triggers.iter().filter_map(AutomationTrigger::plugin_id) {
            let plugin_owned = sqlx::query_scalar::<_, i32>(
                r"
                SELECT 1 FROM custom_trigger_plugins
                WHERE id = $1 AND tenant_id = $2 AND project_id = $3
                FOR SHARE
                ",
            )
            .bind(plugin_id)
            .bind(tenant_id)
            .bind(project_id)
            .fetch_optional(&mut *tx)
            .await?;
            if plugin_owned.is_none() {
                return Err(PostgresStoreError::ScopeMismatch);
            }
        }

        let written = sqlx::query(
            r"
            INSERT INTO automations (
                id, name, description, workflow_id, job_input, status, next_fire_at,
                tenant_id, project_id, updated_at
            )
            VALUES ($1, $2, $3, $4, $5::jsonb, $6, NULL, $7, $8, now())
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                workflow_id = EXCLUDED.workflow_id,
                job_input = EXCLUDED.job_input,
                status = EXCLUDED.status,
                updated_at = now()
            WHERE automations.tenant_id = EXCLUDED.tenant_id
              AND automations.project_id = EXCLUDED.project_id
            ",
        )
        .bind(automation.id().as_str())
        .bind(automation.name())
        .bind(automation.description())
        .bind(automation.workflow_id().as_str())
        .bind(automation.job_input_json())
        .bind(automation.status().to_string())
        .bind(tenant_id)
        .bind(project_id)
        .execute(&mut *tx)
        .await?;
        if written.rows_affected() == 0 {
            return Err(PostgresStoreError::ScopeMismatch);
        }

        write_automation_triggers(&mut tx, automation.id(), triggers, condition_json).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Lists a project's automations, scoping before the limit.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn list_automations_in_project(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Automation>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, name, description, workflow_id, job_input::text AS job_input, status
            FROM automations
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
        rows.iter().map(row_to_automation).collect()
    }

    /// Finds one automation inside a project.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn find_automation_in_project(
        &self,
        id: &AutomationId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Automation>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, name, description, workflow_id, job_input::text AS job_input, status
            FROM automations
            WHERE id = $1 AND tenant_id = $2 AND project_id = $3
            ",
        )
        .bind(id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_automation).transpose()
    }

    /// Saves a custom trigger plugin owned by one project.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError::ScopeMismatch`] when another project holds this
    /// id, and [`PostgresStoreError`] when persistence fails.
    pub async fn upsert_custom_trigger_plugin_in_project(
        &self,
        plugin: &CustomTriggerPlugin,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), PostgresStoreError> {
        let written = sqlx::query(
            r"
            INSERT INTO custom_trigger_plugins (
                id, name, description, runtime_image, command, config_schema,
                tenant_id, project_id, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6::jsonb, $7, $8, now())
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                runtime_image = EXCLUDED.runtime_image,
                command = EXCLUDED.command,
                config_schema = EXCLUDED.config_schema,
                updated_at = now()
            WHERE custom_trigger_plugins.tenant_id = EXCLUDED.tenant_id
              AND custom_trigger_plugins.project_id = EXCLUDED.project_id
            ",
        )
        .bind(plugin.id())
        .bind(plugin.name())
        .bind(plugin.description())
        .bind(plugin.runtime_image())
        .bind(plugin.command())
        .bind(plugin.config_schema_json())
        .bind(tenant_id)
        .bind(project_id)
        .execute(&self.pool)
        .await?;
        if written.rows_affected() == 0 {
            return Err(PostgresStoreError::ScopeMismatch);
        }
        Ok(())
    }

    /// Lists a project's custom trigger plugins, scoping before the limit.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails.
    pub async fn list_custom_trigger_plugins_in_project(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<CustomTriggerPlugin>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, name, description, runtime_image, command, config_schema::text
            FROM custom_trigger_plugins
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
        rows.iter().map(row_to_custom_trigger_plugin).collect()
    }

    /// Finds a custom trigger plugin inside a project.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails.
    pub async fn find_custom_trigger_plugin_in_project(
        &self,
        id: &str,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<CustomTriggerPlugin>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, name, description, runtime_image, command, config_schema::text
            FROM custom_trigger_plugins
            WHERE id = $1 AND tenant_id = $2 AND project_id = $3
            ",
        )
        .bind(id)
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_custom_trigger_plugin).transpose()
    }

    /// Lists automations.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn list_automations(
        &self,
        limit: i64,
    ) -> Result<Vec<Automation>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, name, description, workflow_id, job_input::text AS job_input, status
            FROM automations
            ORDER BY updated_at DESC, id ASC
            LIMIT $1
            ",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(row_to_automation).collect()
    }

    /// Finds one automation.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn find_automation(
        &self,
        id: &AutomationId,
    ) -> Result<Option<Automation>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, name, description, workflow_id, job_input::text AS job_input, status
            FROM automations
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(row_to_automation).transpose()
    }

    /// Updates automation enabled/disabled status.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn set_automation_status(
        &self,
        id: &AutomationId,
        status: AutomationStatus,
    ) -> Result<Option<Automation>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            UPDATE automations
            SET status = $2,
                next_fire_at = CASE
                    WHEN $2 = 'enabled'
                        AND EXISTS (
                            SELECT 1
                            FROM automation_triggers
                            WHERE automation_id = automations.id
                              AND kind = 'schedule'
                              AND enabled
                              AND (config->>'interval_seconds') ~ '^[0-9]+$'
                              AND (config->>'interval_seconds')::integer > 0
                        )
                        THEN COALESCE(next_fire_at, now())
                    ELSE NULL
                END,
                updated_at = now()
            WHERE id = $1
            RETURNING id, name, description, workflow_id, job_input::text AS job_input,
                      status
            ",
        )
        .bind(id.as_str())
        .bind(status.to_string())
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(row_to_automation).transpose()
    }

    /// Deletes an automation and its trigger graph.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when deletion fails.
    pub async fn delete_automation(&self, id: &AutomationId) -> Result<bool, PostgresStoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE workflow_runs SET automation_id = NULL WHERE automation_id = $1")
            .bind(id.as_str())
            .execute(&mut *tx)
            .await?;

        let result = sqlx::query("DELETE FROM automations WHERE id = $1")
            .bind(id.as_str())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;

        Ok(result.rows_affected() > 0)
    }

    /// Replaces an automation trigger graph and its condition tree.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn replace_automation_triggers(
        &self,
        automation_id: &AutomationId,
        triggers: &[AutomationTrigger],
        condition_json: &str,
    ) -> Result<(), PostgresStoreError> {
        let mut tx = self.pool.begin().await?;
        write_automation_triggers(&mut tx, automation_id, triggers, condition_json).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Lists trigger definitions and the stored condition tree for one automation.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn list_automation_triggers(
        &self,
        automation_id: &AutomationId,
    ) -> Result<(Vec<AutomationTrigger>, String), PostgresStoreError> {
        let condition_json: Option<String> =
            sqlx::query_scalar("SELECT condition_tree::text FROM automations WHERE id = $1")
                .bind(automation_id.as_str())
                .fetch_optional(&self.pool)
                .await?;
        let rows = sqlx::query(
            r"
            SELECT automation_id, name, kind, config::text, plugin_id, enabled
            FROM automation_triggers
            WHERE automation_id = $1
            ORDER BY name ASC
            ",
        )
        .bind(automation_id.as_str())
        .fetch_all(&self.pool)
        .await?;

        Ok((
            rows.iter()
                .map(row_to_automation_trigger)
                .collect::<Result<Vec<_>, _>>()?,
            condition_json.unwrap_or_else(|| "{}".to_string()),
        ))
    }

    /// Inserts or updates a custom trigger plugin registry entry.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn upsert_custom_trigger_plugin(
        &self,
        plugin: &CustomTriggerPlugin,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO custom_trigger_plugins (
                id, name, description, runtime_image, command, config_schema, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6::jsonb, now())
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                runtime_image = EXCLUDED.runtime_image,
                command = EXCLUDED.command,
                config_schema = EXCLUDED.config_schema,
                updated_at = now()
            ",
        )
        .bind(plugin.id())
        .bind(plugin.name())
        .bind(plugin.description())
        .bind(plugin.runtime_image())
        .bind(plugin.command())
        .bind(plugin.config_schema_json())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Lists custom trigger plugins.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails.
    pub async fn list_custom_trigger_plugins(
        &self,
        limit: i64,
    ) -> Result<Vec<CustomTriggerPlugin>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, name, description, runtime_image, command, config_schema::text
            FROM custom_trigger_plugins
            ORDER BY updated_at DESC, id ASC
            LIMIT $1
            ",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(row_to_custom_trigger_plugin).collect()
    }

    /// Finds a custom trigger plugin by id.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails.
    pub async fn find_custom_trigger_plugin(
        &self,
        id: &str,
    ) -> Result<Option<CustomTriggerPlugin>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, name, description, runtime_image, command, config_schema::text
            FROM custom_trigger_plugins
            WHERE id = $1
            ",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(row_to_custom_trigger_plugin).transpose()
    }
}

/// Replaces an automation's triggers and condition inside a caller's transaction.
async fn write_automation_triggers(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    automation_id: &AutomationId,
    triggers: &[AutomationTrigger],
    condition_json: &str,
) -> Result<(), PostgresStoreError> {
    sqlx::query("DELETE FROM automation_triggers WHERE automation_id = $1")
        .bind(automation_id.as_str())
        .execute(&mut **tx)
        .await?;

    for trigger in triggers {
        sqlx::query(
            r"
            INSERT INTO automation_triggers (
                id, automation_id, name, kind, config, plugin_id, enabled, updated_at
            )
            VALUES ($1, $2, $3, $4, $5::jsonb, $6, $7, now())
            ",
        )
        .bind(format!(
            "{}_{}",
            automation_id.as_str(),
            trigger.name().as_str()
        ))
        .bind(automation_id.as_str())
        .bind(trigger.name().as_str())
        .bind(trigger.kind().to_string())
        .bind(trigger.config_json())
        .bind(trigger.plugin_id())
        .bind(trigger.enabled())
        .execute(&mut **tx)
        .await?;
    }

    sqlx::query(
        r"
        UPDATE automations
        SET condition_tree = $2::jsonb,
            next_fire_at = CASE
                WHEN status = 'enabled'
                    AND EXISTS (
                        SELECT 1
                        FROM automation_triggers
                        WHERE automation_id = automations.id
                          AND kind = 'schedule'
                          AND enabled
                          AND (config->>'interval_seconds') ~ '^[0-9]+$'
                          AND (config->>'interval_seconds')::integer > 0
                    )
                    THEN COALESCE(next_fire_at, now())
                ELSE NULL
            END,
            updated_at = now()
        WHERE id = $1
        ",
    )
    .bind(automation_id.as_str())
    .bind(condition_json)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
