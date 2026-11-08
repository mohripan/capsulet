//! Shared tracing, metrics, and request-correlation utilities.

use std::{
    collections::{BTreeSet, HashMap},
    env,
    sync::{Mutex, OnceLock},
    time::Duration,
};

use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use opentelemetry::{KeyValue, global, propagation::TextMapPropagator, trace::TraceContextExt};
use opentelemetry_sdk::{Resource, propagation::TraceContextPropagator, trace::SdkTracerProvider};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

pub use metrics;
pub use tracing;

pub mod genai;
#[cfg(feature = "testing")]
pub mod testing;

static PROMETHEUS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();
static TRACER_PROVIDER: OnceLock<Mutex<Option<SdkTracerProvider>>> = OnceLock::new();

/// Initializes JSON logs, Prometheus metrics, and optional OTLP traces.
///
/// OTLP export is enabled when `OTEL_EXPORTER_OTLP_ENDPOINT` is set. The
/// standard OTEL protocol environment variables are honored by the exporter, and
/// the standard sampler variables (`OTEL_TRACES_SAMPLER`, `OTEL_TRACES_SAMPLER_ARG`)
/// and batch processor variables (`OTEL_BSP_*`) by the SDK.
///
/// Export runs on the batch processor's own thread and never blocks the code being
/// traced: when its queue is full a span is dropped and counted, not waited for. That
/// is deliberate. Telemetry describes execution and nothing reads it back, so a
/// collector that is down costs spans, never an outcome.
///
/// # Errors
///
/// Returns an error when the metrics recorder, tracing subscriber, or OTLP
/// exporter cannot be installed.
pub fn init(service_name: &'static str) -> Result<(), Box<dyn std::error::Error>> {
    install_prometheus_recorder()?;

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        let level = env::var("CAPSULET_LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
        EnvFilter::new(format!("capsulet={level},{level}"))
    });
    let fmt_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_current_span(true)
        .with_span_list(true);

    if env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok() {
        let exporter = opentelemetry_otlp::SpanExporter::builder().build()?;
        let provider = SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(
                Resource::builder()
                    .with_service_name(service_name)
                    .with_attribute(KeyValue::new(
                        genai::SEMCONV_RESOURCE_ATTRIBUTE,
                        genai::SEMCONV_VERSION,
                    ))
                    .build(),
            )
            .build();
        global::set_tracer_provider(provider.clone());
        let tracer = global::tracer(service_name);
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .with(tracing_opentelemetry::layer().with_tracer(tracer))
            .try_init()?;
        let _ = TRACER_PROVIDER.set(Mutex::new(Some(provider)));
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .try_init()?;
    }

    tracing::info!(service.name = service_name, "observability initialized");
    Ok(())
}

/// Flushes the tracer provider if OTLP tracing was enabled.
pub fn shutdown() {
    let Some(provider) = TRACER_PROVIDER.get() else {
        return;
    };
    let Ok(mut provider) = provider.lock() else {
        return;
    };
    if let Some(provider) = provider.take()
        && let Err(error) = provider.shutdown()
    {
        tracing::warn!(%error, "failed to shut down tracer provider");
    }
}

/// The current span's trace context as a W3C `traceparent`, when there is one.
///
/// `None` when tracing is not exporting, or the current span is not sampled into a
/// trace: there is nothing for anything later to continue.
#[must_use]
pub fn current_trace_parent() -> Option<String> {
    let context = tracing::Span::current().context();
    if !context.span().span_context().is_valid() {
        return None;
    }
    let mut carrier = HashMap::new();
    TraceContextPropagator::new().inject_context(&context, &mut carrier);
    carrier.remove("traceparent")
}

/// Makes `span` continue the trace a stored `traceparent` names.
///
/// Has to be called before the span is first entered, because that is when its
/// parent is fixed. A missing or malformed value leaves the span where it was, as the
/// root of a trace of its own: correlation is worth having, and not worth failing
/// anything over.
pub fn continue_trace(span: &tracing::Span, trace_parent: Option<&str>) {
    let Some(trace_parent) = trace_parent else {
        return;
    };
    let carrier = HashMap::from([("traceparent".to_string(), trace_parent.to_string())]);
    let context = TraceContextPropagator::new().extract(&carrier);
    if context.span().span_context().is_valid() {
        let _ = span.set_parent(context);
    }
}

/// The label a [`BoundedLabel`] answers once it is full.
pub const OVERFLOW_LABEL: &str = "_other";

/// A metric label whose values come from outside, held to a fixed number of them.
///
/// The first `limit` distinct values are passed through and everything after them is
/// [`OVERFLOW_LABEL`]. A model name is the case it is for: it comes from a definition,
/// so a tenant decides how many there are, and an unbounded label is a way to make a
/// metrics backend fall over. Traces carry the exact value either way.
#[derive(Debug)]
pub struct BoundedLabel {
    limit: usize,
    seen: Mutex<BTreeSet<String>>,
}

impl BoundedLabel {
    /// A label that passes through at most `limit` distinct values.
    #[must_use]
    pub const fn new(limit: usize) -> Self {
        Self {
            limit,
            seen: Mutex::new(BTreeSet::new()),
        }
    }

    /// The label to use for `value`.
    #[must_use]
    pub fn label(&self, value: &str) -> String {
        let Ok(mut seen) = self.seen.lock() else {
            return OVERFLOW_LABEL.to_string();
        };
        if seen.contains(value) {
            return value.to_string();
        }
        if seen.len() < self.limit {
            seen.insert(value.to_string());
            return value.to_string();
        }
        OVERFLOW_LABEL.to_string()
    }
}

/// Renders process-local Prometheus metrics collected through the `metrics` crate.
#[must_use]
pub fn render_metrics() -> String {
    PROMETHEUS_HANDLE
        .get()
        .map_or_else(String::new, PrometheusHandle::render)
}

/// Returns the incoming request ID or creates a new UUID-backed one.
#[must_use]
pub fn request_id(existing: Option<&str>) -> String {
    existing
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map_or_else(|| Uuid::new_v4().to_string(), str::to_owned)
}

/// Records one completed HTTP request using RED-style metric names.
pub fn record_http_request(method: &str, path: &str, status: u16, duration: Duration) {
    let status_class = status_class(status);
    metrics::counter!(
        "capsulet_http_requests_total",
        "method" => method.to_string(),
        "route" => path.to_string(),
        "status_class" => status_class,
    )
    .increment(1);
    if status >= 500 {
        metrics::counter!(
            "capsulet_http_request_errors_total",
            "method" => method.to_string(),
            "route" => path.to_string(),
            "status_class" => status_class,
        )
        .increment(1);
    }
    metrics::histogram!(
        "capsulet_http_request_duration_seconds",
        "method" => method.to_string(),
        "route" => path.to_string(),
        "status_class" => status_class,
    )
    .record(duration.as_secs_f64());
}

/// Records a service loop/tick outcome.
pub fn record_service_tick(component: &str, outcome: &str, duration: Duration) {
    metrics::counter!(
        "capsulet_service_ticks_total",
        "component" => component.to_string(),
        "outcome" => outcome.to_string(),
    )
    .increment(1);
    metrics::histogram!(
        "capsulet_service_tick_duration_seconds",
        "component" => component.to_string(),
        "outcome" => outcome.to_string(),
    )
    .record(duration.as_secs_f64());
}

pub(crate) fn install_prometheus_recorder() -> Result<(), Box<dyn std::error::Error>> {
    if PROMETHEUS_HANDLE.get().is_some() {
        return Ok(());
    }
    let handle = PrometheusBuilder::new().install_recorder()?;
    let _ = PROMETHEUS_HANDLE.set(handle);
    Ok(())
}

fn status_class(status: u16) -> &'static str {
    match status {
        100..=199 => "1xx",
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        500..=599 => "5xx",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::{BoundedLabel, OVERFLOW_LABEL, continue_trace, current_trace_parent};

    #[test]
    fn a_bounded_label_passes_through_only_its_first_values() {
        let label = BoundedLabel::new(2);
        assert_eq!(label.label("a"), "a");
        assert_eq!(label.label("b"), "b");
        assert_eq!(label.label("c"), OVERFLOW_LABEL);
        assert_eq!(label.label("a"), "a");
    }

    #[test]
    fn without_an_exporter_there_is_no_trace_to_continue() {
        assert_eq!(current_trace_parent(), None);
        // And continuing one that is malformed is not an error.
        continue_trace(&tracing::Span::current(), Some("not a traceparent"));
    }
}
