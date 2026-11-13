//! Automations, their triggers, and custom trigger plugins.

use capsulet_core::{Automation, AutomationTrigger, CustomTriggerPlugin};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;

use crate::{error::ApiError, http::json_from_string};

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateAutomationRequest {
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub workflow_id: String,
    pub status: Option<String>,
    #[schema(value_type = Object)]
    pub job_input: Option<Value>,
    pub triggers: Option<Vec<CreateAutomationTriggerRequest>>,
    #[schema(value_type = Object)]
    pub condition: Option<Value>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateAutomationTriggerRequest {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub config: Value,
    pub plugin_id: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTriggerPluginRequest {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub runtime_image: String,
    pub command: Option<Vec<String>>,
    pub python_script: Option<String>,
    #[schema(value_type = Object)]
    pub config_schema: Option<Value>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ListAutomationsResponse {
    pub(crate) automations: Vec<AutomationResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ListAutomationTriggersResponse {
    pub(crate) triggers: Vec<TriggerResponse>,
    #[schema(value_type = Object)]
    pub(crate) condition: Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ListTriggerPluginsResponse {
    pub(crate) trigger_plugins: Vec<TriggerPluginResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct AutomationResponse {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) workflow_id: String,
    pub(crate) status: String,
    pub(crate) triggers: Vec<TriggerResponse>,
    #[schema(value_type = Object)]
    pub(crate) condition: Value,
    #[schema(value_type = Object)]
    pub(crate) job_input: Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct TriggerResponse {
    pub(crate) name: String,
    pub(crate) kind: String,
    #[schema(value_type = Object)]
    pub(crate) config: Value,
    pub(crate) plugin_id: Option<String>,
    pub(crate) enabled: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct TriggerPluginResponse {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) runtime_image: String,
    pub(crate) command: Vec<String>,
    pub(crate) python_script: String,
    #[schema(value_type = Object)]
    pub(crate) config_schema: Value,
}

impl AutomationResponse {
    pub(crate) fn new(
        automation: &Automation,
        triggers: &[AutomationTrigger],
        condition_json: &str,
    ) -> Result<Self, ApiError> {
        Ok(Self {
            id: automation.id().as_str().to_string(),
            name: automation.name().to_string(),
            description: automation.description().to_string(),
            workflow_id: automation.workflow_id().as_str().to_string(),
            status: automation.status().to_string(),
            triggers: triggers.iter().map(TriggerResponse::from).collect(),
            condition: json_from_string(condition_json)?,
            job_input: json_from_string(automation.job_input_json()).unwrap_or_else(|_| json!({})),
        })
    }
}

impl From<&AutomationTrigger> for TriggerResponse {
    fn from(trigger: &AutomationTrigger) -> Self {
        Self {
            name: trigger.name().as_str().to_string(),
            kind: trigger.kind().to_string(),
            config: json_from_string(trigger.config_json()).unwrap_or_else(|_| json!({})),
            plugin_id: trigger.plugin_id().map(str::to_string),
            enabled: trigger.enabled(),
        }
    }
}

impl From<&CustomTriggerPlugin> for TriggerPluginResponse {
    fn from(plugin: &CustomTriggerPlugin) -> Self {
        Self {
            id: plugin.id().to_string(),
            name: plugin.name().to_string(),
            description: plugin.description().to_string(),
            runtime_image: plugin.runtime_image().to_string(),
            command: plugin.command().to_vec(),
            python_script: plugin_python_script(plugin),
            config_schema: json_from_string(plugin.config_schema_json())
                .unwrap_or_else(|_| json!({})),
        }
    }
}

pub(super) fn plugin_python_script(plugin: &CustomTriggerPlugin) -> String {
    let command = plugin.command();
    if command.len() == 3 && command[0] == "python" && command[1] == "-c" {
        command[2].clone()
    } else {
        String::new()
    }
}
