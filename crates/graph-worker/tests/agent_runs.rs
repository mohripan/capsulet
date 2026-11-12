//! An agent run from the HTTP request to the worker that executes it.
//!
//! Real router, real `PostgreSQL`, real graph worker. The only thing faked is the
//! model, because what is being tested is everything around it. Each test gets its
//! own tenant, and its workers only lease that tenant's runs, so the suite can share
//! a database without one test advancing another's run.

mod support;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use capsulet_api::{AppState, AuthConfig, router};
use capsulet_application::JobRunRepository;
use capsulet_core::{
    AgentRunId, ClaimId, Entity, EntityId, JobDefinition, JobDefinitionId, JobRunId, MemoryScope,
};
use capsulet_graph_worker::{
    AgentExecutor, Completion, Embedding, Executor, GraphWorker, ModelClient, ModelError,
    ModelRequest, Progress, SystemClock, WorkerConfig,
};
use capsulet_ir::digest::Digest;
use capsulet_observability::testing::{CapturedSpan, FailingExport, TraceCapture};
use capsulet_postgres::PostgresStore;
use capsulet_runtime::bundle::{Rework, RunBundle, SealedRunBundle, explain};
use capsulet_runtime::event::NodeUsage;
use capsulet_runtime::{RunEvent, RunStatus};
use capsulet_storage::FilesystemObjectStore;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

fn unique(prefix: &str) -> String {
    support::fixture_id(prefix)
}

fn database_url() -> String {
    support::required_database_url()
}

/// A model that says what it was told to say and remembers what it was asked.
#[derive(Default)]
struct ScriptedModel {
    answers: Mutex<Vec<String>>,
    prompts: Mutex<Vec<String>>,
    tokens_each: Option<u64>,
}

impl ScriptedModel {
    fn answering(answers: &[&str]) -> Arc<Self> {
        Arc::new(Self {
            answers: Mutex::new(answers.iter().map(|each| (*each).to_string()).collect()),
            prompts: Mutex::new(Vec::new()),
            tokens_each: Some(10),
        })
    }

    fn without_token_counts(answers: &[&str]) -> Arc<Self> {
        Arc::new(Self {
            answers: Mutex::new(answers.iter().map(|each| (*each).to_string()).collect()),
            prompts: Mutex::new(Vec::new()),
            tokens_each: None,
        })
    }

    fn prompts(&self) -> Vec<String> {
        self.prompts.lock().expect("not poisoned").clone()
    }
}

#[async_trait]
impl ModelClient for ScriptedModel {
    fn name(&self) -> String {
        "scripted".to_string()
    }

    async fn complete(&self, request: &ModelRequest) -> Result<Completion, ModelError> {
        self.prompts
            .lock()
            .expect("not poisoned")
            .push(request.prompt.clone());
        let mut answers = self.answers.lock().expect("not poisoned");
        // The last answer repeats, so a test can say "from then on, always this".
        let text = if answers.len() > 1 {
            answers.remove(0)
        } else {
            answers.first().cloned().unwrap_or_default()
        };
        Ok(Completion {
            text,
            usage: NodeUsage {
                provider: Some("scripted".to_string()),
                model: Some(request.model.clone()),
                input_tokens: self.tokens_each,
                output_tokens: None,
                cost_micro_units: None,
                latency_ms: Some(1),
            },
        })
    }

    async fn embed(
        &self,
        _model: &str,
        _text: &str,
        _timeout: Duration,
    ) -> Result<Embedding, ModelError> {
        Err(ModelError::Unreachable("not scripted".to_string()))
    }
}

/// A router and workers, for one tenant.
struct Stack {
    app: Router,
    store: PostgresStore,
    tenant: String,
    project: String,
    model: Option<Arc<ScriptedModel>>,
    token: String,
    foreign_token: String,
    _object_root: ObjectRoot,
}

struct ObjectRoot(std::path::PathBuf);

impl Drop for ObjectRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Stack {
    async fn new(model: Option<Arc<ScriptedModel>>) -> Self {
        let store = PostgresStore::connect(&database_url())
            .await
            .expect("connect to postgres");
        store.migrate().await.expect("run migrations");

        let tenant = unique("tenant");
        let project = unique("project");
        let token = format!("operator-token-{}", unique("secret-0123456789"));
        let foreign_token = format!("operator-token-{}", unique("foreign-0123456789"));
        let auth = AuthConfig::from_json(
            &json!([
                {
                    "name": "author one",
                    "role": "admin",
                    "tenant_id": tenant,
                    "project_id": project,
                    "token": token,
                },
                {
                    "name": "foreign operator",
                    "role": "operator",
                    "tenant_id": tenant,
                    "project_id": unique("foreign_project"),
                    "token": foreign_token,
                },
            ])
            .to_string(),
        )
        .expect("a valid credential set");

        // Outside the database, so named for this process as well as this test.
        let root = std::env::temp_dir().join(format!(
            "capsulet-agent-runs-{}-{}",
            std::process::id(),
            unique("objects")
        ));
        std::fs::create_dir_all(&root).expect("a scratch directory");
        let app = router(
            AppState::new(
                store.clone(),
                FilesystemObjectStore::new(&root),
                ["mini".to_string()],
            )
            .with_auth(auth),
        );
        Self {
            app,
            store,
            tenant,
            project,
            model,
            token,
            foreign_token,
            _object_root: ObjectRoot(root),
        }
    }

    async fn request_as(
        &self,
        token: &str,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("authorization", format!("Bearer {token}"));
        if body.is_some() {
            request = request.header("content-type", "application/json");
        }
        let response = self
            .app
            .clone()
            .oneshot(
                request
                    .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))
                    .expect("a valid request"),
            )
            .await
            .expect("a response");
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes();
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, value)
    }

    async fn post(&self, path: &str, body: Value) -> (StatusCode, Value) {
        self.request_as(&self.token, Method::POST, path, Some(body))
            .await
    }

    async fn get(&self, path: &str) -> (StatusCode, Value) {
        self.request_as(&self.token, Method::GET, path, None).await
    }

    fn worker(&self, steps_per_lease: u32) -> GraphWorker {
        let model = self
            .model
            .clone()
            .map(|model| model as Arc<dyn ModelClient>);
        self.worker_using(model, steps_per_lease)
    }

    fn worker_using(
        &self,
        model: Option<Arc<dyn ModelClient>>,
        steps_per_lease: u32,
    ) -> GraphWorker {
        GraphWorker::new(
            self.store.clone(),
            Arc::new(AgentExecutor::new(self.store.clone(), model)) as Arc<dyn Executor>,
            Arc::new(SystemClock),
            WorkerConfig {
                worker_id: unique("worker"),
                tenant: Some(self.tenant.clone()),
                lease_seconds: 60,
                steps_per_lease,
                ..WorkerConfig::default()
            },
        )
    }

    /// Advances until nothing in this tenant has anything to do.
    async fn run_to_rest(&self) {
        let worker = self.worker(256);
        for _ in 0..64 {
            match worker.advance_one().await.expect("the worker advances") {
                Progress::NothingToDo => return,
                Progress::Ended | Progress::Suspended | Progress::StillRunning => {}
                Progress::LeaseLost => panic!("nobody else leases this tenant's runs"),
            }
        }
        panic!("the runs did not come to rest");
    }

    /// Creates a graph and an agent, and returns the agent's id.
    async fn agent(&self, graph: Value, budget: Value, conditions: &[&str]) -> String {
        let graph_id = unique("graph");
        let mut graph = graph;
        graph["id"] = json!(graph_id);
        graph["name"] = json!("A graph");
        let (status, body) = self.post("/v1/graphs", graph).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");

        let agent_id = unique("agent");
        let (status, body) = self
            .post(
                "/v1/agents",
                json!({
                    "id": agent_id,
                    "name": "An agent",
                    "graph_id": graph_id,
                    "budget": budget,
                    "termination_conditions": conditions,
                }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        agent_id
    }

    async fn start(&self, agent: &str, state: Value) -> String {
        let run_id = unique("run");
        let (status, body) = self
            .post(
                &format!("/v1/agents/{agent}/runs"),
                json!({ "id": run_id, "initial_state": state }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(body["status"], "queued");
        run_id
    }

    async fn run(&self, id: &str) -> Value {
        let (status, body) = self.get(&format!("/v1/agent-runs/{id}")).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    async fn event_kinds(&self, id: &str) -> Vec<String> {
        let (status, body) = self.get(&format!("/v1/agent-runs/{id}/events")).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["events"]
            .as_array()
            .expect("a log")
            .iter()
            .map(|event| event["kind"].as_str().expect("a kind").to_string())
            .collect()
    }
}

fn generous() -> Value {
    json!({ "max_steps": 5, "max_tokens": 10_000, "max_seconds": 300, "max_cost_micros": 1_000 })
}

fn port(id: &str, direction: &str, value_type: &str) -> Value {
    json!({ "id": id, "direction": direction, "value_type": value_type })
}

fn at(node: &str, port_id: &str, value_type: &str) -> Value {
    json!({ "kind": "port", "node_id": node, "port_id": port_id, "value_type": value_type })
}

fn field(name: &str, value_type: &str) -> Value {
    json!({ "kind": "state_field", "field": name, "value_type": value_type })
}

fn edge(id: &str, source: &Value, target: &Value) -> Value {
    json!({ "id": id, "sources": [source], "targets": [target] })
}

/// `query -> normalize -> prompt -> model -> check`, with the answer written to state
/// and read back on the next time round.
fn answering_graph(check_config: &Value) -> Value {
    json!({
        "nodes": [
            {
                "id": "normalize",
                "name": "Normalize",
                "kind": "query_normalizer",
                "ports": [
                    port("in", "input", "user_query"),
                    port("out", "output", "normalized_query"),
                ],
            },
            {
                "id": "prompt",
                "name": "Prompt",
                "kind": "prompt_builder",
                "ports": [
                    port("query", "input", "normalized_query"),
                    port("last", "input", "model_response"),
                    port("text", "output", "prompt"),
                ],
                "config": { "template": "Q: {{query}} Last: {{last}}" },
            },
            {
                "id": "model",
                "name": "Model",
                "kind": "llm",
                "ports": [
                    port("prompt", "input", "prompt"),
                    port("answer", "output", "model_response"),
                ],
                "config": { "model": "small-model" },
            },
            {
                "id": "check",
                "name": "Check",
                "kind": "validator",
                "ports": [
                    port("answer", "input", "model_response"),
                    port("result", "output", "validation_result"),
                ],
                "config": check_config,
            },
        ],
        "hyperedges": [
            edge("q-in", &field("query", "user_query"), &at("normalize", "in", "user_query")),
            edge("q-prompt", &at("normalize", "out", "normalized_query"), &at("prompt", "query", "normalized_query")),
            edge("last-prompt", &field("answer", "model_response"), &at("prompt", "last", "model_response")),
            edge("p-model", &at("prompt", "text", "prompt"), &at("model", "prompt", "prompt")),
            edge("m-check", &at("model", "answer", "model_response"), &at("check", "answer", "model_response")),
            edge("m-state", &at("model", "answer", "model_response"), &field("answer", "model_response")),
        ],
        "transition_policy": { "mode": "static", "cycles_allowed": false },
    })
}

#[tokio::test]
async fn a_run_started_over_http_is_executed_by_the_worker() {
    let model = ScriptedModel::answering(&["Capsulet checks things."]);
    let stack = Stack::new(Some(Arc::clone(&model))).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.non_empty": "true" })),
            generous(),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(
            &agent,
            json!({ "query": "  what is\n capsulet? ", "answer": "" }),
        )
        .await;

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(run["state"]["answer"], "Capsulet checks things.");
    assert_eq!(
        run["state"]["query"], "  what is\n capsulet? ",
        "what the run was given stays what it was given"
    );
    assert_eq!(
        run["assurance"], "unverified",
        "finishing is not the same as being right"
    );
    assert_eq!(run["execution"]["usage"]["tokens"], 10);
    assert_eq!(run["execution"]["ir_run_id"], id);
    assert!(run["state_version"].as_u64().expect("a count") >= 4);

    assert_eq!(
        model.prompts(),
        vec!["Q: what is capsulet? Last: ".to_string()],
        "the normalized query reached the model through the template"
    );

    let kinds = stack.event_kinds(&id).await;
    assert_eq!(kinds[0], "admitted");
    assert_eq!(kinds[1], "inputs_bound");
    assert!(kinds.contains(&"resources_consumed".to_string()));
    assert_eq!(kinds.last().map(String::as_str), Some("completed"));
}

#[tokio::test]
async fn a_failed_check_goes_round_again_carrying_what_the_last_attempt_left() {
    let model = ScriptedModel::answering(&["no sources", "as cited above"]);
    let stack = Stack::new(Some(Arc::clone(&model))).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.contains": "cited" })),
            generous(),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(run["state"]["answer"], "as cited above");
    assert_eq!(run["execution"]["usage"]["tokens"], 20);
    assert_eq!(
        model.prompts(),
        vec![
            "Q: why? Last: ".to_string(),
            "Q: why? Last: no sources".to_string(),
        ],
        "a retry has to be able to see its own last attempt"
    );
}

#[tokio::test]
async fn a_failed_check_with_no_retry_policy_fails_the_run_and_says_why() {
    let model = ScriptedModel::answering(&["no sources"]);
    let stack = Stack::new(Some(model)).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.contains": "cited" })),
            generous(),
            &["safety_failure"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "failed", "{run}");
    let detail = run["execution"]["detail"].as_str().expect("a reason");
    assert!(detail.contains("cited"), "{detail}");
}

#[tokio::test]
async fn running_out_of_iterations_stops_a_run_rather_than_succeeding_it() {
    let model = ScriptedModel::answering(&["one", "two", "three", "four"]);
    let stack = Stack::new(Some(Arc::clone(&model))).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.contains": "never" })),
            json!({ "max_steps": 2, "max_tokens": 10_000, "max_seconds": 300, "max_cost_micros": 1_000 }),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "stopped", "{run}");
    assert!(
        run["execution"]["detail"]
            .as_str()
            .expect("a reason")
            .contains("iterations"),
        "{run}"
    );
    assert_eq!(model.prompts().len(), 2);
}

#[tokio::test]
async fn running_out_of_tokens_stops_a_run_part_way_through() {
    let model = ScriptedModel::answering(&["no sources"]);
    let stack = Stack::new(Some(Arc::clone(&model))).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.contains": "never" })),
            json!({ "max_steps": 20, "max_tokens": 10, "max_seconds": 300, "max_cost_micros": 1_000 }),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "stopped", "{run}");
    assert!(
        run["execution"]["detail"]
            .as_str()
            .expect("a reason")
            .contains("tokens"),
        "{run}"
    );
    assert_eq!(model.prompts().len(), 1);
}

#[tokio::test]
async fn a_model_that_reports_no_usage_is_counted_as_unknown() {
    let model = ScriptedModel::without_token_counts(&["an answer"]);
    let stack = Stack::new(Some(model)).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.non_empty": "true" })),
            generous(),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(run["execution"]["usage"]["tokens"], 0);
    assert_eq!(
        run["execution"]["usage"]["unmeasured"], 1,
        "zero tokens and nobody-counted-them must not read the same"
    );
}

#[tokio::test]
async fn a_model_repeating_itself_is_stopped_for_going_nowhere() {
    let model = ScriptedModel::answering(&["the same wrong answer"]);
    let stack = Stack::new(Some(Arc::clone(&model))).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.contains": "never" })),
            json!({ "max_steps": 20, "max_tokens": 10_000, "max_seconds": 300, "max_cost_micros": 1_000 }),
            &["validator_pass", "no_progress"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "stopped", "{run}");
    assert!(
        run["execution"]["detail"]
            .as_str()
            .expect("a reason")
            .contains("non_progress"),
        "{run}"
    );
    assert!(
        model.prompts().len() < 20,
        "non-progress has to stop it before the budget does"
    );
}

#[tokio::test]
async fn with_no_model_configured_a_run_fails_and_names_the_fix() {
    let stack = Stack::new(None).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.non_empty": "true" })),
            generous(),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "failed", "{run}");
    assert!(
        run["execution"]["detail"]
            .as_str()
            .expect("a reason")
            .contains("CAPSULET_OLLAMA_URL"),
        "{run}"
    );
}

#[tokio::test]
async fn a_safety_check_stops_the_run() {
    let model = ScriptedModel::answering(&["rm -rf /"]);
    let stack = Stack::new(Some(model)).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.not_contains": "rm -rf", "on_fail": "safety" })),
            generous(),
            &["validator_pass", "safety_failure"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "stopped", "{run}");
    assert!(
        run["execution"]["detail"]
            .as_str()
            .expect("a reason")
            .contains("unsafe_effect"),
        "{run}"
    );
}

async fn escalating_run(stack: &Stack) -> String {
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.contains": "cited", "on_fail": "escalate" })),
            generous(),
            &["validator_pass", "human_escalation"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;
    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "waiting", "{run}");
    assert!(
        run["execution"]["waiting_on"]
            .as_str()
            .expect("what it waits on")
            .contains("human-reviewer"),
        "{run}"
    );
    id
}

#[tokio::test]
async fn approving_at_a_gate_waives_the_failure_and_the_run_finishes() {
    let model = ScriptedModel::answering(&["no sources but fine"]);
    let stack = Stack::new(Some(model)).await;
    let id = escalating_run(&stack).await;

    let (status, body) = stack
        .post(
            &format!("/v1/agent-runs/{id}/decisions"),
            json!({ "approved": true }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "succeeded", "{run}");
    assert!(
        stack
            .event_kinds(&id)
            .await
            .contains(&"resumed".to_string())
    );
}

#[tokio::test]
async fn refusing_at_a_gate_stops_the_run() {
    let model = ScriptedModel::answering(&["no sources and wrong"]);
    let stack = Stack::new(Some(model)).await;
    let id = escalating_run(&stack).await;

    let (status, body) = stack
        .post(
            &format!("/v1/agent-runs/{id}/decisions"),
            json!({ "approved": false }),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "stopped", "{run}");
    assert!(
        run["execution"]["detail"]
            .as_str()
            .expect("a reason")
            .contains("refused"),
        "{run}"
    );
}

#[tokio::test]
async fn deciding_on_a_run_that_is_not_waiting_is_a_conflict() {
    let model = ScriptedModel::answering(&["fine"]);
    let stack = Stack::new(Some(model)).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.non_empty": "true" })),
            generous(),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;

    let (status, body) = stack
        .post(
            &format!("/v1/agent-runs/{id}/decisions"),
            json!({ "approved": true }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "agent_run_state_conflict");
}

#[tokio::test]
async fn cancelling_stops_a_run_and_a_finished_run_cannot_be_cancelled() {
    let model = ScriptedModel::answering(&["never asked"]);
    let stack = Stack::new(Some(Arc::clone(&model))).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.non_empty": "true" })),
            generous(),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;

    let (status, body) = stack
        .post(&format!("/v1/agent-runs/{id}/cancel"), json!({}))
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "cancelled", "{run}");
    assert!(
        model.prompts().is_empty(),
        "a run cancelled before it started must not spend anything"
    );

    let (status, body) = stack
        .post(&format!("/v1/agent-runs/{id}/cancel"), json!({}))
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
}

#[tokio::test]
async fn a_worker_stopping_after_every_step_changes_nothing() {
    let model = ScriptedModel::answering(&["no sources", "as cited above"]);
    let stack = Stack::new(Some(Arc::clone(&model))).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.contains": "cited" })),
            generous(),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;

    // Every step on a worker that has never seen this run, so nothing survives
    // between steps except what the log and the value store hold.
    for _ in 0..512 {
        match stack
            .worker(1)
            .advance_one()
            .await
            .expect("the worker advances")
        {
            // One step per lease means the step that finishes the run is not the pass
            // that notices, so the next worker finds nothing left to lease.
            Progress::Ended | Progress::NothingToDo => break,
            Progress::Suspended | Progress::StillRunning | Progress::LeaseLost => {}
        }
    }

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(run["state"]["answer"], "as cited above");
    assert_eq!(
        model.prompts().len(),
        2,
        "two iterations, each asking once, however many workers it took"
    );
}

#[tokio::test]
async fn a_bad_start_gets_an_answer_now_rather_than_a_run_that_fails_later() {
    let stack = Stack::new(None).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.non_empty": "true" })),
            generous(),
            &["validator_pass"],
        )
        .await;

    let (status, body) = stack
        .post(
            &format!("/v1/agents/{agent}/runs"),
            json!({ "initial_state": { "answer": "" } }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .expect("a message")
            .contains("`query`"),
        "{body}"
    );

    let (status, body) = stack
        .post(
            &format!("/v1/agents/{agent}/runs"),
            json!({ "initial_state": { "query": 42, "answer": "" } }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    let (status, body) = stack
        .post(
            &format!("/v1/agents/{agent}/runs"),
            json!({ "initial_state": [] }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn a_planner_agent_is_refused_at_the_start_by_name() {
    let stack = Stack::new(None).await;
    let mut graph = answering_graph(&json!({ "rule.non_empty": "true" }));
    graph["transition_policy"] = json!({ "mode": "planner", "actions": ["model"] });
    let agent = stack.agent(graph, generous(), &["validator_pass"]).await;

    let (status, body) = stack
        .post(
            &format!("/v1/agents/{agent}/runs"),
            json!({ "initial_state": { "query": "why?", "answer": "" } }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "agent_not_runnable");
    assert!(
        body["message"]
            .as_str()
            .expect("a message")
            .contains("planner"),
        "{body}"
    );
}

#[tokio::test]
async fn another_project_can_see_none_of_it() {
    let model = ScriptedModel::answering(&["a private answer"]);
    let stack = Stack::new(Some(model)).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.non_empty": "true" })),
            generous(),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;
    stack.run_to_rest().await;

    for path in [
        format!("/v1/agent-runs/{id}"),
        format!("/v1/agent-runs/{id}/events"),
    ] {
        let (status, _) = stack
            .request_as(&stack.foreign_token, Method::GET, &path, None)
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }
    for path in [
        format!("/v1/agent-runs/{id}/cancel"),
        format!("/v1/agents/{agent}/runs"),
    ] {
        let (status, _) = stack
            .request_as(
                &stack.foreign_token,
                Method::POST,
                &path,
                Some(json!({ "initial_state": { "query": "x", "answer": "" } })),
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }

    let (_, listed) = stack
        .request_as(&stack.foreign_token, Method::GET, "/v1/agent-runs", None)
        .await;
    assert_eq!(listed["agent_runs"], json!([]));

    // Holding a digest is not a way in either.
    let answer = serde_json::to_string("a private answer").expect("encodes");
    let digest = Digest::of(answer.as_bytes());
    assert!(
        stack
            .store
            .get_ir_run_value(&stack.tenant, &stack.project, &digest)
            .await
            .expect("reads")
            .is_some()
    );
    assert!(
        stack
            .store
            .get_ir_run_value(&stack.tenant, "some-other-project", &digest)
            .await
            .expect("reads")
            .is_none()
    );
}

fn job_graph(job: &str) -> Value {
    json!({
        "nodes": [{
            "id": "job",
            "name": "Job",
            "kind": "job",
            "ports": [port("result", "output", "json")],
            "config": { "job_definition_id": job, "execution_pool": "mini" },
        }],
        "hyperedges": [
            edge("job-out", &at("job", "result", "json"), &field("result", "json")),
        ],
        "transition_policy": { "mode": "static", "cycles_allowed": false },
    })
}

async fn job_definition(stack: &Stack) -> String {
    let id = unique("job");
    let definition = JobDefinition::new(
        JobDefinitionId::new(id.clone()).expect("a valid id"),
        "A job",
        "python:3.12-slim",
        vec!["python".to_string()],
        Vec::new(),
        "job.py",
        "{}",
        capsulet_core::RetryPolicy::no_retry(),
    )
    .expect("a valid definition");
    stack
        .store
        .upsert_job_definition_in_project(&definition, &stack.tenant, &stack.project)
        .await
        .expect("stored");
    id
}

/// The idempotency key the run's log says was presented for its effect.
///
/// Read from the log rather than derived again, because the log is what a retry reads.
async fn claimed_key(
    store: &PostgresStore,
    tenant: &str,
    project: &str,
    run: &str,
) -> Option<String> {
    store
        .load_ir_run_events(tenant, project, run)
        .await
        .expect("reads")
        .into_iter()
        .find_map(|recorded| match recorded.event {
            RunEvent::EffectClaimed { key, .. } => key,
            _ => None,
        })
}

/// The id an effect derives from its key.
fn derived_id(prefix: &str, key: &str) -> String {
    let hex = Digest::of(key.as_bytes()).to_string();
    format!(
        "{prefix}{}",
        &hex.rsplit(':').next().expect("a digest")[..32]
    )
}

/// Waits for the job run a node created, then finishes it the way a worker would.
async fn finish_job_when_it_appears(
    store: PostgresStore,
    tenant: String,
    project: String,
    run: String,
    status: &'static str,
) {
    for _ in 0..400 {
        if let Some(key) = claimed_key(&store, &tenant, &project, &run).await {
            let id = JobRunId::new(derived_id("agent_job_", &key)).expect("a valid id");
            if store.find_by_id(&id).await.expect("reads").is_some() {
                // Standing in for the job worker, which is not what this test is about.
                let pool = sqlx::PgPool::connect(&database_url())
                    .await
                    .expect("connects");
                sqlx::query("UPDATE job_runs SET status = $2 WHERE id = $1")
                    .bind(id.as_str())
                    .bind(status)
                    .execute(&pool)
                    .await
                    .expect("updates");
                pool.close().await;
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the job run never appeared");
}

#[tokio::test]
async fn a_job_node_runs_its_job_once_and_reports_how_it_came_out() {
    let stack = Stack::new(None).await;
    let job = job_definition(&stack).await;
    let agent = stack
        .agent(job_graph(&job), generous(), &["validator_pass"])
        .await;
    let id = stack.start(&agent, json!({})).await;

    let waiter = tokio::spawn(finish_job_when_it_appears(
        stack.store.clone(),
        stack.tenant.clone(),
        stack.project.clone(),
        id.clone(),
        "succeeded",
    ));
    stack.run_to_rest().await;
    waiter.await.expect("the job finished");

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(run["state"]["result"]["status"], "succeeded");
    assert_eq!(run["execution"]["usage"]["effects"], 1);

    let kinds = stack.event_kinds(&id).await;
    let claimed = kinds
        .iter()
        .filter(|kind| *kind == "effect_claimed")
        .count();
    let finalized = kinds
        .iter()
        .filter(|kind| *kind == "effect_finalized")
        .count();
    assert_eq!((claimed, finalized), (1, 1));
}

#[tokio::test]
async fn a_job_that_fails_fails_the_node_with_the_effect_still_on_record() {
    let stack = Stack::new(None).await;
    let job = job_definition(&stack).await;
    let agent = stack
        .agent(job_graph(&job), generous(), &["validator_pass"])
        .await;
    let id = stack.start(&agent, json!({})).await;

    let waiter = tokio::spawn(finish_job_when_it_appears(
        stack.store.clone(),
        stack.tenant.clone(),
        stack.project.clone(),
        id.clone(),
        "failed",
    ));
    stack.run_to_rest().await;
    waiter.await.expect("the job finished");

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "failed", "{run}");
    let kinds = stack.event_kinds(&id).await;
    assert!(
        kinds.contains(&"effect_finalized".to_string()),
        "the job ran, so its effect is not abandoned: {kinds:?}"
    );
}

#[tokio::test]
async fn a_job_in_another_project_is_refused_rather_than_run() {
    let stack = Stack::new(None).await;
    let foreign = unique("foreign_job");
    let definition = JobDefinition::new(
        JobDefinitionId::new(foreign.clone()).expect("a valid id"),
        "Not yours",
        "python:3.12-slim",
        vec!["python".to_string()],
        Vec::new(),
        "job.py",
        "{}",
        capsulet_core::RetryPolicy::no_retry(),
    )
    .expect("a valid definition");
    stack
        .store
        .upsert_job_definition_in_project(&definition, &stack.tenant, "another-project")
        .await
        .expect("stored");

    let agent = stack
        .agent(job_graph(&foreign), generous(), &["validator_pass"])
        .await;
    let id = stack.start(&agent, json!({})).await;
    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "failed", "{run}");
    assert!(
        run["execution"]["detail"]
            .as_str()
            .expect("a reason")
            .contains("policy_denial"),
        "{run}"
    );
    let key = claimed_key(&stack.store, &stack.tenant, &stack.project, &id)
        .await
        .expect("the attempt was claimed");
    assert!(
        stack
            .store
            .find_by_id(&JobRunId::new(derived_id("agent_job_", &key)).expect("a valid id"))
            .await
            .expect("reads")
            .is_none(),
        "no job run may exist for a definition this project does not own"
    );
}

fn memory_write_graph() -> Value {
    json!({
        "nodes": [{
            "id": "write",
            "name": "Write",
            "kind": "memory_write",
            "ports": [port("claim", "input", "json")],
        }],
        "hyperedges": [
            edge("claim-in", &field("claim", "json"), &at("write", "claim", "json")),
        ],
        "transition_policy": { "mode": "static", "cycles_allowed": false },
    })
}

#[tokio::test]
async fn an_agent_can_only_propose_a_claim_never_activate_one() {
    let stack = Stack::new(None).await;
    let subject = unique("acme");
    stack
        .store
        .upsert_memory_entity(
            &Entity::new(
                EntityId::new(subject.clone()).expect("a valid id"),
                MemoryScope::new(stack.tenant.clone(), stack.project.clone()).expect("a scope"),
                "organization",
                "Acme",
                Vec::new(),
            )
            .expect("a valid entity"),
        )
        .await
        .expect("stored");
    let agent = stack
        .agent(memory_write_graph(), generous(), &["validator_pass"])
        .await;
    let id = stack
        .start(
            &agent,
            json!({ "claim": {
                "subject_id": subject,
                "predicate": "renewed",
                "object": "2026-03-01",
                "evidence_ids": ["evidence-1"],
                "observed_at": "2026-11-08T00:00:00Z",
                "status": "active",
            }}),
        )
        .await;

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "succeeded", "{run}");

    let key = claimed_key(&stack.store, &stack.tenant, &stack.project, &id)
        .await
        .expect("the attempt was claimed");
    let claim_id = ClaimId::new(derived_id("agent_claim_", &key)).expect("a valid id");
    let claim = stack
        .store
        .find_memory_claim(&claim_id)
        .await
        .expect("reads")
        .expect("the claim was proposed");
    assert_eq!(
        claim.status().to_string(),
        "candidate",
        "an agent's output is a candidate until somebody reviews it"
    );
    assert_eq!(claim.scope().project_id(), stack.project);
}

#[tokio::test]
async fn a_claim_that_cannot_be_built_is_refused_and_writes_nothing() {
    let stack = Stack::new(None).await;
    let agent = stack
        .agent(memory_write_graph(), generous(), &["validator_pass"])
        .await;
    let id = stack
        .start(&agent, json!({ "claim": { "predicate": "renewed" } }))
        .await;

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "failed", "{run}");
    assert!(
        run["execution"]["detail"]
            .as_str()
            .expect("a reason")
            .contains("a claim to write needs"),
        "{run}"
    );
    let kinds = stack.event_kinds(&id).await;
    assert!(kinds.contains(&"effect_abandoned".to_string()), "{kinds:?}");
}

#[tokio::test]
async fn the_definition_a_run_executed_stays_readable_after_the_agent_changes() {
    let model = ScriptedModel::answering(&["an answer"]);
    let stack = Stack::new(Some(model)).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.non_empty": "true" })),
            generous(),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;
    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    let digest = run["execution"]["definition_digest"]
        .as_str()
        .expect("a digest");
    let (status, body) = stack.get(&format!("/v1/ir/definitions/{digest}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["digest"], digest);
    let bytes = body["canonical_bytes"].as_str().expect("the bytes");
    assert!(
        bytes.contains("Q: {{query}} Last: {{last}}"),
        "the prompt is part of the version"
    );
}

// ---- telemetry
//
// Each of these installs a subscriber on its own thread, and a tokio test runs on one
// thread, so what a test captures is exactly what its own run produced.

/// A model that refuses, with an error body that repeats what it was asked, the way
/// some providers' error pages do.
struct RefusingModel;

#[async_trait]
impl ModelClient for RefusingModel {
    fn name(&self) -> String {
        "refusing".to_string()
    }

    async fn complete(&self, request: &ModelRequest) -> Result<Completion, ModelError> {
        Err(ModelError::Rejected {
            status: 503,
            body: format!("overloaded while answering: {}", request.prompt),
        })
    }

    async fn embed(
        &self,
        _model: &str,
        _text: &str,
        _timeout: Duration,
    ) -> Result<Embedding, ModelError> {
        Err(ModelError::Unreachable("not scripted".to_string()))
    }
}

fn leases_of<'a>(spans: &'a [CapturedSpan], run: &str) -> Vec<&'a CapturedSpan> {
    spans
        .iter()
        .filter(|span| {
            span.name == "capsulet.lease" && span.attribute("capsulet.run_id") == Some(run)
        })
        .collect()
}

fn children_of<'a>(spans: &'a [CapturedSpan], parents: &[&CapturedSpan]) -> Vec<&'a CapturedSpan> {
    let ids: BTreeSet<&str> = parents.iter().map(|span| span.span_id.as_str()).collect();
    spans
        .iter()
        .filter(|span| {
            span.parent_span_id
                .as_deref()
                .is_some_and(|parent| ids.contains(parent))
        })
        .collect()
}

fn assert_nothing_said(spans: &[CapturedSpan], content: &[&str]) {
    for span in spans {
        let rendered = span.rendered();
        for each in content {
            assert!(
                !rendered.contains(each),
                "`{each}` reached telemetry on `{}`: {rendered}",
                span.name
            );
        }
    }
}

#[tokio::test]
async fn a_run_is_one_trace_from_its_request_through_every_lease_on_every_worker() {
    const QUERY: &str = "which-moon-7731?";
    const FIRST: &str = "no sources 4410";
    const SECOND: &str = "as cited above 5521";
    let capture = TraceCapture::start();
    let model = ScriptedModel::answering(&[FIRST, SECOND]);
    let stack = Stack::new(Some(Arc::clone(&model))).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.contains": "cited" })),
            generous(),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": QUERY, "answer": "" }))
        .await;

    // Two workers taking turns a step at a time, so the run is carried by many
    // leases held by more than one worker.
    let workers = [stack.worker(1), stack.worker(1)];
    let mut turns = 0;
    while workers[turns % 2]
        .advance_one()
        .await
        .expect("the worker advances")
        != Progress::NothingToDo
    {
        turns += 1;
        assert!(turns < 256, "the run did not come to rest");
    }
    let run = stack.run(&id).await;
    assert_eq!(run["status"], "succeeded", "{run}");

    let spans = capture.spans();
    let path = format!("/v1/agents/{agent}/runs");
    let start = spans
        .iter()
        .find(|span| {
            span.name == "http.request"
                && span.attribute("http.method") == Some("POST")
                && span.attribute("http.route") == Some("/v1/agents/{id}/runs")
                && span.attribute("url.path") == Some(path.as_str())
        })
        .expect("the request that started the run was traced");

    let leases = assert_leases_continue(&spans, start, &stack, &id);
    assert_attempts_and_calls(&spans, start, &leases);
    assert_nothing_said(&spans, &[QUERY, FIRST, SECOND, "Last:"]);
}

/// Every lease of the run joins the starting request's trace, and both workers held one.
fn assert_leases_continue<'a>(
    spans: &'a [CapturedSpan],
    start: &CapturedSpan,
    stack: &Stack,
    id: &str,
) -> Vec<&'a CapturedSpan> {
    let leases = leases_of(spans, id);
    assert!(leases.len() > 2, "{} leases", leases.len());
    for lease in &leases {
        assert_eq!(
            lease.trace_id, start.trace_id,
            "every lease continues the trace of the request that started the run"
        );
        assert!(lease.parent_span_id.is_some());
        assert_eq!(
            lease.attribute("capsulet.tenant_id"),
            Some(stack.tenant.as_str())
        );
        assert_eq!(
            lease.attribute("capsulet.project_id"),
            Some(stack.project.as_str())
        );
        assert!(lease.attribute("capsulet.definition_digest").is_some());
        assert!(lease.attribute("capsulet.lease.epoch").is_some());
        assert!(lease.error.is_none(), "{lease:?}");
    }
    let holders: BTreeSet<&str> = leases
        .iter()
        .filter_map(|lease| lease.attribute("capsulet.worker_id"))
        .collect();
    assert_eq!(
        holders.len(),
        2,
        "both workers carried the run: {holders:?}"
    );
    // A lease records where the run stood at its last fold. The newest one wrote
    // the completion, so by its fold both model calls had been paid for.
    let last = leases
        .iter()
        .max_by_key(|lease| {
            lease
                .attribute("capsulet.lease.epoch")
                .and_then(|epoch| epoch.parse::<u64>().ok())
        })
        .expect("a lease");
    assert_eq!(
        last.attribute("capsulet.run.spent.tokens"),
        Some("20"),
        "what the run spent, as its log says"
    );
    assert_eq!(last.attribute("capsulet.run.spent.unmeasured"), Some("0"));
    leases
}

/// Each node attempt sits under a lease, and each model call under its attempt.
fn assert_attempts_and_calls(
    spans: &[CapturedSpan],
    start: &CapturedSpan,
    leases: &[&CapturedSpan],
) {
    let nodes = children_of(spans, leases);
    let attempts: Vec<&&CapturedSpan> = nodes
        .iter()
        .filter(|node| node.attribute("capsulet.node.kind") == Some("llm"))
        .collect();
    assert_eq!(attempts.len(), 2, "one attempt each time round the loop");
    let iterations: BTreeSet<&str> = attempts
        .iter()
        .filter_map(|node| node.attribute("capsulet.loop.iteration"))
        .collect();
    assert_eq!(
        iterations.len(),
        2,
        "each attempt says which time round it was"
    );
    for attempt in &attempts {
        assert_eq!(attempt.attribute("capsulet.usage.tokens"), Some("10"));
        assert_eq!(
            attempt.attribute("capsulet.usage.unmeasured"),
            Some("false")
        );
    }
    for node in &nodes {
        assert!(node.name.starts_with("capsulet.node "), "{}", node.name);
        assert_eq!(node.trace_id, start.trace_id);
        assert_eq!(node.attribute("capsulet.node.outcome"), Some("finished"));
        let started: u64 = node
            .attribute("capsulet.event.started_position")
            .and_then(|position| position.parse().ok())
            .expect("the position of the event that opened the attempt");
        let finished: u64 = node
            .attribute("capsulet.event.finished_position")
            .and_then(|position| position.parse().ok())
            .expect("the position of the event that closed it");
        assert!(started < finished);
    }

    let calls = children_of(
        spans,
        &attempts.iter().map(|each| **each).collect::<Vec<_>>(),
    );
    assert_eq!(calls.len(), 2, "each attempt made one model call");
    for call in &calls {
        assert_eq!(call.name, "text_completion small-model");
        assert_eq!(call.kind, "client");
        assert_eq!(
            call.attribute("gen_ai.operation.name"),
            Some("text_completion")
        );
        assert_eq!(call.attribute("gen_ai.provider.name"), Some("scripted"));
        assert_eq!(call.attribute("gen_ai.request.model"), Some("small-model"));
        assert_eq!(call.attribute("gen_ai.usage.input_tokens"), Some("10"));
        assert_eq!(
            call.attribute("gen_ai.usage.output_tokens"),
            None,
            "a count nobody reported is left off, not written down as zero"
        );
        assert!(call.error.is_none());
    }
}

#[tokio::test]
async fn a_refused_model_call_is_an_error_that_names_its_kind_and_not_its_content() {
    const QUERY: &str = "private-question-8812";
    let capture = TraceCapture::start();
    let stack = Stack::new(None).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.non_empty": "true" })),
            generous(),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": QUERY, "answer": "" }))
        .await;
    let worker = stack.worker_using(Some(Arc::new(RefusingModel)), 256);
    for _ in 0..64 {
        if worker.advance_one().await.expect("the worker advances") == Progress::NothingToDo {
            break;
        }
    }

    let spans = capture.spans();
    let call = spans
        .iter()
        .find(|span| span.attribute("gen_ai.operation.name") == Some("text_completion"))
        .expect("the model call was traced");
    assert_eq!(call.error.as_deref(), Some("refused with status 503"));
    assert_eq!(call.attribute("error.type"), Some("rejected"));
    let node = spans
        .iter()
        .find(|span| Some(span.span_id.as_str()) == call.parent_span_id.as_deref())
        .expect("the call sits under its node attempt");
    assert_eq!(node.attribute("capsulet.node.outcome"), Some("failed"));
    assert_eq!(node.attribute("error.type"), Some("verifier_unavailable"));
    assert_eq!(node.error.as_deref(), Some("verifier_unavailable"));

    assert_nothing_said(&spans, &[QUERY, "overloaded while answering"]);

    // Nor does the run's log, which an erasure cannot rewrite.
    let (_, events) = stack.get(&format!("/v1/agent-runs/{id}/events")).await;
    let log = events.to_string();
    assert!(log.contains("refused with status 503"), "{log}");
    assert!(!log.contains("overloaded while answering"), "{log}");
    assert!(!log.contains(QUERY), "{log}");
}

#[tokio::test]
async fn an_effect_is_a_tool_span_and_a_refused_one_says_why_only_by_kind() {
    const PREDICATE: &str = "renewed-unique-6630";
    let capture = TraceCapture::start();
    let stack = Stack::new(None).await;
    let agent = stack
        .agent(memory_write_graph(), generous(), &["validator_pass"])
        .await;
    let id = stack
        .start(&agent, json!({ "claim": { "predicate": PREDICATE } }))
        .await;
    stack.run_to_rest().await;
    let run = stack.run(&id).await;
    assert_eq!(run["status"], "failed", "{run}");

    let spans = capture.spans();
    let leases = leases_of(&spans, &id);
    let tool = children_of(&spans, &leases)
        .into_iter()
        .find(|span| span.attribute("gen_ai.operation.name") == Some("execute_tool"))
        .expect("the effect was traced under its lease");
    assert!(tool.name.starts_with("execute_tool "), "{}", tool.name);
    assert_eq!(tool.kind, "internal");
    assert_eq!(tool.attribute("gen_ai.tool.type"), Some("datastore"));
    assert_eq!(tool.attribute("capsulet.node.kind"), Some("memory_write"));
    assert_eq!(tool.attribute("capsulet.effect.outcome"), Some("failed"));
    assert_eq!(tool.attribute("capsulet.effect.recovered"), Some("false"));
    assert!(tool.attribute("capsulet.effect.attempt").is_some());
    assert!(tool.attribute("capsulet.event.claimed_position").is_some());
    let kind = tool.attribute("error.type").expect("a failure kind");
    assert_eq!(tool.error.as_deref(), Some(kind));
    assert!(
        kind.chars()
            .all(|each| each.is_ascii_lowercase() || each == '_'),
        "a kind from a closed set, not a message: {kind}"
    );

    assert_nothing_said(&spans, &[PREDICATE, "a claim to write needs"]);
}

/// Runs the retrying agent to rest and says how it ended.
async fn retrying_run_outcome() -> (Value, Value, Value, Vec<String>) {
    let model = ScriptedModel::answering(&["no sources", "as cited above"]);
    let stack = Stack::new(Some(model)).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.contains": "cited" })),
            generous(),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;
    stack.run_to_rest().await;
    let run = stack.run(&id).await;
    let kinds = stack.event_kinds(&id).await;
    (
        run["status"].clone(),
        run["state"].clone(),
        run["execution"]["usage"].clone(),
        kinds,
    )
}

#[tokio::test]
async fn a_collector_that_is_down_changes_nothing_about_how_a_run_ends() {
    let untraced = retrying_run_outcome().await;

    let collector_down = FailingExport::start();
    let traced = retrying_run_outcome().await;

    assert!(
        collector_down.attempts() > 0,
        "spans were exported, and every export was refused"
    );
    assert_eq!(untraced.0, "succeeded");
    assert_eq!(
        traced, untraced,
        "telemetry failing is not something a run can notice"
    );
}

#[tokio::test]
async fn the_metrics_a_run_records_have_labels_from_closed_sets() {
    // Another test in this binary may have installed it already, which is as good.
    let _ = capsulet_observability::testing::install_metrics();
    let model = ScriptedModel::answering(&["an answer"]);
    let stack = Stack::new(Some(model)).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.non_empty": "true" })),
            generous(),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;
    stack.run_to_rest().await;
    assert_eq!(stack.run(&id).await["status"], "succeeded");

    let model_labels: &[&str] = &[
        "gen_ai_operation_name",
        "gen_ai_provider_name",
        "gen_ai_request_model",
        "error_type",
        "quantile",
    ];
    let token_labels: &[&str] = &[
        "gen_ai_operation_name",
        "gen_ai_provider_name",
        "gen_ai_request_model",
        "gen_ai_token_type",
        "quantile",
    ];
    let closed: BTreeMap<&str, &[&str]> = BTreeMap::from([
        (
            "capsulet_graph_worker_node_attempts_total",
            &["kind", "outcome", "failure"][..],
        ),
        (
            "capsulet_graph_worker_effect_attempts_total",
            &["kind", "outcome"][..],
        ),
        ("gen_ai_client_operation_duration_seconds", model_labels),
        ("gen_ai_client_token_usage", token_labels),
        (
            "capsulet_model_usage_unmeasured_total",
            &[
                "gen_ai_operation_name",
                "gen_ai_provider_name",
                "gen_ai_token_type",
            ][..],
        ),
    ]);

    let rendered = capsulet_observability::render_metrics();
    let mut seen = BTreeSet::new();
    for line in rendered.lines().filter(|line| !line.starts_with('#')) {
        let Some((name, rest)) = line.split_once('{') else {
            continue;
        };
        let base = name.trim_end_matches("_sum").trim_end_matches("_count");
        let Some(allowed) = closed.get(base) else {
            continue;
        };
        seen.insert(base);
        let (labels, _) = rest.split_once('}').expect("a closed label set");
        for pair in labels.split("\",") {
            let (key, value) = pair.split_once("=\"").expect("a key and a value");
            assert!(
                allowed.contains(&key),
                "`{base}` has a label `{key}` outside its closed set: {line}"
            );
            for identifier in [&stack.tenant, &stack.project, &id] {
                assert!(
                    !value.contains(identifier.as_str()),
                    "an unbounded identifier became a label value: {line}"
                );
            }
        }
    }
    for expected in [
        "capsulet_graph_worker_node_attempts_total",
        "gen_ai_client_operation_duration_seconds",
        "gen_ai_client_token_usage",
        "capsulet_model_usage_unmeasured_total",
    ] {
        assert!(
            seen.contains(expected),
            "`{expected}` was not recorded:\n{rendered}"
        );
    }
}

// ---- erasure

/// What the project still holds, as text, from every place a run's payloads live.
async fn held_content(stack: &Stack) -> Vec<String> {
    let mut held: Vec<String> = sqlx::query_scalar(
        "SELECT content FROM ir_run_values \
         WHERE tenant_id = $1 AND project_id = $2 AND content IS NOT NULL",
    )
    .bind(&stack.tenant)
    .bind(&stack.project)
    .fetch_all(stack.store.pool())
    .await
    .expect("values read");
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT state_json FROM agent_runs WHERE tenant_id = $1 AND project_id = $2",
    )
    .bind(&stack.tenant)
    .bind(&stack.project)
    .fetch_all(stack.store.pool())
    .await
    .expect("runs read");
    held.extend(rows);
    let snapshots: Vec<String> = sqlx::query_scalar(
        "SELECT snapshots.state_json FROM agent_state_snapshots snapshots \
         JOIN agent_runs runs ON runs.id = snapshots.agent_run_id \
         WHERE runs.tenant_id = $1 AND runs.project_id = $2",
    )
    .bind(&stack.tenant)
    .bind(&stack.project)
    .fetch_all(stack.store.pool())
    .await
    .expect("snapshots read");
    held.extend(snapshots);
    held
}

async fn answering_agent(stack: &Stack) -> String {
    stack
        .agent(
            answering_graph(&json!({ "rule.non_empty": "true" })),
            generous(),
            &["validator_pass"],
        )
        .await
}

#[tokio::test]
async fn erasing_a_finished_run_removes_what_it_held_and_keeps_its_record() {
    const QUERY: &str = "erase-me-question-3301";
    const ANSWER: &str = "erase-me-answer-3302";
    let stack = Stack::new(Some(ScriptedModel::answering(&[ANSWER]))).await;
    let agent = answering_agent(&stack).await;
    let id = stack
        .start(&agent, json!({ "query": QUERY, "answer": "" }))
        .await;
    stack.run_to_rest().await;
    assert_eq!(stack.run(&id).await["state"]["answer"], ANSWER);
    let record = stack.event_kinds(&id).await;
    assert!(
        held_content(&stack)
            .await
            .iter()
            .any(|each| each.contains(QUERY)),
        "the run held its question before it was erased"
    );

    // Not from another project, which cannot even tell the run exists.
    let run_id = AgentRunId::new(id.clone()).expect("a valid id");
    assert_eq!(
        stack
            .store
            .erase_agent_run_payloads_in_project(&run_id, &stack.tenant, "another-project", "x")
            .await
            .expect("asks"),
        None
    );

    let (status, body) = stack
        .post(&format!("/v1/agent-runs/{id}/erase"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body["values_erased"].as_u64().expect("a count") >= 4,
        "{body}"
    );
    assert_eq!(body["run"]["state"], json!({}));
    assert!(
        body["run"]["execution"]["payloads_erased_at"].is_string(),
        "{body}"
    );
    assert_eq!(
        body["run"]["status"], "succeeded",
        "erasing what a run held does not change how it ended"
    );
    assert_eq!(
        stack.event_kinds(&id).await,
        record,
        "the record that it ran stays whole"
    );
    for held in held_content(&stack).await {
        assert!(
            !held.contains(QUERY) && !held.contains(ANSWER),
            "still held: {held}"
        );
    }
    let tombstones: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ir_run_values \
         WHERE tenant_id = $1 AND project_id = $2 AND erased_by IS NOT NULL",
    )
    .bind(&stack.tenant)
    .bind(&stack.project)
    .fetch_one(stack.store.pool())
    .await
    .expect("counted");
    assert!(tombstones >= 4, "each erased value says when and by whom");

    let (status, again) = stack
        .post(&format!("/v1/agent-runs/{id}/erase"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(
        again["values_erased"], 0,
        "asking again erases nothing more"
    );

    // The same bytes produced later are held again: an erasure is about what was
    // held, not a ban on a string.
    let next = stack
        .start(&agent, json!({ "query": QUERY, "answer": "" }))
        .await;
    stack.run_to_rest().await;
    let rerun = stack.run(&next).await;
    assert_eq!(rerun["status"], "succeeded", "{rerun}");
    assert_eq!(rerun["state"]["answer"], ANSWER);
    assert_eq!(
        stack.run(&id).await["state"],
        json!({}),
        "and the erased run stays erased"
    );
}

#[tokio::test]
async fn a_run_that_has_not_ended_cannot_have_its_payloads_erased() {
    let stack = Stack::new(Some(ScriptedModel::answering(&["an answer"]))).await;
    let agent = answering_agent(&stack).await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;

    let (status, body) = stack
        .post(&format!("/v1/agent-runs/{id}/erase"), json!({}))
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "agent_run_state_conflict");

    stack.run_to_rest().await;
    let run = stack.run(&id).await;
    assert_eq!(run["status"], "succeeded", "nothing it needed went: {run}");
    assert_eq!(run["state"]["answer"], "an answer");
}

#[tokio::test]
async fn bytes_another_run_holds_are_kept_for_it_and_go_when_it_is_erased() {
    const QUERY: &str = "shared-question-4417";
    let stack = Stack::new(Some(ScriptedModel::answering(&["an answer"]))).await;
    let agent = answering_agent(&stack).await;
    let finished = stack
        .start(&agent, json!({ "query": QUERY, "answer": "" }))
        .await;
    stack.run_to_rest().await;
    // Queued with exactly the same input, so it holds the same stored value.
    let live = stack
        .start(&agent, json!({ "query": QUERY, "answer": "" }))
        .await;

    let (status, body) = stack
        .post(&format!("/v1/agent-runs/{finished}/erase"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body["values_kept"].as_u64().expect("a count") >= 1,
        "{body}"
    );
    assert_eq!(body["run"]["state"], json!({}));

    // The live run never notices: what it was given is still there for it.
    stack.run_to_rest().await;
    let run = stack.run(&live).await;
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(run["state"]["query"], QUERY);
    assert!(
        held_content(&stack)
            .await
            .iter()
            .any(|each| each.contains(QUERY)),
        "held, because a run that has not been erased holds it"
    );

    let (status, body) = stack
        .post(&format!("/v1/agent-runs/{live}/erase"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for held in held_content(&stack).await {
        assert!(
            !held.contains(QUERY),
            "nobody holds it now, and it is gone: {held}"
        );
    }
}

// ---- debug bundles
//
// Each of these exports a bundle over HTTP and reads it the way `capsulet-replay`
// does, offline. With `CAPSULET_WRITE_BUNDLE_FIXTURES` set they also write it to
// `crates/replay/tests/fixtures`, which is where the replay suite's fixtures come
// from: real runs, regenerated rather than written by hand.

/// A model that never answers, standing in for a worker that died mid-call.
struct SilentModel;

#[async_trait]
impl ModelClient for SilentModel {
    fn name(&self) -> String {
        "silent".to_string()
    }

    async fn complete(&self, _request: &ModelRequest) -> Result<Completion, ModelError> {
        std::future::pending().await
    }

    async fn embed(
        &self,
        _model: &str,
        _text: &str,
        _timeout: Duration,
    ) -> Result<Embedding, ModelError> {
        std::future::pending().await
    }
}

async fn bundle_of(stack: &Stack, id: &str, fixture: &str) -> (SealedRunBundle, RunBundle) {
    let (status, body) = stack
        .post(&format!("/v1/agent-runs/{id}/debug-bundle"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let sealed: SealedRunBundle = serde_json::from_value(body).expect("a sealed bundle");
    let bundle = sealed.open().expect("the bundle matches its digest");
    if std::env::var_os("CAPSULET_WRITE_BUNDLE_FIXTURES").is_some() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../replay/tests/fixtures")
            .join(format!("run-{fixture}.json"));
        let text = serde_json::to_string_pretty(&sealed).expect("encodes");
        std::fs::write(path, text + "\n").expect("the fixture is written");
    }
    (sealed, bundle)
}

#[tokio::test]
async fn a_bundle_explains_a_run_that_succeeded_and_carries_none_of_its_content() {
    const QUERY: &str = "bundle-question-5150";
    const ANSWER: &str = "bundle-answer-5151";
    let stack = Stack::new(Some(ScriptedModel::answering(&[ANSWER]))).await;
    let agent = answering_agent(&stack).await;
    let id = stack
        .start(&agent, json!({ "query": QUERY, "answer": "" }))
        .await;
    stack.run_to_rest().await;

    let (sealed, bundle) = bundle_of(&stack, &id, "succeeded").await;
    let explained = explain(&bundle);
    assert!(explained.agrees(), "{explained:?}");
    assert_eq!(explained.status, Ok(RunStatus::Completed));
    assert!(explained.definition_matches);
    assert_eq!(explained.spent.tokens, 10);
    assert_eq!(explained.unfinished, [] as [capsulet_ir::Identifier; 0]);
    let model = explained
        .nodes
        .iter()
        .find(|node| node.kind == "llm")
        .expect("the model node");
    assert_eq!(
        (model.attempts, model.finished, model.rework),
        (1, 1, Rework::Model)
    );
    assert!(
        !sealed.body.contains(QUERY) && !sealed.body.contains(ANSWER),
        "a bundle names what a run was given and produced by digest, never by content"
    );

    let mut tampered = sealed.clone();
    tampered.body = tampered.body.replacen("completed", "cancelled", 1);
    assert!(
        tampered.open().is_err(),
        "a changed body no longer matches its digest"
    );
}

#[tokio::test]
async fn a_bundle_explains_a_run_that_failed() {
    let stack = Stack::new(None).await;
    let agent = answering_agent(&stack).await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;
    stack.run_to_rest().await;

    let (_, bundle) = bundle_of(&stack, &id, "failed").await;
    let explained = explain(&bundle);
    assert!(explained.agrees(), "{explained:?}");
    assert_eq!(explained.status, Ok(RunStatus::Failed));
    assert!(
        explained
            .ended_because
            .as_deref()
            .is_some_and(|reason| reason.contains("verifier_unavailable")),
        "{explained:?}"
    );
}

#[tokio::test]
async fn a_bundle_explains_a_run_interrupted_mid_call() {
    let stack = Stack::new(None).await;
    let agent = answering_agent(&stack).await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;
    // The worker is dropped while it waits on the model, as a crash would leave it.
    let worker = stack.worker_using(Some(Arc::new(SilentModel)), 256);
    assert!(
        tokio::time::timeout(Duration::from_secs(3), worker.advance_one())
            .await
            .is_err(),
        "the model never answers"
    );

    let (_, bundle) = bundle_of(&stack, &id, "interrupted").await;
    let explained = explain(&bundle);
    assert!(explained.agrees(), "{explained:?}");
    assert_eq!(explained.status, Ok(RunStatus::Running));
    let doing: Vec<String> = explained
        .unfinished
        .iter()
        .map(ToString::to_string)
        .collect();
    let model = explained
        .nodes
        .iter()
        .find(|node| node.kind == "llm")
        .expect("the model node");
    assert_eq!(
        doing,
        vec![model.node.to_string()],
        "it was in the model call"
    );
    assert_eq!((model.attempts, model.finished), (1, 0));
}

#[tokio::test]
async fn a_bundle_explains_a_run_that_ran_out_of_budget() {
    let stack = Stack::new(Some(ScriptedModel::answering(&["one", "two", "three"]))).await;
    let agent = stack
        .agent(
            answering_graph(&json!({ "rule.contains": "never" })),
            json!({ "max_steps": 2, "max_tokens": 10_000, "max_seconds": 300, "max_cost_micros": 1_000 }),
            &["validator_pass"],
        )
        .await;
    let id = stack
        .start(&agent, json!({ "query": "why?", "answer": "" }))
        .await;
    stack.run_to_rest().await;

    let (_, bundle) = bundle_of(&stack, &id, "budget-exhausted").await;
    let explained = explain(&bundle);
    assert!(explained.agrees(), "{explained:?}");
    assert_eq!(explained.status, Ok(RunStatus::Failed));
    assert!(
        explained
            .ended_because
            .as_deref()
            .is_some_and(|reason| reason.contains("iterations budget ran out")),
        "{explained:?}"
    );
    let model = explained
        .nodes
        .iter()
        .find(|node| node.kind == "llm")
        .expect("the model node");
    assert_eq!(model.finished, 2, "one call each time round, twice");
}

#[tokio::test]
async fn payloads_past_retention_are_erased_and_newer_ones_kept() {
    const SHARED: &str = "retention-shared-question-7701";
    let stack = Stack::new(Some(ScriptedModel::answering(&["an answer"]))).await;
    let agent = answering_agent(&stack).await;
    let alone = stack
        .start(
            &agent,
            json!({ "query": "retention-alone-7700", "answer": "" }),
        )
        .await;
    let sharing = stack
        .start(&agent, json!({ "query": SHARED, "answer": "" }))
        .await;
    stack.run_to_rest().await;
    // Not run yet, and holding the same input as `sharing`.
    let live = stack
        .start(&agent, json!({ "query": SHARED, "answer": "" }))
        .await;
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("after the epoch")
            .as_millis(),
    )
    .expect("fits");

    // Retention has not run out for anything that ended in the last hour.
    let early = stack
        .store
        .erase_payloads_past_retention(now - 3_600_000, Some(&stack.tenant), 100)
        .await
        .expect("sweeps");
    assert_eq!(early, capsulet_postgres::PayloadRetentionSweep::default());
    assert_eq!(stack.run(&alone).await["state"]["answer"], "an answer");

    let due = stack
        .store
        .erase_payloads_past_retention(now + 3_600_000, Some(&stack.tenant), 100)
        .await
        .expect("sweeps");
    assert_eq!(
        due.runs_erased, 2,
        "both ended runs are past retention: {due:?}"
    );
    assert!(due.values_erased >= 4, "{due:?}");
    assert!(
        due.values_kept >= 1,
        "what the live run holds stays: {due:?}"
    );

    for erased in [&alone, &sharing] {
        let run = stack.run(erased).await;
        assert_eq!(run["state"], json!({}));
        assert!(run["execution"]["payloads_erased_at"].is_string());
    }
    let by: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT erased_by FROM ir_run_values \
         WHERE tenant_id = $1 AND project_id = $2 AND erased_by IS NOT NULL",
    )
    .bind(&stack.tenant)
    .bind(&stack.project)
    .fetch_all(stack.store.pool())
    .await
    .expect("read");
    assert_eq!(by, vec![capsulet_postgres::RETENTION_ERASER.to_string()]);
    assert_eq!(
        stack.run(&live).await["status"],
        "queued",
        "a run that has not ended is never swept"
    );

    // The live run runs as if nothing happened, and is swept once it is past retention.
    stack.run_to_rest().await;
    assert_eq!(stack.run(&live).await["state"]["query"], SHARED);
    let next = stack
        .store
        .erase_payloads_past_retention(now + 3_600_000, Some(&stack.tenant), 100)
        .await
        .expect("sweeps");
    assert_eq!(next.runs_erased, 1, "{next:?}");
    for held in held_content(&stack).await {
        assert!(!held.contains(SHARED), "still held: {held}");
    }
}
