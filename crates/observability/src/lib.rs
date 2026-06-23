//! Shared tracing, metrics, and request-correlation utilities.

use std::{
    env,
    sync::{Mutex, OnceLock},
    time::Duration,
};

use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use opentelemetry::global;
use opentelemetry_sdk::{Resource, trace::SdkTracerProvider};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

pub use metrics;
pub use tracing;

static PROMETHEUS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();
static TRACER_PROVIDER: OnceLock<Mutex<Option<SdkTracerProvider>>> = OnceLock::new();

/// Initializes JSON logs, Prometheus metrics, and optional OTLP traces.
///
/// OTLP export is enabled when `OTEL_EXPORTER_OTLP_ENDPOINT` is set. The
/// standard OTEL protocol environment variables are honored by the exporter.
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
            .with_resource(Resource::builder().with_service_name(service_name).build())
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

fn install_prometheus_recorder() -> Result<(), Box<dyn std::error::Error>> {
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
