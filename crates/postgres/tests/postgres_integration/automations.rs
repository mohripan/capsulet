//! Automations and the triggers that start their runs.

use super::*;

#[tokio::test]
async fn saves_and_finds_automation_when_database_is_available() {
    let database_url = database_url();

    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");

    let workflow = WorkflowDefinition::new(
        WorkflowId::new(unique_id("workflow_automation_test")).expect("workflow id"),
        "Automation persistence workflow",
        "",
        WorkflowStatus::Enabled,
        Vec::new(),
    );
    store
        .upsert_workflow(&workflow)
        .await
        .expect("save workflow");

    let automation = Automation::new(
        AutomationId::new(unique_id("automation_interval_test")).expect("automation id"),
        "Interval automation",
        "",
        workflow.id().clone(),
        "{}",
        AutomationStatus::Enabled,
    );
    store
        .upsert_automation(&automation)
        .await
        .expect("save automation");

    let persisted = store
        .find_automation(automation.id())
        .await
        .expect("find automation")
        .expect("automation exists");

    assert_eq!(persisted.status(), AutomationStatus::Enabled);
}

#[tokio::test]
async fn due_schedule_trigger_creates_workflow_run_when_database_is_available() {
    let database_url = database_url();

    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");

    let workflow = WorkflowDefinition::new(
        WorkflowId::new(unique_id("workflow_schedule_trigger_test")).expect("workflow id"),
        "Schedule trigger workflow",
        "",
        WorkflowStatus::Enabled,
        Vec::new(),
    );
    store
        .upsert_workflow(&workflow)
        .await
        .expect("save workflow");

    let automation = Automation::new(
        AutomationId::new(unique_id("automation_schedule_trigger_test")).expect("automation id"),
        "Schedule trigger automation",
        "",
        workflow.id().clone(),
        "{}",
        AutomationStatus::Enabled,
    );
    store
        .upsert_automation(&automation)
        .await
        .expect("save automation");
    store
        .replace_automation_triggers(
            automation.id(),
            &[AutomationTrigger::new(
                automation.id().clone(),
                TriggerName::new("schedule_ready").expect("trigger name"),
                TriggerKind::Schedule,
                "{\"interval_seconds\":30}",
                None,
                true,
            )],
            "{\"trigger\":\"schedule_ready\"}",
        )
        .await
        .expect("save trigger");

    let triggered = store
        .trigger_due_interval_automations()
        .await
        .expect("trigger due schedule automations");
    let created_for_automation: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workflow_runs WHERE automation_id = $1 AND workflow_id = $2",
    )
    .bind(automation.id().as_str())
    .bind(workflow.id().as_str())
    .fetch_one(store.pool())
    .await
    .expect("count workflow runs for automation");

    assert!(triggered >= 1);
    assert_eq!(created_for_automation, 1);
}

#[tokio::test]
async fn custom_trigger_claim_is_exclusive_when_database_is_available() {
    let database_url = database_url();
    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");
    let workflow = WorkflowDefinition::new(
        WorkflowId::new(unique_id("workflow_custom_trigger")).expect("workflow id"),
        "Custom trigger workflow",
        "",
        WorkflowStatus::Enabled,
        Vec::new(),
    );
    store.upsert_workflow(&workflow).await.expect("workflow");
    let automation = Automation::new(
        AutomationId::new(unique_id("automation_custom_trigger")).expect("automation id"),
        "Custom trigger automation",
        "",
        workflow.id().clone(),
        "{}",
        AutomationStatus::Enabled,
    );
    store
        .upsert_automation(&automation)
        .await
        .expect("automation");
    let plugin_id = unique_id("plugin_custom_trigger");
    store
        .upsert_custom_trigger_plugin(&CustomTriggerPlugin::new(
            plugin_id.clone(),
            "Plugin",
            "",
            "example.invalid/plugin@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            vec!["/plugin".to_string()],
            "{}",
        ))
        .await
        .expect("plugin");
    store
        .replace_automation_triggers(
            automation.id(),
            &[AutomationTrigger::new(
                automation.id().clone(),
                TriggerName::new("ready").expect("trigger name"),
                TriggerKind::Custom,
                "{\"poll_seconds\":60}",
                Some(plugin_id),
                true,
            )],
            "{\"trigger\":\"ready\"}",
        )
        .await
        .expect("trigger");

    let claimed = store
        .claim_custom_trigger("evaluator-a", 60)
        .await
        .expect("claim")
        .expect("due custom trigger");
    assert_eq!(claimed.trigger_name, "ready");
    assert_eq!(claimed.automation_id, automation.id().as_str());
    let second_claim = store
        .claim_custom_trigger("evaluator-b", 60)
        .await
        .expect("second claim");
    assert!(second_claim.as_ref().is_none_or(|trigger| {
        trigger.automation_id != claimed.automation_id
            || trigger.trigger_name != claimed.trigger_name
    }));
    store
        .complete_custom_trigger("evaluator-a", &claimed, 60, None, "delivery")
        .await
        .expect("complete");
    if let Some(trigger) = second_claim {
        store
            .complete_custom_trigger("evaluator-b", &trigger, 60, None, "delivery")
            .await
            .expect("complete unrelated trigger claimed during test");
    }
}
