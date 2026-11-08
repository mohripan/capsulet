//! What the worker tells an operator about a run, and what it never tells them.
//!
//! Spans and metrics describe execution. Nothing reads them back: no decision, no
//! retry and no verdict depends on whether a span was exported, so a collector that
//! is down costs spans and never changes an outcome. Every clock read here is for a
//! duration an operator sees, never for anything the run records.
//!
//! What a span carries is identity, shape and accounting: which run, which node,
//! which attempt, which model, how many tokens, how it ended. What it never carries
//! is content. A prompt, a completion, a retrieved document, an input value or a
//! failure's detail stays in the run's log and value store, where the project's
//! access rules apply. A failure is described by its kind, which is a closed set,
//! rather than by its message, which can quote the value that failed.
//!
//! Attribute names follow the generative-AI semantic conventions at
//! [`capsulet_observability::genai::SEMCONV_VERSION`] where one applies, and use
//! the `capsulet.` namespace where none does.

use std::time::Instant;

use capsulet_ir::agent::KIND_PARAMETER;
use capsulet_ir::id::Identifier;
use capsulet_ir::node::Node;
use capsulet_observability::tracing::{self, Span, field};
use capsulet_observability::{self as observability, BoundedLabel, genai, metrics};
use capsulet_postgres::IrRunRecord;
use capsulet_runtime::RunState;
use capsulet_runtime::event::NodeUsage;

use crate::execute::{EffectOutcome, NodeOutcome};
use crate::model::ModelError;
use crate::runtime::{Progress, WorkerError};

/// How many distinct model names metrics label before the rest share one label.
const MODEL_LABEL_LIMIT: usize = 32;

/// The most of a definition-supplied string that telemetry carries, in bytes.
const MAX_SUPPLIED_TEXT: usize = 128;

static MODEL_LABELS: BoundedLabel = BoundedLabel::new(MODEL_LABEL_LIMIT);

/// The node kinds the agent executor runs. Anything else is labelled `other`, because
/// a definition can set the parameter to whatever it likes.
const KNOWN_KINDS: &[&str] = &[
    "agent_enter",
    "agent_leave",
    "agent_continue",
    "query_normalizer",
    "prompt_builder",
    "return",
    "llm",
    "planner",
    "embedding",
    "retriever",
    "memory_read",
    "reranker",
    "validator",
    "job",
    "memory_write",
];

/// The span one leased pass over a run is recorded under.
///
/// It continues the trace of the request that started the run, so every lease of a
/// run, on whichever worker, lands in one trace; a run started while nothing was
/// traced gets a trace per lease, joined by `capsulet.run_id`.
pub(crate) fn lease_span(lease: &IrRunRecord, worker: &str) -> Span {
    let span = tracing::info_span!(
        "capsulet.lease",
        capsulet.tenant_id = %lease.tenant_id,
        capsulet.project_id = %lease.project_id,
        capsulet.run_id = %lease.id,
        capsulet.definition_digest = %lease.definition_digest,
        capsulet.lease.epoch = lease.epoch.0,
        capsulet.worker_id = worker,
        capsulet.lease.progress = field::Empty,
        capsulet.run.status = field::Empty,
        capsulet.run.spent.tokens = field::Empty,
        capsulet.run.spent.cost_micro_units = field::Empty,
        capsulet.run.spent.effects = field::Empty,
        capsulet.run.spent.unmeasured = field::Empty,
        "error.type" = field::Empty,
        otel.status_code = field::Empty,
        otel.status_description = field::Empty,
    );
    observability::continue_trace(&span, lease.trace_parent.as_deref());
    span
}

/// Records where a run stands, as of the fold just taken.
pub(crate) fn record_state(span: &Span, state: &RunState) {
    let spent = state.spent();
    span.record("capsulet.run.status", state.status().as_str());
    span.record("capsulet.run.spent.tokens", spent.tokens);
    span.record(
        "capsulet.run.spent.cost_micro_units",
        spent.cost_micro_units,
    );
    span.record("capsulet.run.spent.effects", spent.effects);
    span.record("capsulet.run.spent.unmeasured", spent.unmeasured);
}

/// Records how a lease ended.
pub(crate) fn finish_lease(span: &Span, outcome: &Result<Progress, WorkerError>) {
    match outcome {
        Ok(progress) => {
            span.record("capsulet.lease.progress", progress_name(progress));
        }
        Err(error) => {
            let kind = worker_error_kind(error);
            span.record("capsulet.lease.progress", "error");
            span.record("error.type", kind);
            span.record("otel.status_code", "error");
            span.record("otel.status_description", error.to_string());
        }
    }
}

/// The span one node attempt is recorded under.
pub(crate) fn node_span(
    node: &Node,
    iteration: Option<(&Identifier, u32)>,
    started_position: Option<u64>,
) -> Span {
    tracing::info_span!(
        "capsulet.node",
        otel.name = %format!("capsulet.node {}", node.id),
        capsulet.node.id = %node.id,
        capsulet.node.kind = kind_of(node),
        capsulet.loop.region = iteration.map(|(region, _)| region.as_str()),
        capsulet.loop.iteration = iteration.map(|(_, index)| index),
        capsulet.event.started_position = started_position,
        capsulet.event.finished_position = field::Empty,
        capsulet.node.outcome = field::Empty,
        capsulet.usage.tokens = field::Empty,
        capsulet.usage.cost_micro_units = field::Empty,
        capsulet.usage.unmeasured = field::Empty,
        "error.type" = field::Empty,
        otel.status_code = field::Empty,
        otel.status_description = field::Empty,
    )
}

/// Records how a node attempt came out, and counts it.
pub(crate) fn finish_node(span: &Span, node: &Node, outcome: &NodeOutcome) {
    let (outcome_name, failure, usage) = match outcome {
        NodeOutcome::Finished { usage, .. } => ("finished", None, usage.as_ref()),
        NodeOutcome::Failed { failure, usage, .. } => {
            ("failed", Some(failure.as_str()), usage.as_ref())
        }
    };
    span.record("capsulet.node.outcome", outcome_name);
    if let Some(failure) = failure {
        // The kind, never the detail: a detail can quote the value that failed.
        span.record("error.type", failure);
        span.record("otel.status_code", "error");
        span.record("otel.status_description", failure);
    }
    if let Some(usage) = usage {
        record_usage(span, usage);
    }
    metrics::counter!(
        "capsulet_graph_worker_node_attempts_total",
        "kind" => metric_kind(node),
        "outcome" => outcome_name,
        "failure" => failure.unwrap_or("none"),
    )
    .increment(1);
}

/// Records the position the attempt's last event was written at.
pub(crate) fn record_finished_at(span: &Span, position: u64) {
    span.record("capsulet.event.finished_position", position);
}

/// The span one effect attempt is recorded under: an agent acting on the world.
pub(crate) fn effect_span(node: &Node, effect: &Identifier, attempt: u32, recovered: bool) -> Span {
    let tool_type = match node.parameters.get(KIND_PARAMETER).map(String::as_str) {
        Some("job") => Some("extension"),
        Some("memory_write") => Some("datastore"),
        _ => None,
    };
    tracing::info_span!(
        "gen_ai.execute_tool",
        otel.name = %format!("{} {effect}", genai::EXECUTE_TOOL),
        otel.kind = "internal",
        gen_ai.operation.name = genai::EXECUTE_TOOL,
        gen_ai.tool.name = %effect,
        gen_ai.tool.type = tool_type,
        capsulet.node.id = %node.id,
        capsulet.node.kind = kind_of(node),
        capsulet.effect.attempt = attempt,
        capsulet.effect.recovered = recovered,
        capsulet.event.claimed_position = field::Empty,
        capsulet.effect.outcome = field::Empty,
        "error.type" = field::Empty,
        otel.status_code = field::Empty,
        otel.status_description = field::Empty,
    )
}

/// Records the position an effect's claim was written at.
pub(crate) fn record_claimed_at(span: &Span, position: u64) {
    span.record("capsulet.event.claimed_position", position);
}

/// Records how an effect attempt came out, and counts it.
pub(crate) fn finish_effect(span: &Span, node: &Node, outcome: &EffectOutcome) {
    let (outcome_name, error) = match outcome {
        EffectOutcome::Performed {
            node_failure: Some((failure, _)),
            ..
        } => ("performed", Some(failure.as_str())),
        EffectOutcome::Performed { .. } => ("performed", None),
        EffectOutcome::Failed { failure, .. } => ("failed", Some(failure.as_str())),
        // Not known to have happened or not: the claim stays open for the next
        // decision, and an operator should see that as the doubt it is.
        EffectOutcome::Uncertain { .. } => ("uncertain", Some("uncertain")),
    };
    span.record("capsulet.effect.outcome", outcome_name);
    if let Some(error) = error {
        span.record("error.type", error);
        span.record("otel.status_code", "error");
        span.record("otel.status_description", error);
    }
    metrics::counter!(
        "capsulet_graph_worker_effect_attempts_total",
        "kind" => metric_kind(node),
        "outcome" => outcome_name,
    )
    .increment(1);
}

/// One call to a model, from asking to answer.
pub(crate) struct ModelCall {
    span: Span,
    started: Instant,
    operation: &'static str,
    provider: String,
    model: String,
}

impl ModelCall {
    /// Opens the span a model call is recorded under.
    pub(crate) fn start(
        operation: &'static str,
        provider: &str,
        model: &str,
        temperature_milli: Option<u32>,
        max_tokens: Option<u32>,
    ) -> Self {
        // A model name is whatever the agent's definition says, so it is held to a
        // length before it becomes a span name, an attribute or a label.
        let model = bounded(model);
        let span = tracing::info_span!(
            "gen_ai.client",
            otel.name = %format!("{operation} {model}"),
            otel.kind = "client",
            gen_ai.operation.name = operation,
            gen_ai.provider.name = provider,
            gen_ai.request.model = model,
            gen_ai.request.temperature =
                temperature_milli.map(|milli| f64::from(milli) / 1_000.0),
            gen_ai.request.max_tokens = max_tokens,
            gen_ai.usage.input_tokens = field::Empty,
            gen_ai.usage.output_tokens = field::Empty,
            capsulet.usage.latency_ms = field::Empty,
            capsulet.usage.cost_micro_units = field::Empty,
            "error.type" = field::Empty,
            otel.status_code = field::Empty,
            otel.status_description = field::Empty,
        );
        Self {
            span,
            started: Instant::now(),
            operation,
            provider: provider.to_string(),
            model: model.to_string(),
        }
    }

    /// The span to run the call under.
    pub(crate) fn span(&self) -> Span {
        self.span.clone()
    }

    /// Records how the call came out.
    ///
    /// Token counts a provider did not report are left off the span and out of the
    /// token metric, and counted as unmeasured instead: unknown is not zero.
    pub(crate) fn finish(self, outcome: Result<&NodeUsage, &ModelError>) {
        let model = MODEL_LABELS.label(&self.model);
        let elapsed = self.started.elapsed().as_secs_f64();
        match outcome {
            Ok(usage) => {
                self.record_tokens(usage, &model);
                if let Some(latency) = usage.latency_ms {
                    self.span.record("capsulet.usage.latency_ms", latency);
                }
                if let Some(cost) = usage.cost_micro_units {
                    self.span.record("capsulet.usage.cost_micro_units", cost);
                }
                metrics::histogram!(
                    "gen_ai_client_operation_duration_seconds",
                    "gen_ai_operation_name" => self.operation,
                    "gen_ai_provider_name" => self.provider.clone(),
                    "gen_ai_request_model" => model,
                )
                .record(elapsed);
            }
            Err(error) => {
                let kind = model_error_kind(error);
                self.span.record("error.type", kind);
                self.span.record("otel.status_code", "error");
                // Never the response body: a provider can echo the prompt back.
                self.span
                    .record("otel.status_description", model_error_summary(error));
                metrics::histogram!(
                    "gen_ai_client_operation_duration_seconds",
                    "gen_ai_operation_name" => self.operation,
                    "gen_ai_provider_name" => self.provider.clone(),
                    "gen_ai_request_model" => model,
                    "error_type" => kind,
                )
                .record(elapsed);
            }
        }
    }

    fn record_tokens(&self, usage: &NodeUsage, model: &str) {
        let counts = [
            ("input", usage.input_tokens),
            ("output", usage.output_tokens),
        ];
        for (token_type, count) in counts {
            match count {
                Some(count) => {
                    let field = if token_type == "input" {
                        "gen_ai.usage.input_tokens"
                    } else {
                        "gen_ai.usage.output_tokens"
                    };
                    self.span.record(field, count);
                    #[allow(
                        clippy::cast_precision_loss,
                        reason = "a histogram observation; a token count past 2^52 is not a reading anyone needs to the unit"
                    )]
                    let observed = count as f64;
                    metrics::histogram!(
                        "gen_ai_client_token_usage",
                        "gen_ai_operation_name" => self.operation,
                        "gen_ai_provider_name" => self.provider.clone(),
                        "gen_ai_request_model" => model.to_string(),
                        "gen_ai_token_type" => token_type,
                    )
                    .record(observed);
                }
                None => {
                    metrics::counter!(
                        "capsulet_model_usage_unmeasured_total",
                        "gen_ai_operation_name" => self.operation,
                        "gen_ai_provider_name" => self.provider.clone(),
                        "gen_ai_token_type" => token_type,
                    )
                    .increment(1);
                }
            }
        }
    }
}

/// The span a lookup in the project's claims is recorded under.
///
/// The query is not on it, and neither is anything found: both are content.
pub(crate) fn retrieval_span(top_k: usize, window: i64) -> Span {
    #[allow(
        clippy::cast_precision_loss,
        reason = "the convention types top_k as a double, and a top_k past 2^52 is not one"
    )]
    let top_k = top_k as f64;
    tracing::info_span!(
        "gen_ai.retrieval",
        otel.name = %format!("{} capsulet.claims", genai::RETRIEVAL),
        otel.kind = "client",
        gen_ai.operation.name = genai::RETRIEVAL,
        gen_ai.data_source.id = "capsulet.claims",
        gen_ai.request.top_k = top_k,
        capsulet.retrieval.window = window,
        capsulet.retrieval.returned = field::Empty,
        "error.type" = field::Empty,
        otel.status_code = field::Empty,
        otel.status_description = field::Empty,
    )
}

/// Records how many documents a lookup returned, or that it failed.
pub(crate) fn finish_retrieval(span: &Span, returned: Option<usize>) {
    if let Some(returned) = returned {
        span.record("capsulet.retrieval.returned", returned);
    } else {
        span.record("error.type", "storage");
        span.record("otel.status_code", "error");
        span.record("otel.status_description", "storage");
    }
}

/// Records what an attempt spent, by the rule the run's own accounting uses, so the
/// span and the log never disagree about what was measured.
fn record_usage(span: &Span, usage: &NodeUsage) {
    let tokens = usage.known_tokens();
    if let Some(tokens) = tokens {
        span.record("capsulet.usage.tokens", tokens);
    }
    if let Some(cost) = usage.cost_micro_units {
        span.record("capsulet.usage.cost_micro_units", cost);
    }
    span.record(
        "capsulet.usage.unmeasured",
        tokens.is_none() && usage.cost_micro_units.is_none(),
    );
}

fn kind_of(node: &Node) -> &str {
    node.parameters
        .get(KIND_PARAMETER)
        .map_or("none", String::as_str)
}

fn metric_kind(node: &Node) -> &'static str {
    match node.parameters.get(KIND_PARAMETER) {
        None => "none",
        Some(kind) => KNOWN_KINDS
            .iter()
            .find(|known| **known == kind.as_str())
            .copied()
            .unwrap_or("other"),
    }
}

const fn progress_name(progress: &Progress) -> &'static str {
    match progress {
        Progress::NothingToDo => "nothing_to_do",
        Progress::Ended => "ended",
        Progress::Suspended => "suspended",
        Progress::StillRunning => "still_running",
        Progress::LeaseLost => "lease_lost",
    }
}

const fn worker_error_kind(error: &WorkerError) -> &'static str {
    match error {
        WorkerError::Storage(_) => "storage",
        WorkerError::UnknownDefinition { .. } => "unknown_definition",
        WorkerError::Unfoldable { .. } => "unfoldable",
        WorkerError::Undeliverable { .. } => "undeliverable",
        WorkerError::Inconsistent { .. } => "inconsistent",
        WorkerError::ControlMissing { .. } => "control_missing",
    }
}

const fn model_error_kind(error: &ModelError) -> &'static str {
    match error {
        ModelError::Unreachable(_) => "unreachable",
        ModelError::Rejected { .. } => "rejected",
        ModelError::Malformed(_) => "malformed",
    }
}

fn model_error_summary(error: &ModelError) -> String {
    match error {
        ModelError::Rejected { status, .. } => format!("refused with status {status}"),
        ModelError::Unreachable(_) | ModelError::Malformed(_) => error.to_string(),
    }
}

/// At most [`MAX_SUPPLIED_TEXT`] bytes of `value`, cut on a character boundary.
fn bounded(value: &str) -> &str {
    if value.len() <= MAX_SUPPLIED_TEXT {
        return value;
    }
    let mut end = MAX_SUPPLIED_TEXT;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

#[cfg(test)]
mod tests {
    use super::{MAX_SUPPLIED_TEXT, bounded};

    #[test]
    fn supplied_text_is_cut_to_a_bound_on_a_character_boundary() {
        assert_eq!(bounded("small-model"), "small-model");
        let long = "m".repeat(10_000);
        assert_eq!(bounded(&long).len(), MAX_SUPPLIED_TEXT);
        // Two-byte characters, so the bound falls inside one.
        let wide = "é".repeat(100);
        let cut = bounded(&wide);
        assert!(cut.len() <= MAX_SUPPLIED_TEXT);
        assert!(cut.chars().all(|each| each == 'é'));
    }
}
