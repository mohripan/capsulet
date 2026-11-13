//! Workflow definitions, their dependencies, and workflow runs.

use super::*;

#[tokio::test]
async fn creates_and_returns_workflow_dag_dependencies() {
    let app = test_app(FakeStore::with_definition("job_graph"));
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/workflows")
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "id": "workflow_graph",
                    "name": "Graph",
                    "steps": [
                        { "id": "root_a", "name": "A", "job_definition_id": "job_graph", "execution_pool": "mini" },
                        { "id": "root_b", "name": "B", "job_definition_id": "job_graph", "execution_pool": "mini" },
                        { "id": "merge", "name": "Merge", "job_definition_id": "job_graph", "execution_pool": "mini" }
                    ],
                    "dependencies": [
                        { "from_step_id": "root_a", "to_step_id": "merge" },
                        { "from_step_id": "root_b", "to_step_id": "merge" }
                    ]
                }).to_string())).expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    assert_eq!(
        response_json(response).await["dependencies"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/workflows/workflow_graph")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        response_json(response).await["dependencies"][1]["to_step_id"],
        "merge"
    );
}

#[tokio::test]
async fn rejects_cyclic_workflow_dependencies() {
    let app = test_app(FakeStore::with_definition("job_graph"));
    let response = app.oneshot(
        Request::builder().method(Method::POST).uri("/v1/workflows").header("content-type", "application/json")
            .body(Body::from(json!({
                "name": "Cycle",
                "steps": [
                    { "id": "a", "name": "A", "job_definition_id": "job_graph", "execution_pool": "mini" },
                    { "id": "b", "name": "B", "job_definition_id": "job_graph", "execution_pool": "mini" }
                ],
                "dependencies": [
                    { "from_step_id": "a", "to_step_id": "b" },
                    { "from_step_id": "b", "to_step_id": "a" }
                ]
            }).to_string())).expect("request")
    ).await.expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    assert!(
        response_json(response).await["message"]
            .as_str()
            .unwrap()
            .contains("cycle")
    );
}

#[tokio::test]
async fn editability_reports_workflow_locked_by_queued_run() {
    let response = test_app(store_with_queued_workflow_run())
        .oneshot(
            Request::builder()
                .uri("/v1/workflows/wf_locked/editability")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response_json(response).await["editable"], false);
}

#[tokio::test]
async fn update_workflow_rejects_definition_used_by_queued_run() {
    let response = test_app(store_with_queued_workflow_run())
        .oneshot(Request::builder().method(Method::PUT).uri("/v1/workflows/wf_locked").header("content-type", "application/json").body(Body::from(json!({"name":"Changed","steps":[{"name":"Run job","job_definition_id":"job_hello_python","execution_pool":"mini"}]}).to_string())).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::CONFLICT);
}

#[tokio::test]
async fn topology_returns_workflow_and_pool_route() {
    let response =
        test_app(FakeStore::with_definition("job_hello_python").with_workflow("wf_topology"))
            .oneshot(
                Request::builder()
                    .uri("/v1/topology")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
    let body = response_json(response).await;
    assert!(
        body["edges"]
            .as_array()
            .expect("topology edges")
            .iter()
            .any(|edge| edge["from"] == "workflow:wf_topology" && edge["to"] == "pool:mini")
    );
}

#[tokio::test]
async fn fetches_workflow_run_logs_for_step_runs() {
    let workflow_id = WorkflowId::new("wf_logs").expect("workflow id");
    let workflow_run_id = WorkflowRunId::new("workflow_run_logs").expect("workflow run id");
    let first_job_run_id = JobRunId::new("run_step_one").expect("first job run id");
    let second_job_run_id = JobRunId::new("run_step_two").expect("second job run id");
    let workflow_run = WorkflowRun::new(
        workflow_run_id.clone(),
        workflow_id.clone(),
        None,
        "{}",
        WorkflowRunStatus::Running,
        2,
        "2026-06-13 12:00:00+00",
    );
    let first_step_run = WorkflowStepRun::new(
        WorkflowStepRunId::new("workflow_step_run_one").expect("first step run id"),
        workflow_run_id.clone(),
        WorkflowStepId::new("wf_logs_step_1").expect("first step id"),
        Some(first_job_run_id.clone()),
        1,
        WorkflowRunStatus::Succeeded,
    );
    let second_step_run = WorkflowStepRun::new(
        WorkflowStepRunId::new("workflow_step_run_two").expect("second step run id"),
        workflow_run_id.clone(),
        WorkflowStepId::new("wf_logs_step_2").expect("second step id"),
        Some(second_job_run_id.clone()),
        2,
        WorkflowRunStatus::Running,
    );
    let first_log =
        JobRunLog::new(first_job_run_id.clone(), "step one complete\n").expect("first log");
    let log_artifact = JobArtifact::new(
        ArtifactId::new("artifact_stdout_log").expect("artifact id"),
        first_job_run_id,
        None,
        "stdout.log",
        "artifacts/run_step_one/stdout.log",
        "text/plain",
        4096,
        None,
        ArtifactObjectKind::Log,
    )
    .expect("log artifact");

    let response = test_app(
        FakeStore::default()
            .with_workflow_run(workflow_run)
            .with_workflow_step_run(first_step_run)
            .with_workflow_step_run(second_step_run)
            .with_log(first_log)
            .with_artifact(log_artifact),
    )
    .oneshot(
        Request::builder()
            .uri("/v1/workflow-runs/workflow_run_logs/logs")
            .body(Body::empty())
            .expect("request"),
    )
    .await
    .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "workflow_run_id": "workflow_run_logs",
            "workflow_id": "wf_logs",
            "status": "running",
            "entries": [
                {
                    "step_run_id": "workflow_step_run_one",
                    "workflow_step_id": "wf_logs_step_1",
                    "job_run_id": "run_step_one",
                    "position": 1,
                    "status": "succeeded",
                    "logs": "step one complete\n",
                    "object_log_available": true
                },
                {
                    "step_run_id": "workflow_step_run_two",
                    "workflow_step_id": "wf_logs_step_2",
                    "job_run_id": "run_step_two",
                    "position": 2,
                    "status": "running",
                    "logs": "",
                    "object_log_available": false
                }
            ]
        })
    );
}

#[tokio::test]
async fn removes_queued_workflow_run_before_steps_start() {
    let workflow_run = WorkflowRun::new(
        WorkflowRunId::new("workflow_run_remove").expect("workflow run id"),
        WorkflowId::new("wf_remove").expect("workflow id"),
        None,
        "{}",
        WorkflowRunStatus::Queued,
        0,
        "2026-06-13 12:00:00+00",
    );

    let response = test_app(FakeStore::default().with_workflow_run(workflow_run))
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/workflow-runs/workflow_run_remove/remove")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(response_json(response).await["status"], "removed");
}

#[tokio::test]
async fn rejects_removing_workflow_run_after_step_started() {
    let workflow_run_id = WorkflowRunId::new("workflow_run_started").expect("workflow run id");
    let workflow_run = WorkflowRun::new(
        workflow_run_id.clone(),
        WorkflowId::new("wf_started").expect("workflow id"),
        None,
        "{}",
        WorkflowRunStatus::Queued,
        0,
        "2026-06-13 12:00:00+00",
    );
    let step_run = WorkflowStepRun::new(
        WorkflowStepRunId::new("workflow_step_started").expect("step run id"),
        workflow_run_id,
        WorkflowStepId::new("wf_started_step").expect("step id"),
        Some(JobRunId::new("run_started_step").expect("job run id")),
        1,
        WorkflowRunStatus::Running,
    );

    let response = test_app(
        FakeStore::default()
            .with_workflow_run(workflow_run)
            .with_workflow_step_run(step_run),
    )
    .oneshot(
        Request::builder()
            .method(Method::POST)
            .uri("/v1/workflow-runs/workflow_run_started/remove")
            .body(Body::empty())
            .expect("request"),
    )
    .await
    .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(response).await["code"],
        json!("invalid_workflow_run_transition")
    );
}

#[tokio::test]
async fn cancels_running_workflow_run_and_active_step() {
    let workflow_run_id = WorkflowRunId::new("workflow_run_cancel").expect("workflow run id");
    let job_run = JobRun::new(
        JobRunId::new("run_cancel_step").expect("job run id"),
        JobDefinitionId::new("job_hello_python").expect("definition id"),
        ExecutionPoolName::new("mini").expect("pool"),
    );
    let workflow_run = WorkflowRun::new(
        workflow_run_id.clone(),
        WorkflowId::new("wf_cancel").expect("workflow id"),
        None,
        "{}",
        WorkflowRunStatus::Running,
        1,
        "2026-06-13 12:00:00+00",
    );
    let step_run = WorkflowStepRun::new(
        WorkflowStepRunId::new("workflow_step_cancel").expect("step run id"),
        workflow_run_id,
        WorkflowStepId::new("wf_cancel_step").expect("step id"),
        Some(job_run.id().clone()),
        1,
        WorkflowRunStatus::Running,
    );

    let response = test_app(
        FakeStore::with_definition("job_hello_python")
            .with_run(job_run)
            .with_workflow_run(workflow_run)
            .with_workflow_step_run(step_run),
    )
    .oneshot(
        Request::builder()
            .method(Method::POST)
            .uri("/v1/workflow-runs/workflow_run_cancel/cancel")
            .body(Body::empty())
            .expect("request"),
    )
    .await
    .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(response_json(response).await["status"], "cancelled");
}

#[tokio::test]
async fn rejects_cancelling_queued_workflow_run() {
    let workflow_run = WorkflowRun::new(
        WorkflowRunId::new("workflow_run_cancel_queued").expect("workflow run id"),
        WorkflowId::new("wf_cancel_queued").expect("workflow id"),
        None,
        "{}",
        WorkflowRunStatus::Queued,
        0,
        "2026-06-13 12:00:00+00",
    );

    let response = test_app(FakeStore::default().with_workflow_run(workflow_run))
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/workflow-runs/workflow_run_cancel_queued/cancel")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(response).await["code"],
        json!("invalid_workflow_run_transition")
    );
}
