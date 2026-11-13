//! An agent run from the HTTP request to the worker that executes it.
//!
//! Real router, real `PostgreSQL`, real graph worker. The only thing faked is the
//! model, because what is being tested is everything around it. Each test gets its
//! own tenant, and its workers only lease that tenant's runs, so the suite can share
//! a database without one test advancing another's run.
//!
//! One module per area of what is tested; what they share is here.

#[path = "../support/mod.rs"]
mod support;

mod decisions;
mod erasure;
mod execution;
mod explanations;
mod jobs;
mod memory;
mod telemetry;

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

async fn answering_agent(stack: &Stack) -> String {
    stack
        .agent(
            answering_graph(&json!({ "rule.non_empty": "true" })),
            generous(),
            &["validator_pass"],
        )
        .await
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

/// Every decision in the run's log is the one `decide` makes again from the log
/// before it, read from the run's bundle as anyone debugging it would.
async fn assert_decisions_replay(stack: &Stack, id: &str) {
    let (status, body) = stack
        .post(&format!("/v1/agent-runs/{id}/debug-bundle"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let sealed: SealedRunBundle = serde_json::from_value(body).expect("a sealed bundle");
    let bundle = sealed.open().expect("the bundle matches its digest");
    let replay = capsulet_runtime::replay::replay_decisions(&bundle.definition, &bundle.events);
    assert!(
        replay.agrees(),
        "decisions the runtime would not make again: {:?}",
        replay.divergences
    );
}

/// The run's own export, opened as an offline reader opens it.
async fn exported(stack: &Stack, id: &str) -> RunBundle {
    let (status, body) = stack
        .post(&format!("/v1/agent-runs/{id}/debug-bundle"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let sealed: SealedRunBundle = serde_json::from_value(body).expect("a sealed bundle");
    sealed.open().expect("the bundle matches its digest")
}
