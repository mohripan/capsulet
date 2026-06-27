use std::collections::HashSet;

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use capsulet_core::{
    Automation, AutomationId, AutomationStatus, AutomationTrigger, ConditionExpr,
    CustomTriggerPlugin, TriggerKind, TriggerName, WorkflowId,
};
use capsulet_storage::ObjectStore;
use serde_json::{Value, json};

use crate::{
    auth::Principal,
    error::ApiError,
    http::{
        assign_resource_project, generated_id, json_from_string, project_context,
        require_project_role, require_resource_project, valid_json_object_string,
    },
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
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateAutomationRequest>,
) -> Result<(StatusCode, Json<AutomationResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, "project_operator")?;
    let build = build_automation(&state, request).await?;
    let automation = build.automation;
    require_resource_project(
        &state.store,
        "workflows",
        automation.workflow_id().as_str(),
        &context,
    )
    .await?;
    require_trigger_plugin_projects(&state.store, &build.triggers, &context).await?;
    state
        .store
        .upsert_automation(&automation)
        .await
        .map_err(ApiError::store)?;
    assign_resource_project(
        &state.store,
        "automations",
        automation.id().as_str(),
        &context,
    )
    .await?;
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
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
    Json(mut request): Json<CreateAutomationRequest>,
) -> Result<Json<AutomationResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, "project_operator")?;
    let automation_id = AutomationId::new(id).map_err(ApiError::validation)?;
    require_resource_project(
        &state.store,
        "automations",
        automation_id.as_str(),
        &context,
    )
    .await?;
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
    require_resource_project(
        &state.store,
        "workflows",
        automation.workflow_id().as_str(),
        &context,
    )
    .await?;
    require_trigger_plugin_projects(&state.store, &build.triggers, &context).await?;
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
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, "project_admin")?;
    let automation_id = AutomationId::new(id).map_err(ApiError::validation)?;
    require_resource_project(
        &state.store,
        "automations",
        automation_id.as_str(),
        &context,
    )
    .await?;
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
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<AutomationResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    set_automation_status(state, headers, principal, id, AutomationStatus::Enabled).await
}

pub(crate) async fn disable_automation<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<AutomationResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    set_automation_status(state, headers, principal, id, AutomationStatus::Disabled).await
}

pub(crate) async fn list_automations<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListAutomationsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let automations = state
        .store
        .list_automations(100)
        .await
        .map_err(ApiError::store)?;
    let mut responses = Vec::with_capacity(automations.len());
    for automation in &automations {
        if require_resource_project(
            &state.store,
            "automations",
            automation.id().as_str(),
            &context,
        )
        .await
        .is_err()
        {
            continue;
        }
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
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<AutomationResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = AutomationId::new(id).map_err(ApiError::validation)?;
    require_resource_project(&state.store, "automations", id.as_str(), &context).await?;
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
    let automation_id = AutomationId::new(
        request
            .id
            .as_deref()
            .map_or_else(|| generated_id("automation"), str::to_string),
    )
    .map_err(ApiError::validation)?;
    let triggers = build_automation_triggers(&automation_id, &request)?;
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
            request
                .status
                .as_deref()
                .map(parse_automation_status)
                .transpose()?
                .unwrap_or(AutomationStatus::Enabled),
        ),
        triggers,
        condition_json: condition.to_string(),
    })
}

async fn set_automation_status<S, O>(
    state: AppState<S, O>,
    headers: HeaderMap,
    principal: Principal,
    id: String,
    status: AutomationStatus,
) -> Result<Json<AutomationResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, "project_operator")?;
    let automation_id = AutomationId::new(id).map_err(ApiError::validation)?;
    require_resource_project(
        &state.store,
        "automations",
        automation_id.as_str(),
        &context,
    )
    .await?;
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
    status
        .parse()
        .map_err(|error: capsulet_core::ParseDomainValueError| {
            ApiError::Validation(error.to_string())
        })
}

struct AutomationBuild {
    automation: Automation,
    triggers: Vec<AutomationTrigger>,
    condition_json: String,
}

fn build_automation_triggers(
    automation_id: &AutomationId,
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

    Ok(vec![AutomationTrigger::new(
        automation_id.clone(),
        TriggerName::new("manual").map_err(ApiError::validation)?,
        TriggerKind::Manual,
        "{}",
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
    let kind = parse_trigger_type(&request.kind)?;
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
    if config
        .get("cron")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
    {
        let timezone_valid = config
            .get("timezone")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty());
        return if timezone_valid {
            Ok(())
        } else {
            Err(ApiError::Validation(
                "cron schedule triggers require timezone".to_string(),
            ))
        };
    }
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
        TriggerKind::Manual | TriggerKind::Webhook | TriggerKind::Custom => Ok(()),
    }
}

fn validate_sql_config(config: &Value) -> Result<(), ApiError> {
    let has_connection = config
        .get("connection_name")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    if !has_connection {
        return Err(ApiError::Validation(
            "sql triggers require connection_name".to_string(),
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

fn parse_trigger_type(kind: &str) -> Result<TriggerKind, ApiError> {
    kind.parse()
        .map_err(|error: capsulet_core::ParseDomainValueError| {
            ApiError::Validation(error.to_string())
        })
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
            vec![AutomationTrigger::new(
                automation.id().clone(),
                TriggerName::new("manual").map_err(ApiError::validation)?,
                TriggerKind::Manual,
                "{}",
                None,
                true,
            )],
            json!({ "trigger": "manual" }).to_string(),
        ));
    }
    Ok((triggers, condition_json))
}

pub(crate) async fn list_automation_triggers<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<ListAutomationTriggersResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let automation_id = AutomationId::new(id).map_err(ApiError::validation)?;
    require_resource_project(
        &state.store,
        "automations",
        automation_id.as_str(),
        &context,
    )
    .await?;
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
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateTriggerPluginRequest>,
) -> Result<(StatusCode, Json<TriggerPluginResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, "project_operator")?;
    let plugin = build_trigger_plugin(request)?;
    state
        .store
        .upsert_custom_trigger_plugin(&plugin)
        .await
        .map_err(ApiError::store)?;
    assign_resource_project(&state.store, "trigger_plugins", plugin.id(), &context).await?;
    Ok((
        StatusCode::CREATED,
        Json(TriggerPluginResponse::from(&plugin)),
    ))
}

pub(crate) async fn list_trigger_plugins<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListTriggerPluginsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let plugins = state
        .store
        .list_custom_trigger_plugins(100)
        .await
        .map_err(ApiError::store)?;
    let mut scoped = Vec::new();
    for plugin in &plugins {
        if require_resource_project(&state.store, "trigger_plugins", plugin.id(), &context)
            .await
            .is_ok()
        {
            scoped.push(TriggerPluginResponse::from(plugin));
        }
    }
    Ok(Json(ListTriggerPluginsResponse {
        trigger_plugins: scoped,
    }))
}

async fn require_trigger_plugin_projects<S: ApiStore>(
    store: &S,
    triggers: &[AutomationTrigger],
    context: &crate::http::ProjectContext,
) -> Result<(), ApiError> {
    for trigger in triggers {
        if let Some(plugin_id) = trigger.plugin_id() {
            require_resource_project(store, "trigger_plugins", plugin_id, context).await?;
        }
    }
    Ok(())
}

pub(crate) async fn get_trigger_plugin<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<TriggerPluginResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_resource_project(&state.store, "trigger_plugins", &id, &context).await?;
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
    let command = if let Some(script) = request.python_script {
        if script.trim().is_empty() {
            return Err(ApiError::Validation(
                "custom trigger python script cannot be empty".to_string(),
            ));
        }
        vec!["python".to_string(), "-c".to_string(), script]
    } else {
        let command = request.command.unwrap_or_default();
        if command.is_empty() {
            return Err(ApiError::Validation(
                "plugin command cannot be empty".to_string(),
            ));
        }
        command
    };

    Ok(CustomTriggerPlugin::new(
        request.id,
        request.name,
        request.description.unwrap_or_default(),
        request.runtime_image,
        command,
        request
            .config_schema
            .unwrap_or_else(|| json!({}))
            .to_string(),
    ))
}
