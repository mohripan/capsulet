//! Job definitions, job runs, their logs and their artifacts.

use super::*;

fn queued_run(id: &str, pool: &str) -> JobRun {
    JobRun::new(
        JobRunId::new(id).expect("run id"),
        JobDefinitionId::new("job_hello_python").expect("definition id"),
        ExecutionPoolName::new(pool).expect("pool"),
    )
}

#[tokio::test]
async fn reusing_a_run_id_conflicts_instead_of_overwriting_the_existing_run() {
    let app = authenticated_app(FakeStore::with_definition("dup_job"));
    let submit = |input: &str| {
        let body = json!({
            "job_definition_id": "dup_job",
            "execution_pool": "mini",
            "run_id": "chosen-run-id",
            "input": { "marker": input }
        })
        .to_string();
        app.clone().oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/jobs/runs")
                .header("content-type", "application/json")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .body(Body::from(body))
                .expect("request"),
        )
    };

    assert_eq!(submit("first").await.expect("response").status(), 201);

    let second = submit("second").await.expect("response");
    assert_eq!(second.status(), 409);

    let run = app
        .oneshot(
            Request::builder()
                .uri("/v1/jobs/runs/chosen-run-id")
                .header(
                    "authorization",
                    "Bearer admin-token-0123456789-abcdefghijkl",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let body: Value = serde_json::from_slice(
        &to_bytes(run.into_body(), usize::MAX)
            .await
            .expect("body bytes"),
    )
    .expect("json");
    assert_eq!(body["input"]["marker"], "first");
}

#[tokio::test]
async fn updating_job_definition_preserves_the_previous_immutable_bundle_reference() {
    let app = test_app(FakeStore::default());
    let request = |method, script: &str| {
        Request::builder()
            .uri(if method == Method::POST {
                "/v1/job-definitions"
            } else {
                "/v1/job-definitions/immutable-job"
            })
            .method(method)
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"id":"immutable-job", "name":"Immutable job", "python_script":script})
                    .to_string(),
            ))
            .expect("request")
    };
    let first = app
        .clone()
        .oneshot(request(Method::POST, "print('original')"))
        .await
        .expect("create");
    assert_eq!(first.status(), axum::http::StatusCode::CREATED);
    let first_key = response_json(first).await["bundle_object_key"].clone();
    let second = app
        .oneshot(request(Method::PUT, "print('updated')"))
        .await
        .expect("update");
    assert_eq!(second.status(), axum::http::StatusCode::OK);
    let second_key = response_json(second).await["bundle_object_key"].clone();
    assert_ne!(
        first_key, second_key,
        "an update must not overwrite an earlier bundle's object key"
    );
}

#[tokio::test]
async fn creates_and_reads_reusable_python_job_definition() {
    let app = test_app(FakeStore::default());
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/job-definitions")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id": "job_daily_report",
                        "name": "Daily report",
                        "runtime_image": "python:3.12-slim",
                        "python_script": "print('daily report')",
                        "retry_max_attempts": 2,
                        "retry_delay_seconds": 5
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(create_response.status(), axum::http::StatusCode::CREATED);
    let body = response_json(create_response).await;
    assert_eq!(body["id"], "job_daily_report");
    assert_eq!(body["name"], "Daily report");
    assert_eq!(
        body["bundle_object_key"],
        "bundles/job-definitions/job_daily_report/5e6ea3c3b0b598920ae00f33519df6fd69e375318605fa54a723f6139ad2e5bc/main.py"
    );

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/job-definitions")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(list_response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(list_response).await["job_definitions"][0]["id"],
        "job_daily_report"
    );

    let fetch_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/job-definitions/job_daily_report")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(fetch_response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(fetch_response).await["id"],
        "job_daily_report"
    );

    let source_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/job-definitions/job_daily_report/source")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let source_status = source_response.status();
    let source_body = response_json(source_response).await;
    assert_eq!(source_status, axum::http::StatusCode::OK, "{source_body}");
    assert_eq!(source_body["python_script"], "print('daily report')");
}

#[tokio::test]
async fn reads_inline_python_command_when_source_object_is_missing() {
    let response = test_app(FakeStore::with_definition("job_inline_python"))
        .oneshot(
            Request::builder()
                .uri("/v1/job-definitions/job_inline_python/source")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    let status = response.status();
    let body = response_json(response).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{body}");
    assert_eq!(body["python_script"], "print('fake')");
}

#[tokio::test]
async fn update_job_definition_rejects_definition_used_by_queued_workflow_run() {
    let response = test_app(store_with_queued_workflow_run())
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/v1/job-definitions/job_hello_python")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"name":"Changed","python_script":"print('changed')"}).to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::CONFLICT);
}

#[tokio::test]
async fn delete_job_definition_rejects_definition_used_by_workflow() {
    let response =
        test_app(FakeStore::with_definition("job_hello_python").with_workflow("wf_uses_job"))
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/v1/job-definitions/job_hello_python")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::CONFLICT);
    assert_eq!(
        response_json(response).await["code"],
        "job_definition_in_use"
    );
}

#[tokio::test]
async fn creates_manual_run() {
    let response = test_app(FakeStore::with_definition("job_hello_python"))
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/jobs/runs")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "run_id": "run_api_test",
                        "job_definition_id": "job_hello_python",
                        "host_group": "mini"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    assert_eq!(
        response_json(response).await,
        json!({
            "id": "run_api_test",
            "job_definition_id": "job_hello_python",
            "status": "queued",
            "execution_pool": "mini",
            "host_group": "mini",
            "attempt_count": 0,
            "created_at": "",
            "input": {}
        })
    );
}

#[tokio::test]
async fn rejects_manual_run_when_pool_queue_is_overloaded() {
    let store = FakeStore::with_definition("job_hello_python")
        .with_run(queued_run("run_existing_queued", "mini"));
    let app = test_app_with_admission(
        store.clone(),
        AdmissionConfig {
            max_queued_runs: None,
            max_queued_runs_per_pool: Some(1),
            max_queued_workflow_runs: None,
        },
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/jobs/runs")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "run_id": "run_rejected",
                        "job_definition_id": "job_hello_python",
                        "execution_pool": "mini"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response_json(response).await["code"], "queue_overloaded");
    assert!(
        store
            .runs
            .lock()
            .expect("runs mutex")
            .iter()
            .all(|run| { run.id() != &JobRunId::new("run_rejected").expect("run id") })
    );
}

#[tokio::test]
async fn creates_script_backed_run() {
    let response = test_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/jobs/runs")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "run_id": "run_script_test",
                        "job_definition_id": "script",
                        "execution_pool": "mini",
                        "python_script": "print('from script')"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    let body = response_json(response).await;
    assert_eq!(body["id"], "run_script_test");
    assert_eq!(body["job_definition_id"], "job_definition_run_script_test");
}

#[tokio::test]
async fn rejects_unknown_job_definition() {
    let response = test_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/jobs/runs")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "job_definition_id": "missing",
                        "execution_pool": "mini"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        response_json(response).await["code"],
        json!("unknown_job_definition")
    );
}

#[tokio::test]
async fn rejects_unknown_execution_pool() {
    let response = test_app(FakeStore::with_definition("job_hello_python"))
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/jobs/runs")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "job_definition_id": "job_hello_python",
                        "execution_pool": "gpu"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        response_json(response).await["code"],
        json!("unknown_execution_pool")
    );
}

#[tokio::test]
async fn lists_and_fetches_runs() {
    let run = JobRun::new(
        JobRunId::new("run_listed").expect("valid run id"),
        JobDefinitionId::new("job_hello_python").expect("valid definition id"),
        ExecutionPoolName::new("mini").expect("valid pool"),
    );
    let app = test_app(FakeStore::with_definition("job_hello_python").with_run(run));

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/jobs/runs")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(list_response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(list_response).await["runs"][0]["id"],
        "run_listed"
    );

    let get_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/jobs/runs/run_listed")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(get_response.status(), axum::http::StatusCode::OK);
    assert_eq!(response_json(get_response).await["id"], "run_listed");
}

#[tokio::test]
async fn cancels_run() {
    let run = JobRun::new(
        JobRunId::new("run_cancelled").expect("valid run id"),
        JobDefinitionId::new("job_hello_python").expect("valid definition id"),
        ExecutionPoolName::new("mini").expect("valid pool"),
    );
    let response = test_app(FakeStore::with_definition("job_hello_python").with_run(run))
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/jobs/runs/run_cancelled/cancel")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(response_json(response).await["status"], "cancelled");
}

#[tokio::test]
async fn returns_not_found_for_missing_run() {
    let response = test_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .uri("/v1/jobs/runs/run_missing")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    assert_eq!(
        response_json(response).await["code"],
        json!("job_run_not_found")
    );
}

#[tokio::test]
async fn fetches_run_logs() {
    let run = JobRun::new(
        JobRunId::new("run_with_logs").expect("valid run id"),
        JobDefinitionId::new("job_hello_python").expect("valid definition id"),
        ExecutionPoolName::new("mini").expect("valid pool"),
    );
    let log = JobRunLog::new(run.id().clone(), "hello from logs\n").expect("valid log");
    let response = test_app(
        FakeStore::with_definition("job_hello_python")
            .with_run(run)
            .with_log(log),
    )
    .oneshot(
        Request::builder()
            .uri("/v1/jobs/runs/run_with_logs/logs")
            .body(Body::empty())
            .expect("request"),
    )
    .await
    .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "run_id": "run_with_logs",
            "logs": "hello from logs\n",
            "object_log_available": false
        })
    );
}

#[tokio::test]
async fn returns_not_found_for_missing_run_logs() {
    let run = JobRun::new(
        JobRunId::new("run_without_logs").expect("valid run id"),
        JobDefinitionId::new("job_hello_python").expect("valid definition id"),
        ExecutionPoolName::new("mini").expect("valid pool"),
    );
    let response = test_app(FakeStore::with_definition("job_hello_python").with_run(run))
        .oneshot(
            Request::builder()
                .uri("/v1/jobs/runs/run_without_logs/logs")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    assert_eq!(
        response_json(response).await["code"],
        json!("job_run_logs_not_found")
    );
}

#[tokio::test]
async fn lists_artifacts() {
    let run = JobRun::new(
        JobRunId::new("run_with_artifact").expect("valid run id"),
        JobDefinitionId::new("job_hello_python").expect("valid definition id"),
        ExecutionPoolName::new("mini").expect("valid pool"),
    );
    let artifact = JobArtifact::new(
        ArtifactId::new("artifact_1").expect("artifact id"),
        run.id().clone(),
        None,
        "report.txt",
        "artifacts/run_with_artifact/report.txt",
        "text/plain",
        6,
        None,
        ArtifactObjectKind::Artifact,
    )
    .expect("artifact");
    let response = test_app(
        FakeStore::with_definition("job_hello_python")
            .with_run(run)
            .with_artifact(artifact),
    )
    .oneshot(
        Request::builder()
            .uri("/v1/jobs/runs/run_with_artifact/artifacts")
            .body(Body::empty())
            .expect("request"),
    )
    .await
    .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(response).await["artifacts"][0]["name"],
        "report.txt"
    );
}

#[tokio::test]
async fn downloads_artifact() {
    let run = JobRun::new(
        JobRunId::new("run_with_artifact").expect("valid run id"),
        JobDefinitionId::new("job_hello_python").expect("valid definition id"),
        ExecutionPoolName::new("mini").expect("valid pool"),
    );
    let artifact = JobArtifact::new(
        ArtifactId::new("artifact_1").expect("artifact id"),
        run.id().clone(),
        None,
        "report.txt",
        "artifacts/run_with_artifact/report.txt",
        "text/plain",
        6,
        None,
        ArtifactObjectKind::Artifact,
    )
    .expect("artifact");
    let response = test_app(
        FakeStore::with_definition("job_hello_python")
            .with_run(run)
            .with_artifact(artifact),
    )
    .oneshot(
        Request::builder()
            .uri("/v1/jobs/runs/run_with_artifact/artifacts/artifact_1")
            .body(Body::empty())
            .expect("request"),
    )
    .await
    .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("collect artifact")
        .to_bytes();
    assert_eq!(&bytes[..], b"report");
}
