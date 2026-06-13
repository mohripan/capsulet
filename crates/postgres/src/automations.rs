use capsulet_core::{Automation, AutomationId, AutomationTrigger, CustomTriggerPlugin};

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
                id, name, description, workflow_id, job_input, status, trigger_kind,
                interval_seconds, next_fire_at, updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5::jsonb, $6, $7, $8,
                CASE WHEN $7 = 'interval' THEN now() ELSE NULL END,
                now()
            )
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                workflow_id = EXCLUDED.workflow_id,
                job_input = EXCLUDED.job_input,
                status = EXCLUDED.status,
                trigger_kind = EXCLUDED.trigger_kind,
                interval_seconds = EXCLUDED.interval_seconds,
                next_fire_at = COALESCE(automations.next_fire_at, EXCLUDED.next_fire_at),
                updated_at = now()
            ",
        )
        .bind(automation.id.as_str())
        .bind(&automation.name)
        .bind(&automation.description)
        .bind(automation.workflow_id.as_str())
        .bind(&automation.job_input_json)
        .bind(automation.status.to_string())
        .bind(automation.trigger_kind.to_string())
        .bind(automation.interval_seconds)
        .execute(&self.pool)
        .await?;

        Ok(())
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
            SELECT id, name, description, workflow_id, job_input::text AS job_input, status, trigger_kind, interval_seconds
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
            SELECT id, name, description, workflow_id, job_input::text AS job_input, status, trigger_kind, interval_seconds
            FROM automations
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(row_to_automation).transpose()
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
        sqlx::query(
            r"
            UPDATE automations
            SET condition_tree = $2::jsonb,
                updated_at = now()
            WHERE id = $1
            ",
        )
        .bind(automation_id.as_str())
        .bind(condition_json)
        .execute(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM automation_triggers WHERE automation_id = $1")
            .bind(automation_id.as_str())
            .execute(&mut *tx)
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
                trigger.name.as_str()
            ))
            .bind(automation_id.as_str())
            .bind(trigger.name.as_str())
            .bind(trigger.kind.to_string())
            .bind(&trigger.config_json)
            .bind(trigger.plugin_id.as_deref())
            .bind(trigger.enabled)
            .execute(&mut *tx)
            .await?;
        }

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
        .bind(&plugin.id)
        .bind(&plugin.name)
        .bind(&plugin.description)
        .bind(&plugin.runtime_image)
        .bind(&plugin.command)
        .bind(&plugin.config_schema_json)
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
