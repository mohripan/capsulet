//! Running what an agent's graph says to run.
//!
//! The executor below is what makes a translated agent do real work. It is reached
//! only through the node's `capsulet.kind` parameter, so a definition that was not
//! compiled from an agent keeps being refused rather than guessed at.
//!
//! Two things hold everywhere here. Every value a node produces is checked against
//! the schema its port declares before it is stored, because a port type nothing
//! enforces is a label. And nothing is assumed: a value no source produced, a rule
//! nobody knows, or a model nobody configured stops the node with a typed failure
//! saying which, because the failure's kind is what a declared repair route matches.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use capsulet_application::JobRunRepository;
use capsulet_core::{
    Authority, Claim, ClaimId, ClaimStatus, Confidence, EntityId, EvidenceId, ExecutionPoolName,
    JobDefinitionId, JobRun, JobRunId, MemoryScope,
};
use capsulet_ir::agent::{
    AGENT_CONTINUE, AGENT_ENTER, AGENT_LEAVE, CHECK_PORT_PREFIX, DISTINCT_RESULTS, KEEP_GOING,
    KIND_PARAMETER, RETRY_PARAMETER, SEEN_RESULTS,
};
use capsulet_ir::digest::Digest;
use capsulet_ir::graph::Combine;
use capsulet_ir::loop_region::FailureKind;
use capsulet_ir::node::Node;
use capsulet_observability::genai;
use capsulet_observability::tracing::Instrument;
use capsulet_postgres::{MAX_IR_RUN_VALUE_BYTES, PostgresStore, PostgresStoreError, StoredValue};
use capsulet_runtime::event::{ControlValue, NodeUsage};
use capsulet_runtime::values::check_value;
use serde_json::{Value, json};

use crate::execute::{
    CompensationRequest, EffectOutcome, EffectRequest, Executor, NodeOutcome, NodeRequest,
    ResolvedInput,
};
use crate::model::{ModelClient, ModelError, ModelRequest};
use crate::telemetry::{self, ModelCall};

/// The default number of claims a retriever returns.
const DEFAULT_TOP_K: usize = 5;
/// How many of a project's newest claims a lexical search looks through.
const CLAIM_SEARCH_WINDOW: i64 = 500;
/// The most of a provider's error message a failure detail keeps, in bytes.
const MAX_PROVIDER_MESSAGE: usize = 512;
/// How often a job node looks to see whether its job has finished.
const JOB_POLL: Duration = Duration::from_millis(500);

/// A node's failure, on its way to becoming a [`NodeOutcome::Failed`].
struct Failure {
    kind: FailureKind,
    detail: String,
    usage: Option<Box<NodeUsage>>,
}

impl Failure {
    fn new(kind: FailureKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
            usage: None,
        }
    }

    fn storage(error: &PostgresStoreError) -> Self {
        Self::new(
            FailureKind::VerifierUnavailable,
            format!("the value store refused: {error}"),
        )
    }
}

/// Runs an agent's nodes.
pub struct AgentExecutor {
    store: PostgresStore,
    model: Option<Arc<dyn ModelClient>>,
}

impl AgentExecutor {
    /// An executor over a store, with a model client when one is configured.
    ///
    /// With none, a model node fails and says so rather than the run hanging or the
    /// node pretending. Everything deterministic still runs.
    #[must_use]
    pub fn new(store: PostgresStore, model: Option<Arc<dyn ModelClient>>) -> Self {
        Self { store, model }
    }

    async fn run(&self, request: &NodeRequest<'_>, kind: &str) -> Result<NodeOutcome, Failure> {
        match kind {
            AGENT_ENTER | AGENT_LEAVE => Ok(pass_through(request)?),
            AGENT_CONTINUE => self.decide_continuation(request).await,
            "query_normalizer" => {
                let inputs = self.load_inputs(request).await?;
                let text = first_text(request.node, &inputs)?;
                self.finish(request, &json!(normalize(&text)), None).await
            }
            "prompt_builder" => {
                let inputs = self.load_inputs(request).await?;
                let prompt = self.build_prompt(request, &inputs).await?;
                self.finish(request, &json!(prompt), None).await
            }
            "return" => {
                let inputs = self.load_inputs(request).await?;
                let value = inputs.into_values().next().unwrap_or(Value::Null);
                self.finish(request, &value, None).await
            }
            "llm" | "planner" => self.complete(request).await,
            "embedding" => self.embed(request).await,
            "retriever" => {
                let inputs = self.load_inputs(request).await?;
                let query = first_text(request.node, &inputs)?;
                let top_k = parameter_number(request.node, "top_k").unwrap_or(DEFAULT_TOP_K);
                let documents = self
                    .search_claims(request.tenant, request.project, &query, top_k)
                    .await?;
                self.finish(request, &Value::Array(documents), None).await
            }
            "memory_read" => {
                let limit = parameter_number(request.node, "limit").unwrap_or(DEFAULT_TOP_K);
                let documents = self
                    .search_claims(request.tenant, request.project, "", limit)
                    .await?;
                self.finish(request, &Value::Array(documents), None).await
            }
            "reranker" => self.rerank(request).await,
            "validator" => self.validate(request).await,
            // An effect happens at most once in a run, however many times its loop
            // goes round, so a later iteration reads what the first one produced.
            "job" | "memory_write" => Ok(NodeOutcome::finished(request.previous_outputs.clone())),
            other => Err(Failure::new(
                FailureKind::VerifierUnavailable,
                format!("no provider runs a `{other}` node"),
            )),
        }
    }

    /// Reads every input a node has, assembling multi-source ports.
    async fn load_inputs(
        &self,
        request: &NodeRequest<'_>,
    ) -> Result<BTreeMap<String, Value>, Failure> {
        let mut loaded = BTreeMap::new();
        for (port, input) in request.inputs {
            let value = self
                .load_input(request.tenant, request.project, request.node, port, input)
                .await?;
            loaded.insert(port.clone(), value);
        }
        Ok(loaded)
    }

    async fn load_input(
        &self,
        tenant: &str,
        project: &str,
        node: &Node,
        port: &str,
        input: &ResolvedInput,
    ) -> Result<Value, Failure> {
        let mut values = Vec::with_capacity(input.sources.len());
        for source in &input.sources {
            let Some(digest) = source.digest else {
                return Err(Failure::new(
                    FailureKind::MissingEvidence,
                    format!(
                        "input `{port}` of `{}` reads `{}`, which has produced nothing",
                        node.id, source.name
                    ),
                ));
            };
            values.push((
                source.name.as_str(),
                self.load_value(tenant, project, &digest).await?,
            ));
        }

        match &input.combine {
            Combine::Record { fields } => {
                let mut record = serde_json::Map::new();
                for (field, index) in fields {
                    if let Some((_, value)) = values.get(*index) {
                        record.insert(field.clone(), value.clone());
                    }
                }
                Ok(Value::Object(record))
            }
            Combine::Concat => {
                let mut items = Vec::new();
                for (_, value) in values {
                    match value {
                        Value::Array(mut each) => items.append(&mut each),
                        other => items.push(other),
                    }
                }
                Ok(Value::Array(items))
            }
            Combine::Forward | Combine::Select { .. } => values
                .into_iter()
                .next()
                .map(|(_, value)| value)
                .ok_or_else(|| {
                    Failure::new(
                        FailureKind::MissingEvidence,
                        format!("input `{port}` of `{}` has no source", node.id),
                    )
                }),
        }
    }

    async fn load_value(
        &self,
        tenant: &str,
        project: &str,
        digest: &Digest,
    ) -> Result<Value, Failure> {
        let stored = match self
            .store
            .read_ir_run_value(tenant, project, digest)
            .await
            .map_err(|error| Failure::storage(&error))?
        {
            Some(StoredValue::Present(content)) => content,
            // Somebody asked for these bytes not to be held any more. Saying so is
            // the difference between a run that lost data and one that was told to.
            Some(StoredValue::Erased) => {
                return Err(Failure::new(
                    FailureKind::MissingEvidence,
                    format!("the value for {digest} was erased on request"),
                ));
            }
            None => {
                return Err(Failure::new(
                    FailureKind::StaleReference,
                    format!("no value is stored for {digest} in this project"),
                ));
            }
        };
        serde_json::from_str(&stored).map_err(|error| {
            Failure::new(
                FailureKind::StaleReference,
                format!("the value stored for {digest} does not parse: {error}"),
            )
        })
    }

    /// Checks a value against every output port and stores it once per distinct
    /// encoding.
    async fn finish(
        &self,
        request: &NodeRequest<'_>,
        value: &Value,
        usage: Option<NodeUsage>,
    ) -> Result<NodeOutcome, Failure> {
        let outputs = self
            .store_outputs(request.tenant, request.project, request.node, value)
            .await
            .map_err(|mut failure| {
                failure.usage = usage.clone().map(Box::new);
                failure
            })?;
        Ok(NodeOutcome::Finished {
            outputs,
            control: BTreeMap::new(),
            usage,
        })
    }

    async fn store_outputs(
        &self,
        tenant: &str,
        project: &str,
        node: &Node,
        value: &Value,
    ) -> Result<BTreeMap<String, Digest>, Failure> {
        let mut outputs = BTreeMap::new();
        for port in &node.outputs {
            check_value(&port.schema, value).map_err(|violation| {
                Failure::new(
                    FailureKind::SchemaMismatch,
                    format!("output `{}` of `{}` {violation}", port.id, node.id),
                )
            })?;
            let digest = self.put_value(tenant, project, value).await?;
            outputs.insert(port.id.to_string(), digest);
        }
        Ok(outputs)
    }

    async fn put_value(
        &self,
        tenant: &str,
        project: &str,
        value: &Value,
    ) -> Result<Digest, Failure> {
        let content = serde_json::to_string(value).map_err(|error| {
            Failure::new(
                FailureKind::SchemaMismatch,
                format!("a value could not be encoded: {error}"),
            )
        })?;
        if content.len() > MAX_IR_RUN_VALUE_BYTES {
            return Err(Failure::new(
                FailureKind::SchemaMismatch,
                format!(
                    "a value of {} bytes is larger than the {MAX_IR_RUN_VALUE_BYTES} a run may \
                     store",
                    content.len()
                ),
            ));
        }
        self.store
            .put_ir_run_value(tenant, project, &content)
            .await
            .map_err(|error| Failure::storage(&error))
    }

    /// Decides whether the loop goes round again.
    ///
    /// A failed check goes round when the agent stops on a validator pass, and ends
    /// the run when it does not: a check that failed and was then carried past would
    /// make the run's success say something false.
    ///
    /// The progress reading is how many different results this loop has produced. An
    /// iteration that lands on a result it already produced has gone nowhere, however
    /// busy it looked, and the declared measure stops a loop that keeps doing so.
    async fn decide_continuation(&self, request: &NodeRequest<'_>) -> Result<NodeOutcome, Failure> {
        let inputs = self.load_inputs(request).await?;
        let failed: Vec<String> = inputs
            .iter()
            .filter(|(port, _)| port.starts_with(CHECK_PORT_PREFIX))
            .filter(|(_, result)| result.get("passed").and_then(Value::as_bool) != Some(true))
            .map(|(port, result)| {
                let name = port.trim_start_matches(CHECK_PORT_PREFIX);
                result
                    .get("detail")
                    .and_then(Value::as_str)
                    .map_or_else(|| name.to_string(), |detail| format!("{name}: {detail}"))
            })
            .collect();

        let retries = request
            .node
            .parameters
            .get(RETRY_PARAMETER)
            .is_some_and(|value| value == "true");
        if !failed.is_empty() && !retries {
            return Err(Failure::new(
                FailureKind::MissingEvidence,
                format!(
                    "a check did not pass and this agent does not retry on one: {}",
                    failed.join("; ")
                ),
            ));
        }
        let keep_going = !failed.is_empty();

        // Digests rather than values: this compares what was produced, and a digest
        // is exactly that comparison.
        let fingerprint = request
            .inputs
            .iter()
            .flat_map(|(port, input)| {
                input.sources.iter().map(move |source| {
                    format!(
                        "{port}={}",
                        source
                            .digest
                            .map_or_else(String::new, |digest| digest.to_string())
                    )
                })
            })
            .collect::<Vec<_>>()
            .join("\n");
        let fingerprint = Digest::of(fingerprint.as_bytes()).to_string();

        let mut seen: BTreeSet<String> = match request.previous_outputs.get(SEEN_RESULTS) {
            Some(digest) => self
                .load_value(request.tenant, request.project, digest)
                .await?
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            None => BTreeSet::new(),
        };
        seen.insert(fingerprint);
        let distinct = seen.len();

        let mut outputs = BTreeMap::new();
        for (port, value) in [
            (KEEP_GOING, json!(keep_going)),
            (DISTINCT_RESULTS, json!(distinct)),
            (SEEN_RESULTS, json!(seen)),
        ] {
            outputs.insert(
                port.to_string(),
                self.put_value(request.tenant, request.project, &value)
                    .await?,
            );
        }
        Ok(NodeOutcome::Finished {
            outputs,
            control: BTreeMap::from([
                (
                    KEEP_GOING.to_string(),
                    ControlValue::Bool { value: keep_going },
                ),
                (
                    DISTINCT_RESULTS.to_string(),
                    ControlValue::Integer {
                        value: i128::try_from(distinct).unwrap_or(i128::MAX),
                    },
                ),
            ]),
            usage: None,
        })
    }

    /// Fills a template from the node's inputs, or labels them when it has none.
    ///
    /// One pass over the template, so a value containing `{{something}}` stays text
    /// and cannot pull in a port its author was never given.
    async fn build_prompt(
        &self,
        request: &NodeRequest<'_>,
        inputs: &BTreeMap<String, Value>,
    ) -> Result<String, Failure> {
        let mut rendered = BTreeMap::new();
        for (port, value) in inputs {
            rendered.insert(
                port.clone(),
                self.render(request.tenant, request.project, value).await?,
            );
        }
        let Some(template) = request.node.parameters.get("template") else {
            return Ok(rendered
                .iter()
                .map(|(port, text)| format!("{port}:\n{text}"))
                .collect::<Vec<_>>()
                .join("\n\n"));
        };
        fill_template(template, &rendered).map_err(|name| {
            Failure::new(
                FailureKind::SchemaMismatch,
                format!(
                    "the template of `{}` names `{{{{{name}}}}}`, and no input port is called that",
                    request.node.id
                ),
            )
        })
    }

    /// A value as text for a prompt: documents become the text they cite.
    async fn render(&self, tenant: &str, project: &str, value: &Value) -> Result<String, Failure> {
        match value {
            Value::String(text) => Ok(text.clone()),
            Value::Array(items)
                if !items.is_empty()
                    && items
                        .iter()
                        .all(|item| item.get("content_digest").is_some()) =>
            {
                let mut lines = Vec::with_capacity(items.len());
                for item in items {
                    let digest: Digest = item
                        .get("content_digest")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .parse()
                        .map_err(|error: capsulet_ir::digest::DigestError| {
                            Failure::new(FailureKind::StaleReference, error.to_string())
                        })?;
                    let text = self.load_value(tenant, project, &digest).await?;
                    let source = item.get("source_id").and_then(Value::as_str).unwrap_or("");
                    lines.push(format!("[{source}] {}", text.as_str().unwrap_or_default()));
                }
                Ok(lines.join("\n"))
            }
            other => Ok(other.to_string()),
        }
    }

    async fn complete(&self, request: &NodeRequest<'_>) -> Result<NodeOutcome, Failure> {
        let (client, model) = self.model_for(request.node)?;
        let inputs = self.load_inputs(request).await?;
        let prompt = first_text(request.node, &inputs)?;
        let asked = ModelRequest {
            model: model.clone(),
            prompt,
            temperature_milli: parameter_number(request.node, "temperature_milli")
                .and_then(|value| u32::try_from(value).ok()),
            max_tokens: parameter_number(request.node, "max_tokens")
                .and_then(|value| u32::try_from(value).ok()),
            timeout: Duration::from_millis(request.node.budget.wall_ms),
        };
        let call = ModelCall::start(
            genai::TEXT_COMPLETION,
            &client.name(),
            &model,
            asked.temperature_milli,
            asked.max_tokens,
        );
        let answered = client.complete(&asked).instrument(call.span()).await;
        call.finish(answered.as_ref().map(|completion| &completion.usage));
        let outcome = answered.map_err(|error| model_failure(&client.name(), &model, &error))?;
        self.finish(request, &json!(outcome.text), Some(outcome.usage))
            .await
    }

    async fn embed(&self, request: &NodeRequest<'_>) -> Result<NodeOutcome, Failure> {
        let (client, model) = self.model_for(request.node)?;
        let inputs = self.load_inputs(request).await?;
        let text = first_text(request.node, &inputs)?;
        let timeout = Duration::from_millis(request.node.budget.wall_ms);
        let call = ModelCall::start(genai::EMBEDDINGS, &client.name(), &model, None, None);
        let answered = client
            .embed(&model, &text, timeout)
            .instrument(call.span())
            .await;
        call.finish(answered.as_ref().map(|embedding| &embedding.usage));
        let outcome = answered.map_err(|error| model_failure(&client.name(), &model, &error))?;
        // A vector is floating point, which has no canonical encoding, so it is stored
        // and the graph moves its digest. That is what the port's schema says.
        let digest = self
            .put_value(request.tenant, request.project, &outcome.vector)
            .await?;
        self.finish(request, &json!(digest.to_string()), Some(outcome.usage))
            .await
    }

    fn model_for(&self, node: &Node) -> Result<(Arc<dyn ModelClient>, String), Failure> {
        let Some(client) = &self.model else {
            return Err(Failure::new(
                FailureKind::VerifierUnavailable,
                format!(
                    "`{}` needs a model and no model endpoint is configured; set \
                     CAPSULET_OLLAMA_URL on the graph worker",
                    node.id
                ),
            ));
        };
        let Some(binding) = &node.provider else {
            return Err(Failure::new(
                FailureKind::VerifierUnavailable,
                format!("`{}` names no model", node.id),
            ));
        };
        Ok((Arc::clone(client), binding.selection.clone()))
    }

    /// Looks for the project's active claims sharing words with a query.
    ///
    /// Lexical on purpose: it is what can run with nothing configured, and it is
    /// deterministic. It finds shared words and nothing about meaning. A claim still
    /// under review is not offered, because a run acting on it would launder a
    /// candidate into a fact.
    async fn search_claims(
        &self,
        tenant: &str,
        project: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Value>, Failure> {
        let span = telemetry::retrieval_span(limit, CLAIM_SEARCH_WINDOW);
        let found = self
            .rank_claims(tenant, project, query, limit)
            .instrument(span.clone())
            .await;
        telemetry::finish_retrieval(&span, found.as_ref().ok().map(Vec::len));
        found
    }

    async fn rank_claims(
        &self,
        tenant: &str,
        project: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Value>, Failure> {
        let claims = self
            .store
            .list_memory_claims(tenant, project, CLAIM_SEARCH_WINDOW)
            .await
            .map_err(|error| Failure::storage(&error))?;
        let wanted = words(query);

        let mut scored: Vec<(usize, Claim)> = claims
            .into_iter()
            .filter(|claim| claim.status() == ClaimStatus::Active)
            .map(|claim| (overlap(&wanted, &claim_text(&claim)), claim))
            .filter(|(score, _)| wanted.is_empty() || *score > 0)
            .collect();
        // Best first, then by id, so the same memory answers the same way.
        scored.sort_by(|(left_score, left), (right_score, right)| {
            right_score
                .cmp(left_score)
                .then_with(|| left.id().as_str().cmp(right.id().as_str()))
        });

        let mut documents = Vec::new();
        for (_, claim) in scored.into_iter().take(limit) {
            let digest = self
                .put_value(tenant, project, &json!(claim_text(&claim)))
                .await?;
            documents.push(json!({
                "source_id": claim.id().as_str(),
                "content_digest": digest.to_string(),
            }));
        }
        Ok(documents)
    }

    async fn rerank(&self, request: &NodeRequest<'_>) -> Result<NodeOutcome, Failure> {
        let inputs = self.load_inputs(request).await?;
        let query = inputs.values().find_map(Value::as_str).unwrap_or_default();
        let wanted = words(query);
        let documents = inputs
            .values()
            .find_map(Value::as_array)
            .cloned()
            .ok_or_else(|| {
                Failure::new(
                    FailureKind::MissingEvidence,
                    format!("`{}` was given no documents to order", request.node.id),
                )
            })?;

        let mut scored = Vec::with_capacity(documents.len());
        for document in documents {
            let text = self
                .render(
                    request.tenant,
                    request.project,
                    &Value::Array(vec![document.clone()]),
                )
                .await?;
            scored.push((overlap(&wanted, &text), document));
        }
        // Stable, so documents the query does not separate keep the order they had.
        scored.sort_by(|(left, _), (right, _)| right.cmp(left));
        let ordered: Vec<Value> = scored.into_iter().map(|(_, document)| document).collect();
        self.finish(request, &Value::Array(ordered), None).await
    }

    /// Runs a validator's rules and reports what it found.
    ///
    /// `on_fail` says what a failed check means. By default it is a result the loop
    /// reads and may go round on. `safety` fails the node, which the agent's policy
    /// routes to a stop. `escalate` fails it into a gate, and a person who approves it
    /// waives the failure for the attempt that follows.
    async fn validate(&self, request: &NodeRequest<'_>) -> Result<NodeOutcome, Failure> {
        let inputs = self.load_inputs(request).await?;
        let subject = match inputs.values().next() {
            Some(value) => self.render(request.tenant, request.project, value).await?,
            None => String::new(),
        };
        let problems = run_rules(&request.node.parameters, &subject)
            .map_err(|detail| Failure::new(FailureKind::SchemaMismatch, detail))?;

        if problems.is_empty() {
            return self.finish(request, &json!({ "passed": true }), None).await;
        }
        let detail = problems.join("; ");

        if let Some(by) = request.approved_by {
            return self
                .finish(
                    request,
                    &json!({ "passed": true, "detail": format!("waived by {by}: {detail}") }),
                    None,
                )
                .await;
        }

        match request.node.parameters.get("on_fail").map(String::as_str) {
            Some("safety") => Err(Failure::new(FailureKind::UnsafeEffect, detail)),
            Some("escalate") => Err(Failure::new(FailureKind::InterpretationResidual, detail)),
            Some("retry") | None => {
                self.finish(request, &json!({ "passed": false, "detail": detail }), None)
                    .await
            }
            Some(other) => Err(Failure::new(
                FailureKind::SchemaMismatch,
                format!("`on_fail: {other}` is not something a validator can do"),
            )),
        }
    }

    /// Runs a job, deriving its run's id from the claim's key.
    ///
    /// That derivation is the whole of the retry story: a worker that crashed after
    /// creating the run and before recording it creates nothing the second time, and
    /// goes back to waiting for the run it already started.
    async fn perform_job(&self, request: &EffectRequest<'_>, node: &Node) -> EffectOutcome {
        let refuse =
            |failure: FailureKind, detail: String| EffectOutcome::Failed { failure, detail };

        let Some(key) = request.key else {
            return refuse(
                FailureKind::UnsafeEffect,
                format!("`{}` presented no idempotency key", request.effect.id),
            );
        };
        let Some(definition) = node.parameters.get("job_definition_id") else {
            return refuse(
                FailureKind::SchemaMismatch,
                format!("`{}` names no job definition", node.id),
            );
        };
        let definition = match JobDefinitionId::new(definition.clone()) {
            Ok(definition) => definition,
            Err(error) => return refuse(FailureKind::SchemaMismatch, error),
        };
        let pool = node
            .parameters
            .get("execution_pool")
            .map_or("default", String::as_str);
        let pool = match ExecutionPoolName::new(pool) {
            Ok(pool) => pool,
            Err(error) => return refuse(FailureKind::SchemaMismatch, error),
        };

        let mut input = serde_json::Map::new();
        for (port, resolved) in request.inputs {
            match self
                .load_input(request.tenant, request.project, node, port, resolved)
                .await
            {
                Ok(value) => {
                    input.insert(port.clone(), value);
                }
                Err(failure) => return refuse(failure.kind, failure.detail),
            }
        }

        let hex = Digest::of(key.as_bytes()).to_string();
        let run_id = format!(
            "agent_job_{}",
            hex.rsplit(':')
                .next()
                .unwrap_or_default()
                .get(..32)
                .unwrap_or_default()
        );
        let run_id = match JobRunId::new(run_id) {
            Ok(run_id) => run_id,
            Err(error) => return refuse(FailureKind::SchemaMismatch, error),
        };
        let run = match JobRun::new(run_id.clone(), definition, pool)
            .with_input(Value::Object(input).to_string())
        {
            Ok(run) => run,
            Err(error) => return refuse(FailureKind::SchemaMismatch, error),
        };

        match self
            .store
            .create_job_run_if_absent_in_project(&run, request.tenant, request.project)
            .await
        {
            Ok(_) => {}
            // The far side refused outright: this project does not own that job.
            Err(PostgresStoreError::ScopeMismatch) => {
                return refuse(
                    FailureKind::PolicyDenial,
                    format!("job definition for `{}` is not in this project", node.id),
                );
            }
            // Whether the write landed is unknown. The key makes trying again safe.
            Err(error) => {
                return EffectOutcome::Uncertain {
                    detail: error.to_string(),
                };
            }
        }

        self.wait_for_job(request, &run_id, node).await
    }

    async fn wait_for_job(
        &self,
        request: &EffectRequest<'_>,
        run_id: &JobRunId,
        node: &Node,
    ) -> EffectOutcome {
        let receipt = Digest::of(run_id.as_str().as_bytes());
        let deadline = Instant::now() + Duration::from_millis(node.budget.wall_ms);
        loop {
            let run = match self.store.find_by_id(run_id).await {
                Ok(Some(run)) => run,
                Ok(None) => {
                    return EffectOutcome::Uncertain {
                        detail: format!("job run {run_id} is not there"),
                    };
                }
                Err(error) => {
                    return EffectOutcome::Uncertain {
                        detail: error.to_string(),
                    };
                }
            };
            let status = run.status();
            if status.is_terminal() {
                let reported = json!({
                    "job_run_id": run_id.as_str(),
                    "status": status.to_string(),
                });
                let outputs = match self
                    .store_outputs(request.tenant, request.project, node, &reported)
                    .await
                {
                    Ok(outputs) => outputs,
                    Err(failure) => {
                        return EffectOutcome::Performed {
                            receipt,
                            outputs: BTreeMap::new(),
                            node_failure: Some((failure.kind, failure.detail)),
                        };
                    }
                };
                return if status == capsulet_core::JobRunStatus::Succeeded {
                    EffectOutcome::Performed {
                        receipt,
                        outputs,
                        node_failure: None,
                    }
                } else {
                    // The job ran, so the effect is not abandoned. It is the node
                    // that failed, on the strength of how the job came out.
                    EffectOutcome::Performed {
                        receipt,
                        outputs,
                        node_failure: Some((
                            FailureKind::MissingEvidence,
                            format!("job run {run_id} finished {status}"),
                        )),
                    }
                };
            }
            if Instant::now() >= deadline {
                return EffectOutcome::Performed {
                    receipt,
                    outputs: BTreeMap::new(),
                    node_failure: Some((
                        FailureKind::BudgetExhaustion,
                        format!(
                            "job run {run_id} is still {status} after {} ms; the run has stopped \
                             waiting for it, and it may still finish",
                            node.budget.wall_ms
                        ),
                    )),
                };
            }
            tokio::time::sleep(JOB_POLL).await;
        }
    }

    /// Proposes a claim for review. Never an active one: an agent's output is a
    /// candidate until somebody with the authority says otherwise.
    async fn perform_memory_write(
        &self,
        request: &EffectRequest<'_>,
        node: &Node,
    ) -> EffectOutcome {
        let refuse =
            |failure: FailureKind, detail: String| EffectOutcome::Failed { failure, detail };
        let Some(key) = request.key else {
            return refuse(
                FailureKind::UnsafeEffect,
                format!("`{}` presented no idempotency key", request.effect.id),
            );
        };

        let mut value = None;
        for (port, resolved) in request.inputs {
            match self
                .load_input(request.tenant, request.project, node, port, resolved)
                .await
            {
                Ok(loaded) => value = Some(loaded),
                Err(failure) => return refuse(failure.kind, failure.detail),
            }
        }
        let Some(value) = value else {
            return refuse(
                FailureKind::MissingEvidence,
                format!("`{}` was given nothing to write", node.id),
            );
        };

        let hex = Digest::of(key.as_bytes()).to_string();
        let claim_id = format!(
            "agent_claim_{}",
            hex.rsplit(':')
                .next()
                .unwrap_or_default()
                .get(..32)
                .unwrap_or_default()
        );
        let claim = match claim_from_value(&claim_id, request.tenant, request.project, &value) {
            Ok(claim) => claim,
            Err(detail) => return refuse(FailureKind::SchemaMismatch, detail),
        };

        // A retry must not reset a claim somebody has reviewed since it was proposed.
        match self.store.find_memory_claim(claim.id()).await {
            Ok(Some(_)) => {}
            Ok(None) => {
                if let Err(error) = self.store.upsert_memory_claim(&claim).await {
                    return match error {
                        PostgresStoreError::ScopeMismatch => refuse(
                            FailureKind::PolicyDenial,
                            "this claim belongs to another project".to_string(),
                        ),
                        // A bad reference is a refusal: nothing was written.
                        PostgresStoreError::Sqlx(sqlx::Error::Database(database))
                            if database.is_foreign_key_violation() =>
                        {
                            refuse(FailureKind::StaleReference, database.to_string())
                        }
                        other => EffectOutcome::Uncertain {
                            detail: other.to_string(),
                        },
                    };
                }
            }
            Err(error) => {
                return EffectOutcome::Uncertain {
                    detail: error.to_string(),
                };
            }
        }
        EffectOutcome::performed(Digest::of(claim_id.as_bytes()))
    }
}

#[async_trait]
impl Executor for AgentExecutor {
    async fn run_node(&self, request: NodeRequest<'_>) -> NodeOutcome {
        let Some(kind) = request.node.parameters.get(KIND_PARAMETER) else {
            return NodeOutcome::Failed {
                failure: FailureKind::VerifierUnavailable,
                detail: format!(
                    "no provider is configured for `{}`: only nodes compiled from an agent run \
                     yet",
                    request.node.id
                ),
                usage: None,
            };
        };
        match self.run(&request, kind).await {
            Ok(outcome) => outcome,
            Err(failure) => NodeOutcome::Failed {
                failure: failure.kind,
                detail: failure.detail,
                usage: failure.usage.map(|usage| *usage),
            },
        }
    }

    async fn perform_effect(&self, request: EffectRequest<'_>) -> EffectOutcome {
        let refused = |detail: String| EffectOutcome::Failed {
            failure: FailureKind::VerifierUnavailable,
            detail,
        };
        let Some(node) = request.definition.graph.node(request.node) else {
            return refused(format!("`{}` is not in the definition", request.node));
        };
        match node.parameters.get(KIND_PARAMETER).map(String::as_str) {
            Some("job") => self.perform_job(&request, node).await,
            Some("memory_write") => self.perform_memory_write(&request, node).await,
            _ => refused(format!(
                "no transport is configured for `{}`",
                request.effect.id
            )),
        }
    }

    async fn compensate(&self, request: CompensationRequest<'_>) -> EffectOutcome {
        // Everything an agent's definition declares is irreversible, so the decision
        // core never asks. A definition from elsewhere gets told plainly.
        EffectOutcome::Failed {
            failure: FailureKind::VerifierUnavailable,
            detail: format!(
                "no transport is configured for `{}`, so `{}` cannot be undone here",
                request.effect.id, request.route
            ),
        }
    }
}

/// Forwards each boundary port's value by digest.
///
/// The bytes are not read, because nothing about them changes and a boundary has no
/// opinion about them.
fn pass_through(request: &NodeRequest<'_>) -> Result<NodeOutcome, Failure> {
    let mut outputs = BTreeMap::new();
    for port in &request.node.outputs {
        let name = port.id.to_string();
        let digest = request
            .inputs
            .get(&name)
            .and_then(|input| input.sources.first())
            .and_then(|source| source.digest)
            .ok_or_else(|| {
                Failure::new(
                    FailureKind::MissingEvidence,
                    format!("nothing reached `{name}` on `{}`", request.node.id),
                )
            })?;
        outputs.insert(name, digest);
    }
    Ok(NodeOutcome::finished(outputs))
}

/// The text a node operates on: its first input, as text.
fn first_text(node: &Node, inputs: &BTreeMap<String, Value>) -> Result<String, Failure> {
    let value = inputs.values().next().ok_or_else(|| {
        Failure::new(
            FailureKind::MissingEvidence,
            format!("`{}` was given nothing to work on", node.id),
        )
    })?;
    Ok(value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_string))
}

fn parameter_number(node: &Node, name: &str) -> Option<usize> {
    node.parameters
        .get(name)
        .and_then(|value| value.parse().ok())
}

fn model_failure(provider: &str, model: &str, error: &ModelError) -> Failure {
    let kind = match error {
        ModelError::Unreachable(_) | ModelError::Rejected { .. } => {
            FailureKind::VerifierUnavailable
        }
        ModelError::Malformed(_) => FailureKind::SchemaMismatch,
    };
    // A provider's error body is recorded as it was sent, but not all of it: the
    // detail lives in the run's log, which an erasure does not rewrite.
    let said = match error {
        ModelError::Rejected { status, body } => format!(
            "refused with status {status}: {}",
            cut(body, MAX_PROVIDER_MESSAGE)
        ),
        ModelError::Unreachable(_) | ModelError::Malformed(_) => error.to_string(),
    };
    Failure {
        kind,
        detail: format!("{provider} model `{model}`: {said}"),
        usage: Some(Box::new(NodeUsage {
            provider: Some(provider.to_string()),
            model: Some(model.to_string()),
            ..NodeUsage::default()
        })),
    }
}

/// At most `limit` bytes of `text`, cut on a character boundary.
fn cut(text: &str, limit: usize) -> &str {
    if text.len() <= limit {
        return text;
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Collapses runs of whitespace and trims. The same text in always comes out the same.
fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Fills `{{name}}` from the values, in one pass. Returns the name it could not fill.
fn fill_template(template: &str, values: &BTreeMap<String, String>) -> Result<String, String> {
    let mut filled = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        filled.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            filled.push_str(&rest[start..]);
            return Ok(filled);
        };
        let name = after[..end].trim();
        let Some(value) = values.get(name) else {
            return Err(name.to_string());
        };
        filled.push_str(value);
        rest = &after[end + 2..];
    }
    filled.push_str(rest);
    Ok(filled)
}

fn words(text: &str) -> BTreeSet<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn overlap(wanted: &BTreeSet<String>, text: &str) -> usize {
    words(text).intersection(wanted).count()
}

fn claim_text(claim: &Claim) -> String {
    format!(
        "{} {} {}",
        claim.subject_id().as_str(),
        claim.predicate(),
        claim.object()
    )
}

/// Runs a validator's `rule.*` parameters.
///
/// Returns what failed. An unknown rule is an error rather than a pass: a check that
/// quietly did not run is worse than one that was never declared.
fn run_rules(parameters: &BTreeMap<String, String>, subject: &str) -> Result<Vec<String>, String> {
    let mut problems = Vec::new();
    let count = subject.chars().count();
    for (name, value) in parameters {
        let Some(rule) = name.strip_prefix("rule.") else {
            continue;
        };
        let bound = || {
            value
                .parse::<usize>()
                .map_err(|_| format!("`{name}` must be a whole number, not `{value}`"))
        };
        match rule {
            "non_empty" => {
                if subject.trim().is_empty() {
                    problems.push("the value is empty".to_string());
                }
            }
            "min_chars" => {
                let minimum = bound()?;
                if count < minimum {
                    problems.push(format!("{count} characters is under the minimum {minimum}"));
                }
            }
            "max_chars" => {
                let maximum = bound()?;
                if count > maximum {
                    problems.push(format!("{count} characters is over the maximum {maximum}"));
                }
            }
            "contains" => {
                if !subject.contains(value.as_str()) {
                    problems.push(format!("the value does not contain `{value}`"));
                }
            }
            "not_contains" => {
                if subject.contains(value.as_str()) {
                    problems.push(format!("the value contains `{value}`"));
                }
            }
            "valid_json" => {
                if let Err(error) = serde_json::from_str::<Value>(subject) {
                    problems.push(format!("the value is not valid JSON: {error}"));
                }
            }
            other => return Err(format!("no validator rule is called `{other}`")),
        }
    }
    Ok(problems)
}

/// Builds a candidate claim from what a node was given.
fn claim_from_value(id: &str, tenant: &str, project: &str, value: &Value) -> Result<Claim, String> {
    let text = |field: &str| {
        value
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("a claim to write needs `{field}`"))
    };
    let evidence = value
        .get("evidence_ids")
        .and_then(Value::as_array)
        .ok_or_else(|| "a claim to write needs `evidence_ids`".to_string())?
        .iter()
        .map(|each| {
            each.as_str()
                .ok_or_else(|| "`evidence_ids` must be text".to_string())
                .and_then(|each| EvidenceId::new(each).map_err(|error| error.clone()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Claim::new(
        ClaimId::new(id).map_err(|error| error.clone())?,
        MemoryScope::new(tenant, project).map_err(|error| error.to_string())?,
        EntityId::new(text("subject_id")?).map_err(|error| error.clone())?,
        text("predicate")?,
        text("object")?,
        evidence,
        // What a model wrote is the least-trusted thing the memory holds, until a
        // reviewer says otherwise.
        Confidence::new(0.5).map_err(|error| error.to_string())?,
        Authority::Low,
        text("observed_at")?,
        None,
        None,
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_template_fills_each_name_once_and_never_expands_a_value() {
        let values = BTreeMap::from([
            ("query".to_string(), "{{secret}}".to_string()),
            ("secret".to_string(), "hunter2".to_string()),
        ]);
        assert_eq!(
            fill_template("Q: {{query}}", &values).expect("fills"),
            "Q: {{secret}}",
            "a value containing a placeholder stays text"
        );
    }

    #[test]
    fn a_template_naming_no_port_is_refused() {
        let values = BTreeMap::new();
        assert_eq!(
            fill_template("Q: {{query}}", &values),
            Err("query".to_string())
        );
    }

    #[test]
    fn normalizing_is_stable() {
        assert_eq!(normalize("  a\t b\n\nc "), "a b c");
        assert_eq!(normalize(&normalize("  a\t b ")), "a b");
    }

    #[test]
    fn rules_report_every_problem() {
        let parameters = BTreeMap::from([
            ("rule.contains".to_string(), "cite".to_string()),
            ("rule.max_chars".to_string(), "3".to_string()),
            ("template".to_string(), "not a rule".to_string()),
        ]);
        let problems = run_rules(&parameters, "too long").expect("known rules");
        assert_eq!(problems.len(), 2);
        assert!(run_rules(&parameters, "cit").expect("known rules").len() == 1);
    }

    #[test]
    fn an_unknown_rule_is_an_error_rather_than_a_pass() {
        let parameters = BTreeMap::from([("rule.vibes".to_string(), "good".to_string())]);
        assert!(run_rules(&parameters, "anything").is_err());
    }

    #[test]
    fn a_rule_with_a_bad_bound_is_an_error() {
        let parameters = BTreeMap::from([("rule.max_chars".to_string(), "lots".to_string())]);
        assert!(run_rules(&parameters, "anything").is_err());
    }

    #[test]
    fn valid_json_means_it_parses() {
        let parameters = BTreeMap::from([("rule.valid_json".to_string(), "true".to_string())]);
        assert!(
            run_rules(&parameters, r#"{"a":1}"#)
                .expect("known")
                .is_empty()
        );
        assert_eq!(run_rules(&parameters, "{a:1}").expect("known").len(), 1);
    }

    #[test]
    fn overlap_counts_shared_words_regardless_of_case() {
        assert_eq!(
            overlap(&words("What is CAPSULET?"), "capsulet is a platform"),
            2
        );
        assert_eq!(overlap(&words("anything"), "nothing here"), 0);
    }
}
