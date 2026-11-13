//! Automations, their triggers, and custom trigger plugins.

use async_trait::async_trait;
use capsulet_core::{Automation, AutomationId, AutomationTrigger, CustomTriggerPlugin};
use capsulet_postgres::{PostgresStore, TriggerEvent};

use super::StoreBase;

/// Automations, their triggers, and custom trigger plugins.
#[async_trait]
pub trait AutomationStore: StoreBase {
    async fn enqueue_trigger_event(
        &self,
        event: &TriggerEvent,
        idempotency_key: &str,
    ) -> Result<bool, Self::Error>;

    /// Saves an automation, its triggers and its owner, all or nothing.
    async fn save_automation(
        &self,
        automation: &Automation,
        triggers: &[AutomationTrigger],
        condition_json: &str,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error>;

    async fn list_automations(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Automation>, Self::Error>;

    async fn find_automation(
        &self,
        id: &AutomationId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Automation>, Self::Error>;

    async fn set_automation_status(
        &self,
        id: &AutomationId,
        status: capsulet_core::AutomationStatus,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Automation>, Self::Error>;

    async fn delete_automation(
        &self,
        id: &AutomationId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<bool, Self::Error>;

    async fn list_automation_triggers(
        &self,
        automation_id: &AutomationId,
    ) -> Result<(Vec<AutomationTrigger>, String), Self::Error>;

    async fn upsert_custom_trigger_plugin(
        &self,
        plugin: &CustomTriggerPlugin,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error>;

    async fn list_custom_trigger_plugins(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<CustomTriggerPlugin>, Self::Error>;

    async fn find_custom_trigger_plugin(
        &self,
        id: &str,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<CustomTriggerPlugin>, Self::Error>;
}

#[async_trait]
impl AutomationStore for PostgresStore {
    async fn enqueue_trigger_event(
        &self,
        event: &TriggerEvent,
        idempotency_key: &str,
    ) -> Result<bool, Self::Error> {
        self.enqueue_trigger_event(event, idempotency_key).await
    }

    async fn save_automation(
        &self,
        automation: &Automation,
        triggers: &[AutomationTrigger],
        condition_json: &str,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error> {
        self.save_automation_in_project(automation, triggers, condition_json, tenant_id, project_id)
            .await
    }

    async fn list_automations(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Automation>, Self::Error> {
        self.list_automations_in_project(tenant_id, project_id, limit)
            .await
    }

    async fn find_automation(
        &self,
        id: &AutomationId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Automation>, Self::Error> {
        self.find_automation_in_project(id, tenant_id, project_id)
            .await
    }

    async fn set_automation_status(
        &self,
        id: &AutomationId,
        status: capsulet_core::AutomationStatus,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Automation>, Self::Error> {
        PostgresStore::set_automation_status(self, id, status, tenant_id, project_id).await
    }

    async fn delete_automation(
        &self,
        id: &AutomationId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<bool, Self::Error> {
        PostgresStore::delete_automation(self, id, tenant_id, project_id).await
    }

    async fn list_automation_triggers(
        &self,
        automation_id: &AutomationId,
    ) -> Result<(Vec<AutomationTrigger>, String), Self::Error> {
        self.list_automation_triggers(automation_id).await
    }

    async fn upsert_custom_trigger_plugin(
        &self,
        plugin: &CustomTriggerPlugin,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error> {
        self.upsert_custom_trigger_plugin_in_project(plugin, tenant_id, project_id)
            .await
    }

    async fn list_custom_trigger_plugins(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<CustomTriggerPlugin>, Self::Error> {
        self.list_custom_trigger_plugins_in_project(tenant_id, project_id, limit)
            .await
    }

    async fn find_custom_trigger_plugin(
        &self,
        id: &str,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<CustomTriggerPlugin>, Self::Error> {
        self.find_custom_trigger_plugin_in_project(id, tenant_id, project_id)
            .await
    }
}
