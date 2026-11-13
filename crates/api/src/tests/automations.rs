//! Automations, their triggers, and custom trigger plugins.

use super::*;

fn queued_workflow_run(id: &str, workflow_id: &str) -> WorkflowRun {
    WorkflowRun::new(
        WorkflowRunId::new(id).expect("workflow run id"),
        WorkflowId::new(workflow_id).expect("workflow id"),
        None,
        "{}",
        WorkflowRunStatus::Queued,
        0,
        "",
    )
}

#[tokio::test]
async fn creates_and_lists_custom_trigger_plugins() {
    let app = test_app(FakeStore::default());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/trigger-plugins")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "plugin_customer_threshold",
                        "name": "Customer threshold",
                        "runtime_image": "python:3.12-slim",
                        "python_script": "import json\nprint(json.dumps({'matched': False}))",
                        "config_schema": { "type": "object" }
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    assert_eq!(
        response_json(response).await["id"],
        "plugin_customer_threshold"
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/trigger-plugins")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(
        body["trigger_plugins"][0]["runtime_image"],
        "python:3.12-slim"
    );
    assert_eq!(
        body["trigger_plugins"][0]["python_script"],
        "import json\nprint(json.dumps({'matched': False}))"
    );
}

#[tokio::test]
async fn creates_automation_with_trigger_condition_graph() {
    let store = FakeStore::with_definition("job_hello_python").with_workflow("wf_pipeline");
    store
        .trigger_plugins
        .lock()
        .expect("plugins mutex")
        .push(CustomTriggerPlugin::new(
            "plugin_threshold",
            "Threshold plugin",
            "",
            "python:3.12-slim",
            vec!["python".to_string(), "/plugin/check.py".to_string()],
            "{}",
        ));

    let response = test_app(store)
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/automations")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                            "id": "automation_customer_pipeline",
                            "name": "Customer pipeline",
                            "workflow_id": "wf_pipeline",
                            "triggers": [
                            {
                                "name": "nightly",
                                "kind": "schedule",
                                "config": { "interval_seconds": 300 }
                            },
                            {
                                "name": "orders_changed",
                                "kind": "sql",
                                "config": { "connection_name": "orders", "query": "select 1" }
                            },
                            {
                                "name": "threshold",
                                "kind": "custom",
                                "plugin_id": "plugin_threshold",
                                "config": { "limit": 10 }
                            }
                        ],
                        "condition": {
                            "all": [
                                { "trigger": "nightly" },
                                {
                                    "any": [
                                        { "trigger": "orders_changed" },
                                        { "trigger": "threshold" }
                                    ]
                                }
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    let body = response_json(response).await;
    assert!(body.get("trigger_kind").is_none());
    assert!(body.get("interval_seconds").is_none());
    assert_eq!(body["triggers"][2]["kind"], "custom");
    assert_eq!(
        body["condition"]["all"][1]["any"][0]["trigger"],
        "orders_changed"
    );
}

#[tokio::test]
async fn creates_cron_automation_without_legacy_interval_seconds() {
    let response =
        test_app(FakeStore::with_definition("job_hello_python").with_workflow("wf_pipeline"))
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/automations")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "id": "automation_cron",
                            "name": "Cron automation",
                            "workflow_id": "wf_pipeline",
                            "triggers": [{
                                "name": "nightly",
                                "kind": "schedule",
                                "config": { "cron": "0 0 * * * *", "timezone": "UTC" }
                            }],
                            "condition": { "trigger": "nightly" }
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    let body = response_json(response).await;
    assert!(body.get("trigger_kind").is_none());
    assert!(body.get("interval_seconds").is_none());
    assert_eq!(body["triggers"][0]["config"]["timezone"], "UTC");
}

#[tokio::test]
async fn creates_sql_automation_without_legacy_interval_seconds() {
    let response =
        test_app(FakeStore::with_definition("job_hello_python").with_workflow("wf_pipeline"))
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/automations")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "id": "automation_sql",
                            "name": "SQL automation",
                            "workflow_id": "wf_pipeline",
                            "triggers": [{
                                "name": "ready",
                                "kind": "sql",
                                "config": { "connection_name": "control", "query": "SELECT true" }
                            }],
                            "condition": { "trigger": "ready" }
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    assert_eq!(response_json(response).await["triggers"][0]["kind"], "sql");
}

#[tokio::test]
async fn disabled_automation_can_still_be_triggered_manually() {
    let app = test_app(FakeStore::with_definition("job_hello_python").with_workflow("wf_pipeline"));
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/automations")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "automation_toggle_test",
                        "name": "Toggle test",
                        "workflow_id": "wf_pipeline",
                        "triggers": [{ "name": "manual_ready", "kind": "manual", "config": {} }],
                        "condition": { "trigger": "manual_ready" }
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(create_response.status(), axum::http::StatusCode::CREATED);

    let disable_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/automations/automation_toggle_test/disable")
                .body(Body::from("{}"))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(disable_response.status(), axum::http::StatusCode::OK);
    assert_eq!(response_json(disable_response).await["status"], "disabled");

    let disabled_trigger_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/automations/automation_toggle_test/trigger")
                .body(Body::from("{}"))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        disabled_trigger_response.status(),
        axum::http::StatusCode::CREATED
    );

    let enable_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/automations/automation_toggle_test/enable")
                .body(Body::from("{}"))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(enable_response.status(), axum::http::StatusCode::OK);
    assert_eq!(response_json(enable_response).await["status"], "enabled");

    let trigger_response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/automations/automation_toggle_test/trigger")
                .body(Body::from("{}"))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(trigger_response.status(), axum::http::StatusCode::CREATED);
}

#[tokio::test]
async fn rejects_manual_automation_trigger_when_workflow_queue_is_overloaded() {
    let store = FakeStore::with_definition("job_hello_python")
        .with_workflow("wf_pipeline")
        .with_workflow_run(queued_workflow_run("workflow_run_existing", "wf_pipeline"));
    let app = test_app_with_admission(
        store.clone(),
        AdmissionConfig {
            max_queued_runs: None,
            max_queued_runs_per_pool: None,
            max_queued_workflow_runs: Some(1),
        },
    );
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/automations")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "automation_overload_test",
                        "name": "Overload test",
                        "workflow_id": "wf_pipeline",
                        "triggers": [{ "name": "manual_ready", "kind": "manual", "config": {} }],
                        "condition": { "trigger": "manual_ready" }
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(create_response.status(), axum::http::StatusCode::CREATED);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/automations/automation_overload_test/trigger")
                .body(Body::from("{}"))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response_json(response).await["code"], "queue_overloaded");
    assert_eq!(
        store
            .workflow_runs
            .lock()
            .expect("workflow runs mutex")
            .len(),
        1
    );
}

#[tokio::test]
async fn updates_existing_automation() {
    let app = test_app(FakeStore::with_definition("job_hello_python").with_workflow("wf_pipeline"));
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/automations")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "automation_update_test",
                        "name": "Before update",
                        "workflow_id": "wf_pipeline",
                        "triggers": [{ "name": "manual_ready", "kind": "manual", "config": {} }],
                        "condition": { "trigger": "manual_ready" }
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(create_response.status(), axum::http::StatusCode::CREATED);

    let update_response = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/v1/automations/automation_update_test")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "After update",
                        "workflow_id": "wf_pipeline",
                        "status": "disabled",
                        "job_input": { "email": "mohripan16@gmail.com" },
                        "triggers": [{
                            "name": "schedule_ready",
                            "kind": "schedule",
                            "config": {
                                "start_at": "2026-06-13T09:00",
                                "interval_seconds": 600,
                                "window_seconds": 60
                            }
                        }],
                        "condition": { "trigger": "schedule_ready" }
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(update_response.status(), axum::http::StatusCode::OK);
    let body = response_json(update_response).await;
    assert_eq!(body["name"], "After update");
    assert_eq!(body["status"], "disabled");
    assert!(body.get("trigger_kind").is_none());
    assert!(body.get("interval_seconds").is_none());
    assert_eq!(body["job_input"]["email"], "mohripan16@gmail.com");
    assert_eq!(body["triggers"][0]["name"], "schedule_ready");
}

#[tokio::test]
async fn deletes_existing_automation() {
    let app = test_app(FakeStore::with_definition("job_hello_python").with_workflow("wf_pipeline"));
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/automations")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "automation_delete_test",
                        "name": "Delete test",
                        "workflow_id": "wf_pipeline",
                        "triggers": [{ "name": "manual_ready", "kind": "manual", "config": {} }],
                        "condition": { "trigger": "manual_ready" }
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(create_response.status(), axum::http::StatusCode::CREATED);

    let delete_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/v1/automations/automation_delete_test")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(delete_response.status(), axum::http::StatusCode::NO_CONTENT);

    let get_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/automations/automation_delete_test")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(get_response.status(), axum::http::StatusCode::NOT_FOUND);
}
