//! Automations, their triggers, and custom trigger plugins.

use super::super::*;

#[async_trait::async_trait]
impl AutomationStore for FakeStore {
    async fn enqueue_trigger_event(
        &self,
        _event: &TriggerEvent,
        _idempotency_key: &str,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn save_automation(
        &self,
        automation: &Automation,
        triggers: &[AutomationTrigger],
        condition_json: &str,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error> {
        if !self.owns(
            "workflows",
            automation.workflow_id().as_str(),
            tenant_id,
            project_id,
        ) {
            return Err("scope mismatch".to_string());
        }
        for plugin_id in triggers.iter().filter_map(AutomationTrigger::plugin_id) {
            if !self.owns("trigger_plugins", plugin_id, tenant_id, project_id) {
                return Err("scope mismatch".to_string());
            }
        }
        self.claim_resource(
            "automations",
            automation.id().as_str(),
            tenant_id,
            project_id,
        )?;
        {
            let mut automations = self.automations.lock().map_err(|error| error.to_string())?;
            automations.retain(|existing| existing.id() != automation.id());
            automations.push(automation.clone());
        }
        self.store_automation_triggers(automation.id(), triggers, condition_json)
    }

    async fn list_automations(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Automation>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        let automations = self
            .automations
            .lock()
            .map_err(|error| error.to_string())?
            .clone();
        Ok(automations
            .into_iter()
            .filter(|automation| {
                self.owns(
                    "automations",
                    automation.id().as_str(),
                    tenant_id,
                    project_id,
                )
            })
            .take(limit)
            .collect())
    }

    async fn find_automation(
        &self,
        id: &AutomationId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Automation>, Self::Error> {
        if !self.owns("automations", id.as_str(), tenant_id, project_id) {
            return Ok(None);
        }
        Ok(self
            .automations
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|automation| automation.id().clone() == *id)
            .cloned())
    }

    async fn set_automation_status(
        &self,
        id: &AutomationId,
        status: AutomationStatus,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Automation>, Self::Error> {
        if !self.owned_by("automations", id.as_str(), tenant_id, project_id)? {
            return Ok(None);
        }
        let mut automations = self.automations.lock().map_err(|error| error.to_string())?;
        let Some(automation) = automations
            .iter_mut()
            .find(|automation| automation.id().clone() == *id)
        else {
            return Ok(None);
        };
        *automation = automation.clone().with_status(status);
        Ok(Some(automation.clone()))
    }

    async fn delete_automation(
        &self,
        id: &AutomationId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<bool, Self::Error> {
        if !self.owned_by("automations", id.as_str(), tenant_id, project_id)? {
            return Ok(false);
        }
        let mut automations = self.automations.lock().map_err(|error| error.to_string())?;
        let initial_len = automations.len();
        automations.retain(|automation| automation.id().clone() != *id);
        let deleted = automations.len() != initial_len;
        drop(automations);

        if deleted {
            self.automation_triggers
                .lock()
                .map_err(|error| error.to_string())?
                .retain(|trigger| trigger.automation_id() != id);
            self.automation_conditions
                .lock()
                .map_err(|error| error.to_string())?
                .retain(|(stored_id, _)| stored_id != id.as_str());
        }
        Ok(deleted)
    }

    async fn list_automation_triggers(
        &self,
        automation_id: &AutomationId,
    ) -> Result<(Vec<AutomationTrigger>, String), Self::Error> {
        let triggers = self
            .automation_triggers
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|trigger| trigger.automation_id() == automation_id)
            .cloned()
            .collect();
        let condition = self
            .automation_conditions
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|(id, _)| id == automation_id.as_str())
            .map_or_else(|| "{}".to_string(), |(_, condition)| condition.clone());
        Ok((triggers, condition))
    }

    async fn upsert_custom_trigger_plugin(
        &self,
        plugin: &CustomTriggerPlugin,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error> {
        self.claim_resource("trigger_plugins", plugin.id(), tenant_id, project_id)?;
        let mut plugins = self
            .trigger_plugins
            .lock()
            .map_err(|error| error.to_string())?;
        plugins.retain(|existing| existing.id() != plugin.id());
        plugins.push(plugin.clone());
        Ok(())
    }

    async fn list_custom_trigger_plugins(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<CustomTriggerPlugin>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        let plugins = self
            .trigger_plugins
            .lock()
            .map_err(|error| error.to_string())?
            .clone();
        Ok(plugins
            .into_iter()
            .filter(|plugin| self.owns("trigger_plugins", plugin.id(), tenant_id, project_id))
            .take(limit)
            .collect())
    }

    async fn find_custom_trigger_plugin(
        &self,
        id: &str,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<CustomTriggerPlugin>, Self::Error> {
        if !self.owns("trigger_plugins", id, tenant_id, project_id) {
            return Ok(None);
        }
        Ok(self
            .trigger_plugins
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|plugin| plugin.id() == id)
            .cloned())
    }
}
