use std::sync::{Arc, Mutex};

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request},
};
use capsulet_core::{
    ArtifactId, ArtifactObjectKind, Automation, AutomationId, AutomationTrigger,
    CustomTriggerPlugin, ExecutionPoolName, JobArtifact, JobDefinition, JobDefinitionId, JobRun,
    JobRunId, JobRunLog, JobRunStatus, WorkflowDefinition, WorkflowId, WorkflowRun, WorkflowRunId,
    WorkflowStatus, WorkflowStep, WorkflowStepId, WorkflowStepRun,
};
use capsulet_storage::ObjectStore;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use super::{ApiStore, AppState, router};

#[derive(Debug, Clone, Default)]
struct FakeStore {
    known_definitions: Arc<Mutex<Vec<String>>>,
    runs: Arc<Mutex<Vec<JobRun>>>,
    logs: Arc<Mutex<Vec<JobRunLog>>>,
    artifacts: Arc<Mutex<Vec<JobArtifact>>>,
    workflows: Arc<Mutex<Vec<WorkflowDefinition>>>,
    automations: Arc<Mutex<Vec<Automation>>>,
    automation_triggers: Arc<Mutex<Vec<AutomationTrigger>>>,
    automation_conditions: Arc<Mutex<Vec<(String, String)>>>,
    trigger_plugins: Arc<Mutex<Vec<CustomTriggerPlugin>>>,
    workflow_runs: Arc<Mutex<Vec<WorkflowRun>>>,
}

#[derive(Debug, Clone, Default)]
struct FakeObjectStore {
    objects: ObjectMap,
}

type ObjectMap = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

#[async_trait::async_trait]
impl ObjectStore for FakeObjectStore {
    type Error = String;

    async fn put(&self, key: &str, bytes: Vec<u8>) -> Result<(), Self::Error> {
        self.objects
            .lock()
            .map_err(|error| error.to_string())?
            .push((key.to_string(), bytes));
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self
            .objects
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .rev()
            .find(|(stored_key, _)| stored_key == key)
            .map(|(_, bytes)| bytes.clone()))
    }

    async fn exists(&self, key: &str) -> Result<bool, Self::Error> {
        Ok(self
            .objects
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .any(|(stored_key, _)| stored_key == key))
    }
}

#[async_trait::async_trait]
impl ApiStore for FakeStore {
    type Error = String;

    async fn job_definition_exists(&self, id: &JobDefinitionId) -> Result<bool, Self::Error> {
        Ok(self
            .known_definitions
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .any(|known| known == id.as_str()))
    }

    async fn save_run(&self, run: &JobRun) -> Result<(), Self::Error> {
        self.runs
            .lock()
            .map_err(|error| error.to_string())?
            .push(run.clone());
        Ok(())
    }

    async fn upsert_job_definition(&self, definition: &JobDefinition) -> Result<(), Self::Error> {
        let mut definitions = self
            .known_definitions
            .lock()
            .map_err(|error| error.to_string())?;
        if !definitions
            .iter()
            .any(|known| known == definition.id.as_str())
        {
            definitions.push(definition.id.as_str().to_string());
        }
        Ok(())
    }

    async fn list_job_definitions(&self, limit: i64) -> Result<Vec<JobDefinition>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .known_definitions
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .take(limit)
            .map(|id| {
                JobDefinition::new(
                    JobDefinitionId::new(id.clone()).expect("fake definition id"),
                    id.clone(),
                    "python:3.12-slim",
                    vec![
                        "python".to_string(),
                        "-c".to_string(),
                        "print('fake')".to_string(),
                    ],
                    format!("bundles/{id}.py"),
                    "{}",
                    capsulet_core::RetryPolicy::no_retry(),
                )
                .expect("fake definition")
            })
            .collect())
    }

    async fn find_job_definition(
        &self,
        id: &JobDefinitionId,
    ) -> Result<Option<JobDefinition>, Self::Error> {
        Ok(self
            .known_definitions
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|known| *known == id.as_str())
            .map(|known| {
                JobDefinition::new(
                    id.clone(),
                    known.clone(),
                    "python:3.12-slim",
                    vec![
                        "python".to_string(),
                        "-c".to_string(),
                        "print('fake')".to_string(),
                    ],
                    format!("bundles/{known}.py"),
                    "{}",
                    capsulet_core::RetryPolicy::no_retry(),
                )
                .expect("fake definition")
            }))
    }

    async fn delete_job_definition(&self, id: &JobDefinitionId) -> Result<bool, Self::Error> {
        let mut definitions = self
            .known_definitions
            .lock()
            .map_err(|error| error.to_string())?;
        let initial_len = definitions.len();
        definitions.retain(|known| known != id.as_str());
        Ok(definitions.len() != initial_len)
    }

    async fn upsert_workflow(&self, workflow: &WorkflowDefinition) -> Result<(), Self::Error> {
        let mut workflows = self.workflows.lock().map_err(|error| error.to_string())?;
        workflows.retain(|existing| existing.id != workflow.id);
        workflows.push(workflow.clone());
        Ok(())
    }

    async fn list_workflows(&self, limit: i64) -> Result<Vec<WorkflowDefinition>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .workflows
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_workflow(
        &self,
        id: &WorkflowId,
    ) -> Result<Option<WorkflowDefinition>, Self::Error> {
        Ok(self
            .workflows
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|workflow| workflow.id == *id)
            .cloned())
    }

    async fn upsert_automation(&self, automation: &Automation) -> Result<(), Self::Error> {
        let mut automations = self.automations.lock().map_err(|error| error.to_string())?;
        automations.retain(|existing| existing.id != automation.id);
        automations.push(automation.clone());
        Ok(())
    }

    async fn list_automations(&self, limit: i64) -> Result<Vec<Automation>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .automations
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_automation(&self, id: &AutomationId) -> Result<Option<Automation>, Self::Error> {
        Ok(self
            .automations
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|automation| automation.id == *id)
            .cloned())
    }

    async fn replace_automation_triggers(
        &self,
        automation_id: &AutomationId,
        triggers: &[AutomationTrigger],
        condition_json: &str,
    ) -> Result<(), Self::Error> {
        let mut stored_triggers = self
            .automation_triggers
            .lock()
            .map_err(|error| error.to_string())?;
        stored_triggers.retain(|trigger| trigger.automation_id != *automation_id);
        stored_triggers.extend(triggers.iter().cloned());
        let mut conditions = self
            .automation_conditions
            .lock()
            .map_err(|error| error.to_string())?;
        conditions.retain(|(id, _)| id != automation_id.as_str());
        conditions.push((
            automation_id.as_str().to_string(),
            condition_json.to_string(),
        ));
        Ok(())
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
            .filter(|trigger| trigger.automation_id == *automation_id)
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
    ) -> Result<(), Self::Error> {
        let mut plugins = self
            .trigger_plugins
            .lock()
            .map_err(|error| error.to_string())?;
        plugins.retain(|existing| existing.id != plugin.id);
        plugins.push(plugin.clone());
        Ok(())
    }

    async fn list_custom_trigger_plugins(
        &self,
        limit: i64,
    ) -> Result<Vec<CustomTriggerPlugin>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .trigger_plugins
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_custom_trigger_plugin(
        &self,
        id: &str,
    ) -> Result<Option<CustomTriggerPlugin>, Self::Error> {
        Ok(self
            .trigger_plugins
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|plugin| plugin.id == id)
            .cloned())
    }

    async fn create_workflow_run(
        &self,
        workflow_id: &WorkflowId,
        automation_id: Option<&AutomationId>,
        run_id: &WorkflowRunId,
    ) -> Result<WorkflowRun, Self::Error> {
        let run = WorkflowRun {
            id: run_id.clone(),
            workflow_id: workflow_id.clone(),
            automation_id: automation_id.cloned(),
            status: capsulet_core::WorkflowRunStatus::Queued,
            current_step_position: 0,
        };
        self.workflow_runs
            .lock()
            .map_err(|error| error.to_string())?
            .push(run.clone());
        Ok(run)
    }

    async fn list_workflow_runs(&self, limit: i64) -> Result<Vec<WorkflowRun>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .workflow_runs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .take(limit)
            .cloned()
            .collect())
    }

    async fn list_workflow_step_runs(
        &self,
        _workflow_run_id: &WorkflowRunId,
    ) -> Result<Vec<WorkflowStepRun>, Self::Error> {
        Ok(Vec::new())
    }

    async fn list_runs(&self, limit: i64) -> Result<Vec<JobRun>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .runs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_run(&self, id: &JobRunId) -> Result<Option<JobRun>, Self::Error> {
        Ok(self
            .runs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|run| run.id == *id)
            .cloned())
    }

    async fn find_run_log(&self, id: &JobRunId) -> Result<Option<JobRunLog>, Self::Error> {
        Ok(self
            .logs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|log| log.run_id == *id)
            .cloned())
    }

    async fn cancel_run(&self, id: &JobRunId) -> Result<Option<JobRun>, Self::Error> {
        let mut runs = self.runs.lock().map_err(|error| error.to_string())?;
        let Some(run) = runs.iter_mut().rev().find(|run| run.id == *id) else {
            return Ok(None);
        };
        if !run.status.is_terminal() {
            run.status = JobRunStatus::Cancelled;
        }
        Ok(Some(run.clone()))
    }

    async fn list_artifacts(&self, id: &JobRunId) -> Result<Vec<JobArtifact>, Self::Error> {
        Ok(self
            .artifacts
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|artifact| artifact.run_id == *id)
            .cloned()
            .collect())
    }

    async fn find_artifact(
        &self,
        run_id: &JobRunId,
        artifact_id: &ArtifactId,
    ) -> Result<Option<JobArtifact>, Self::Error> {
        Ok(self
            .artifacts
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|artifact| artifact.run_id == *run_id && artifact.id == *artifact_id)
            .cloned())
    }

    async fn save_artifact(&self, artifact: &JobArtifact) -> Result<(), Self::Error> {
        self.artifacts
            .lock()
            .map_err(|error| error.to_string())?
            .push(artifact.clone());
        Ok(())
    }
}

impl FakeStore {
    fn with_definition(id: &str) -> Self {
        let store = Self::default();
        store
            .known_definitions
            .lock()
            .expect("definition mutex")
            .push(id.to_string());
        store
    }

    fn with_workflow(self, id: &str) -> Self {
        let workflow_id = WorkflowId::new(id).expect("workflow id");
        self.workflows
            .lock()
            .expect("workflows mutex")
            .push(WorkflowDefinition {
                id: workflow_id.clone(),
                name: "Test workflow".to_string(),
                description: String::new(),
                status: WorkflowStatus::Enabled,
                steps: vec![WorkflowStep {
                    id: WorkflowStepId::new(format!("{id}_step_1")).expect("step id"),
                    workflow_id,
                    position: 1,
                    name: "Run job".to_string(),
                    job_definition_id: JobDefinitionId::new("job_hello_python").expect("job id"),
                    execution_pool: ExecutionPoolName::new("mini").expect("pool"),
                }],
            });
        self
    }

    fn with_run(self, run: JobRun) -> Self {
        self.runs.lock().expect("runs mutex").push(run);
        self
    }

    fn with_log(self, log: JobRunLog) -> Self {
        self.logs.lock().expect("logs mutex").push(log);
        self
    }

    fn with_artifact(self, artifact: JobArtifact) -> Self {
        self.artifacts
            .lock()
            .expect("artifacts mutex")
            .push(artifact);
        self
    }
}

fn test_app(store: FakeStore) -> axum::Router {
    let object_store = FakeObjectStore::default();
    object_store.objects.lock().expect("objects mutex").push((
        "artifacts/run_with_artifact/report.txt".to_string(),
        b"report".to_vec(),
    ));
    router(AppState::new(
        store,
        object_store,
        ["mini".to_string(), "large".to_string()],
    ))
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("collect response")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("json response")
}

#[tokio::test]
async fn healthz_returns_ok() {
    let response = test_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(response_json(response).await, json!({ "status": "ok" }));
}

#[tokio::test]
async fn lists_configured_execution_pools() {
    let response = test_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .uri("/v1/execution-pools")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "execution_pools": [
                { "name": "mini", "description": "Default execution pool", "is_default": true, "host_group": "mini" },
                { "name": "large", "description": "Configured execution pool", "is_default": false, "host_group": "large" }
            ]
        })
    );

    let response = test_app(FakeStore::default())
        .oneshot(
            Request::builder()
                .uri("/v1/host-groups")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "host_groups": [
                { "name": "mini", "description": "Default host group", "is_default": true, "execution_pool": "mini", "host_count": null },
                { "name": "large", "description": "Configured host group", "is_default": false, "execution_pool": "large", "host_count": null }
            ]
        })
    );
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
                        "command": ["python", "/plugin/check.py"],
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
    assert_eq!(
        response_json(response).await["trigger_plugins"][0]["runtime_image"],
        "python:3.12-slim"
    );
}

#[tokio::test]
async fn creates_automation_with_trigger_condition_graph() {
    let store = FakeStore::with_definition("job_hello_python").with_workflow("wf_pipeline");
    store
        .trigger_plugins
        .lock()
        .expect("plugins mutex")
        .push(CustomTriggerPlugin {
            id: "plugin_threshold".to_string(),
            name: "Threshold plugin".to_string(),
            description: String::new(),
            runtime_image: "python:3.12-slim".to_string(),
            command: vec!["python".to_string(), "/plugin/check.py".to_string()],
            config_schema_json: "{}".to_string(),
        });

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
                        "trigger_kind": "schedule",
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
    assert_eq!(body["trigger_kind"], "interval");
    assert_eq!(body["interval_seconds"], 300);
    assert_eq!(body["triggers"][2]["kind"], "custom");
    assert_eq!(
        body["condition"]["all"][1]["any"][0]["trigger"],
        "orders_changed"
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
        "bundles/job-definitions/job_daily_report/main.py"
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
            "attempt_count": 0
        })
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
    let log = JobRunLog::new(run.id.clone(), "hello from logs\n").expect("valid log");
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
        run.id.clone(),
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
        run.id.clone(),
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

#[tokio::test]
async fn response_body_helper_handles_empty_body() {
    let bytes = to_bytes(Body::empty(), usize::MAX)
        .await
        .expect("empty body");
    assert!(bytes.is_empty());
}
