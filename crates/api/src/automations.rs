use std::collections::HashSet;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use capsulet_core::{
    Automation, AutomationId, AutomationSettings, AutomationStatus, AutomationTrigger,
    AutomationTriggerKind, ConditionExpr, CustomTriggerPlugin, TriggerKind, TriggerName,
    WorkflowId,
};
use capsulet_storage::ObjectStore;
use serde_json::{Value, json};

use crate::{
    error::ApiError,
    http::{generated_id, json_from_string, valid_json_object_string},
    models::{
        AutomationResponse, CreateAutomationRequest, CreateAutomationTriggerRequest,
        CreateTriggerPluginRequest, ListAutomationTriggersResponse, ListAutomationsResponse,
        ListTriggerPluginsResponse, TriggerPluginResponse, TriggerResponse,
    },
    state::AppState,
    store::ApiStore,
};
pub(crate) async fn create_automation<S, O>(
    State(state): State<AppState<S, O>>,
    Json(request): Json<CreateAutomationRequest>,
) -> Result<(StatusCode, Json<AutomationResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let build = build_automation(&state, request).await?;
    let automation = build.automation;
    state
        .store
        .upsert_automation(&automation)
        .await
        .map_err(ApiError::store)?;
    state
        .store
        .replace_automation_triggers(automation.id(), &build.triggers, &build.condition_json)
        .await
        .map_err(ApiError::store)?;

    Ok((
        StatusCode::CREATED,
        Json(AutomationResponse::new(
            &automation,
            &build.triggers,
            &build.condition_json,
        )?),
    ))
}

pub(crate) async fn update_automation<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
    Json(mut request): Json<CreateAutomationRequest>,
) -> Result<Json<AutomationResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let automation_id = AutomationId::new(id).map_err(ApiError::validation)?;
    if state
        .store
        .find_automation(&automation_id)
        .await
        .map_err(ApiError::store)?
        .is_none()
    {
        return Err(ApiError::AutomationNotFound(
            automation_id.as_str().to_string(),
        ));
    }
    request.id = Some(automation_id.as_str().to_string());
    let build = build_automation(&state, request).await?;
    let automation = build.automation;
    state
        .store
        .upsert_automation(&automation)
        .await
        .map_err(ApiError::store)?;
    state
        .store
        .replace_automation_triggers(automation.id(), &build.triggers, &build.condition_json)
        .await
        .map_err(ApiError::store)?;

    Ok(Json(AutomationResponse::new(
        &automation,
        &build.triggers,
        &build.condition_json,
    )?))
}

pub(crate) async fn delete_automation<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let automation_id = AutomationId::new(id).map_err(ApiError::validation)?;
    let deleted = state
        .store
        .delete_automation(&automation_id)
        .await
        .map_err(ApiError::store)?;
    if !deleted {
        return Err(ApiError::AutomationNotFound(
            automation_id.as_str().to_string(),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn enable_automation<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<AutomationResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    set_automation_status(state, id, AutomationStatus::Enabled).await
}

pub(crate) async fn disable_automation<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<AutomationResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    set_automation_status(state, id, AutomationStatus::Disabled).await
}

pub(crate) async fn list_automations<S, O>(
    State(state): State<AppState<S, O>>,
) -> Result<Json<ListAutomationsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let automations = state
        .store
        .list_automations(100)
        .await
        .map_err(ApiError::store)?;
    let mut responses = Vec::with_capacity(automations.len());
    for automation in &automations {
        let (triggers, condition_json) = trigger_graph_for_response(&state, automation).await?;
        responses.push(AutomationResponse::new(
            automation,
            &triggers,
            &condition_json,
        )?);
    }
    Ok(Json(ListAutomationsResponse {
        automations: responses,
    }))
}

pub(crate) async fn get_automation<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<AutomationResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = AutomationId::new(id).map_err(ApiError::validation)?;
    let Some(automation) = state
        .store
        .find_automation(&id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::AutomationNotFound(id.as_str().to_string()));
    };
    let (triggers, condition_json) = trigger_graph_for_response(&state, &automation).await?;
    Ok(Json(AutomationResponse::new(
        &automation,
        &triggers,
        &condition_json,
    )?))
}

async fn build_automation<S, O>(
    state: &AppState<S, O>,
    request: CreateAutomationRequest,
) -> Result<AutomationBuild, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let workflow_id = WorkflowId::new(request.workflow_id.clone()).map_err(ApiError::validation)?;
    if state
        .store
        .find_workflow(&workflow_id)
        .await
        .map_err(ApiError::store)?
        .is_none()
    {
        return Err(ApiError::WorkflowNotFound(workflow_id.as_str().to_string()));
    }
    let requested_trigger_kind = request
        .trigger_kind
        .as_deref()
        .or_else(|| {
            request
                .triggers
                .as_ref()?
                .first()
                .map(|trigger| trigger.kind.as_str())
        })
        .unwrap_or("manual");
    let trigger_kind = match requested_trigger_kind {
        "manual" => AutomationTriggerKind::Manual,
        "interval" | "schedule" => AutomationTriggerKind::Interval,
        value => {
            return Err(ApiError::Validation(format!(
                "unsupported automation trigger kind: {value}"
            )));
        }
    };
    let inferred_interval_seconds = request.interval_seconds.or_else(|| {
        request
            .triggers
            .as_ref()?
            .iter()
            .find(|trigger| trigger.kind == "schedule")?
            .config
            .get("interval_seconds")
            .and_then(Value::as_i64)
    });
    if trigger_kind == AutomationTriggerKind::Interval && inferred_interval_seconds.is_none() {
        return Err(ApiError::Validation(
            "interval automations require interval_seconds".to_string(),
        ));
    }
    let automation_id = AutomationId::new(
        request
            .id
            .as_deref()
            .map_or_else(|| generated_id("automation"), str::to_string),
    )
    .map_err(ApiError::validation)?;
    let triggers = build_automation_triggers(&automation_id, trigger_kind, &request)?;
    validate_custom_plugin_references(state, &triggers).await?;
    validate_trigger_contracts(state, &triggers).await?;
    let condition = request.condition.unwrap_or_else(|| {
        let trigger_name = triggers
            .first()
            .map_or("manual", |trigger| trigger.name().as_str());
        json!({ "trigger": trigger_name })
    });
    validate_condition_json(&condition, &triggers)?;

    Ok(AutomationBuild {
        automation: Automation::new(
            automation_id,
            request.name,
            request.description.unwrap_or_default(),
            workflow_id,
            valid_json_object_string(
                &request.job_input.unwrap_or_else(|| json!({})),
                "automation job input",
            )?,
            AutomationSettings::new(
                request
                    .status
                    .as_deref()
                    .map(parse_automation_status)
                    .transpose()?
                    .unwrap_or(AutomationStatus::Enabled),
                trigger_kind,
                inferred_interval_seconds,
            ),
        ),
        triggers,
        condition_json: condition.to_string(),
    })
}

async fn set_automation_status<S, O>(
    state: AppState<S, O>,
    id: String,
    status: AutomationStatus,
) -> Result<Json<AutomationResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let automation_id = AutomationId::new(id).map_err(ApiError::validation)?;
    let Some(automation) = state
        .store
        .set_automation_status(&automation_id, status)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::AutomationNotFound(
            automation_id.as_str().to_string(),
        ));
    };
    let (triggers, condition_json) = trigger_graph_for_response(&state, &automation).await?;
    Ok(Json(AutomationResponse::new(
        &automation,
        &triggers,
        &condition_json,
    )?))
}

fn parse_automation_status(status: &str) -> Result<AutomationStatus, ApiError> {
    match status {
        "enabled" => Ok(AutomationStatus::Enabled),
        "disabled" => Ok(AutomationStatus::Disabled),
        value => Err(ApiError::Validation(format!(
            "unsupported automation status: {value}"
        ))),
    }
}

struct AutomationBuild {
    automation: Automation,
    triggers: Vec<AutomationTrigger>,
    condition_json: String,
}

fn build_automation_triggers(
    automation_id: &AutomationId,
    trigger_kind: AutomationTriggerKind,
    request: &CreateAutomationRequest,
) -> Result<Vec<AutomationTrigger>, ApiError> {
    if let Some(triggers) = &request.triggers {
        if triggers.is_empty() {
            return Err(ApiError::Validation(
                "automation must include at least one trigger".to_string(),
            ));
        }
        return triggers
            .iter()
            .map(|trigger| build_trigger(automation_id, trigger))
            .collect();
    }

    let name = if trigger_kind == AutomationTriggerKind::Interval {
        "schedule"
    } else {
        "manual"
    };
    let kind = if trigger_kind == AutomationTriggerKind::Interval {
        TriggerKind::Schedule
    } else {
        TriggerKind::Manual
    };
    let config_json = if let Some(interval_seconds) = request.interval_seconds {
        json!({ "interval_seconds": interval_seconds }).to_string()
    } else {
        "{}".to_string()
    };

    Ok(vec![AutomationTrigger::new(
        automation_id.clone(),
        TriggerName::new(name).map_err(ApiError::validation)?,
        kind,
        config_json,
        None,
        true,
    )])
}

async fn validate_custom_plugin_references<S, O>(
    state: &AppState<S, O>,
    triggers: &[AutomationTrigger],
) -> Result<(), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    for trigger in triggers {
        if trigger.kind() != TriggerKind::Custom {
            continue;
        }
        let Some(plugin_id) = trigger.plugin_id() else {
            return Err(ApiError::Validation(
                "custom triggers require plugin_id".to_string(),
            ));
        };
        if state
            .store
            .find_custom_trigger_plugin(plugin_id)
            .await
            .map_err(ApiError::store)?
            .is_none()
        {
            return Err(ApiError::TriggerPluginNotFound(plugin_id.to_string()));
        }
    }
    Ok(())
}

fn build_trigger(
    automation_id: &AutomationId,
    request: &CreateAutomationTriggerRequest,
) -> Result<AutomationTrigger, ApiError> {
    let kind = parse_trigger_kind(&request.kind)?;
    if kind == TriggerKind::Custom && request.plugin_id.as_deref().unwrap_or("").trim().is_empty() {
        return Err(ApiError::Validation(
            "custom triggers require plugin_id".to_string(),
        ));
    }
    if kind != TriggerKind::Custom && request.plugin_id.is_some() {
        return Err(ApiError::Validation(
            "plugin_id is only valid for custom triggers".to_string(),
        ));
    }
    validate_builtin_trigger_config(kind, &request.config)?;

    Ok(AutomationTrigger::new(
        automation_id.clone(),
        TriggerName::new(request.name.clone()).map_err(ApiError::validation)?,
        kind,
        request.config.to_string(),
        request.plugin_id.clone(),
        request.enabled.unwrap_or(true),
    ))
}

fn validate_schedule_config(config: &Value) -> Result<(), ApiError> {
    let has_start_at = config
        .get("start_at")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    let has_interval = config
        .get("interval_seconds")
        .and_then(Value::as_i64)
        .is_some_and(|seconds| seconds > 0);
    let has_window = config
        .get("window_seconds")
        .and_then(Value::as_i64)
        .is_some_and(|seconds| seconds > 0);
    if has_interval && (!has_start_at || has_window) {
        Ok(())
    } else {
        Err(ApiError::Validation(
            "schedule triggers require start_at, interval_seconds, and window_seconds".to_string(),
        ))
    }
}

fn validate_builtin_trigger_config(kind: TriggerKind, config: &Value) -> Result<(), ApiError> {
    match kind {
        TriggerKind::Schedule => validate_schedule_config(config),
        TriggerKind::Sql => validate_sql_config(config),
        TriggerKind::Manual | TriggerKind::Custom => Ok(()),
    }
}

fn validate_sql_config(config: &Value) -> Result<(), ApiError> {
    let has_connection = ["connection_string", "connection_name"]
        .iter()
        .any(|field| {
            config
                .get(field)
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
        });
    if !has_connection {
        return Err(ApiError::Validation(
            "sql triggers require connection_string".to_string(),
        ));
    }
    for field in ["query"] {
        if config
            .get(field)
            .and_then(Value::as_str)
            .is_none_or(|value| value.trim().is_empty())
        {
            return Err(ApiError::Validation(format!(
                "sql triggers require {field}"
            )));
        }
    }
    Ok(())
}

async fn validate_trigger_contracts<S, O>(
    state: &AppState<S, O>,
    triggers: &[AutomationTrigger],
) -> Result<(), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    for trigger in triggers {
        if trigger.kind() != TriggerKind::Custom {
            continue;
        }
        let Some(plugin_id) = trigger.plugin_id() else {
            continue;
        };
        let Some(plugin) = state
            .store
            .find_custom_trigger_plugin(plugin_id)
            .await
            .map_err(ApiError::store)?
        else {
            continue;
        };
        validate_contract_fields(
            &json_from_string(plugin.config_schema_json())?,
            &json_from_string(trigger.config_json())?,
            "custom trigger",
        )?;
    }
    Ok(())
}

fn validate_contract_fields(schema: &Value, values: &Value, label: &str) -> Result<(), ApiError> {
    let Some(fields) = schema.get("fields").and_then(Value::as_array) else {
        return Ok(());
    };
    for field in fields {
        let Some(name) = field.get("name").and_then(Value::as_str) else {
            continue;
        };
        let required = field
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if required && values.get(name).is_none_or(Value::is_null) {
            return Err(ApiError::Validation(format!(
                "{label} config is missing required field {name}"
            )));
        }
    }
    Ok(())
}

fn validate_condition_json(
    condition: &Value,
    triggers: &[AutomationTrigger],
) -> Result<(), ApiError> {
    let expression = condition_expr_from_json(condition)?;
    let trigger_names = triggers
        .iter()
        .map(|trigger| trigger.name().clone())
        .collect::<HashSet<_>>();
    expression
        .validate_references(&trigger_names)
        .map_err(ApiError::validation)
}

fn condition_expr_from_json(value: &Value) -> Result<ConditionExpr, ApiError> {
    if let Some(trigger) = value.get("trigger").and_then(Value::as_str) {
        return Ok(ConditionExpr::Trigger(
            TriggerName::new(trigger).map_err(ApiError::validation)?,
        ));
    }
    if let Some(all) = value.get("all").and_then(Value::as_array) {
        return Ok(ConditionExpr::All(
            all.iter()
                .map(condition_expr_from_json)
                .collect::<Result<Vec<_>, _>>()?,
        ));
    }
    if let Some(any) = value.get("any").and_then(Value::as_array) {
        return Ok(ConditionExpr::Any(
            any.iter()
                .map(condition_expr_from_json)
                .collect::<Result<Vec<_>, _>>()?,
        ));
    }
    Err(ApiError::Validation(
        "condition must contain trigger, all, or any".to_string(),
    ))
}

fn parse_trigger_kind(kind: &str) -> Result<TriggerKind, ApiError> {
    match kind {
        "manual" => Ok(TriggerKind::Manual),
        "schedule" => Ok(TriggerKind::Schedule),
        "sql" => Ok(TriggerKind::Sql),
        "custom" => Ok(TriggerKind::Custom),
        value => Err(ApiError::Validation(format!(
            "unsupported automation trigger kind: {value}"
        ))),
    }
}

async fn trigger_graph_for_response<S, O>(
    state: &AppState<S, O>,
    automation: &Automation,
) -> Result<(Vec<AutomationTrigger>, String), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let (triggers, condition_json) = state
        .store
        .list_automation_triggers(automation.id())
        .await
        .map_err(ApiError::store)?;
    if triggers.is_empty() {
        return Ok((
            legacy_triggers(automation)?,
            json!({ "trigger": legacy_trigger_name(automation) }).to_string(),
        ));
    }
    Ok((triggers, condition_json))
}

fn legacy_triggers(automation: &Automation) -> Result<Vec<AutomationTrigger>, ApiError> {
    let trigger_name = legacy_trigger_name(automation);
    let kind = if automation.trigger_kind() == AutomationTriggerKind::Interval {
        TriggerKind::Schedule
    } else {
        TriggerKind::Manual
    };
    let config_json = automation.interval_seconds().map_or_else(
        || "{}".to_string(),
        |seconds| json!({ "interval_seconds": seconds }).to_string(),
    );
    Ok(vec![AutomationTrigger::new(
        automation.id().clone(),
        TriggerName::new(trigger_name).map_err(ApiError::validation)?,
        kind,
        config_json,
        None,
        true,
    )])
}

fn legacy_trigger_name(automation: &Automation) -> &'static str {
    if automation.trigger_kind() == AutomationTriggerKind::Interval {
        "schedule"
    } else {
        "manual"
    }
}

pub(crate) async fn list_automation_triggers<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<ListAutomationTriggersResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let automation_id = AutomationId::new(id).map_err(ApiError::validation)?;
    let Some(automation) = state
        .store
        .find_automation(&automation_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::AutomationNotFound(
            automation_id.as_str().to_string(),
        ));
    };
    let (triggers, condition_json) = trigger_graph_for_response(&state, &automation).await?;
    Ok(Json(ListAutomationTriggersResponse {
        triggers: triggers.iter().map(TriggerResponse::from).collect(),
        condition: json_from_string(&condition_json)?,
    }))
}

pub(crate) async fn create_trigger_plugin<S, O>(
    State(state): State<AppState<S, O>>,
    Json(request): Json<CreateTriggerPluginRequest>,
) -> Result<(StatusCode, Json<TriggerPluginResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let plugin = build_trigger_plugin(request)?;
    state
        .store
        .upsert_custom_trigger_plugin(&plugin)
        .await
        .map_err(ApiError::store)?;
    Ok((
        StatusCode::CREATED,
        Json(TriggerPluginResponse::from(&plugin)),
    ))
}

pub(crate) async fn list_trigger_plugins<S, O>(
    State(state): State<AppState<S, O>>,
) -> Result<Json<ListTriggerPluginsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let plugins = state
        .store
        .list_custom_trigger_plugins(100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListTriggerPluginsResponse {
        trigger_plugins: plugins.iter().map(TriggerPluginResponse::from).collect(),
    }))
}

pub(crate) async fn get_trigger_plugin<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<TriggerPluginResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let Some(plugin) = state
        .store
        .find_custom_trigger_plugin(&id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::TriggerPluginNotFound(id));
    };
    Ok(Json(TriggerPluginResponse::from(&plugin)))
}

fn build_trigger_plugin(
    request: CreateTriggerPluginRequest,
) -> Result<CustomTriggerPlugin, ApiError> {
    if request.id.trim().is_empty() {
        return Err(ApiError::Validation(
            "plugin id cannot be empty".to_string(),
        ));
    }
    if request.name.trim().is_empty() {
        return Err(ApiError::Validation(
            "plugin name cannot be empty".to_string(),
        ));
    }
    if request.runtime_image.trim().is_empty() {
        return Err(ApiError::Validation(
            "plugin runtime_image cannot be empty".to_string(),
        ));
    }
    if request.command.is_empty() {
        return Err(ApiError::Validation(
            "plugin command cannot be empty".to_string(),
        ));
    }

    Ok(CustomTriggerPlugin::new(
        request.id,
        request.name,
        request.description.unwrap_or_default(),
        request.runtime_image,
        request.command,
        request
            .config_schema
            .unwrap_or_else(|| json!({}))
            .to_string(),
    ))
}
