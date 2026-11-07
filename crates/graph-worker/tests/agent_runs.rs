//! An agent run from the HTTP request to the worker that executes it.
//!
//! Real router, real `PostgreSQL`, real graph worker. The only thing faked is the
//! model, because what is being tested is everything around it. Each test gets its
//! own tenant, and its workers only lease that tenant's runs, so the suite can share
//! a database without one test advancing another's run.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use capsulet_api::{AppState, AuthConfig, router};
use capsulet_application::JobRunRepository;
use capsulet_core::{
    ClaimId, Entity, EntityId, JobDefinition, JobDefinitionId, JobRunId, MemoryScope,
};
use capsulet_graph_worker::{
    AgentExecutor, Completion, Embedding, Executor, GraphWorker, ModelClient, ModelError,
    ModelRequest, Progress, SystemClock, WorkerConfig,
};
use capsulet_ir::digest::Digest;
use capsulet_postgres::PostgresStore;
use capsulet_runtime::RunEvent;
use capsulet_runtime::event::NodeUsage;
use capsulet_storage::FilesystemObjectStore;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

static NEXT: AtomicU64 = AtomicU64::new(1);

fn unique(prefix: &str) -> String {
    let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}_ar{}_{sequence}", std::process::id())
}

fn database_url() -> String {
    std::env::var("CAPSULET_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect(
            "CAPSULET_TEST_DATABASE_URL is required; run \
             `cargo run -p capsulet-xtask --locked -- verify --gate postgres`",
        )
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

        let root = std::env::temp_dir().join(unique("capsulet-agent-runs"));
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
