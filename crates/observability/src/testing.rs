//! Capturing what a process would export, for tests.
//!
//! Each capture installs a subscriber for the current thread only, so a test on a
//! current-thread runtime sees exactly the spans its own work produced. Spans come
//! back as plain strings rather than OpenTelemetry types, which keeps a test's
//! assertions about names and values instead of about an SDK's data model.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use opentelemetry::trace::{SpanId, SpanKind, Status, TracerProvider as _};
use opentelemetry_sdk::error::{OTelSdkError, OTelSdkResult};
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider, SpanData, SpanExporter};
use tracing::Dispatch;
use tracing::subscriber::DefaultGuard;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;

/// One finished span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedSpan {
    pub name: String,
    pub trace_id: String,
    pub span_id: String,
    /// `None` for the root of a trace.
    pub parent_span_id: Option<String>,
    /// `client`, `internal`, and so on.
    pub kind: String,
    /// The status description, when the span ended in error.
    pub error: Option<String>,
    pub attributes: BTreeMap<String, String>,
    /// Each event, rendered as its name followed by its attributes.
    pub events: Vec<String>,
}

impl CapturedSpan {
    /// One attribute's value.
    #[must_use]
    pub fn attribute(&self, key: &str) -> Option<&str> {
        self.attributes.get(key).map(String::as_str)
    }

    /// Everything the span would export, as one string to search.
    #[must_use]
    pub fn rendered(&self) -> String {
        let mut text = format!("{} {:?}", self.name, self.error);
        for (key, value) in &self.attributes {
            let _ = write!(text, " {key}={value}");
        }
        for event in &self.events {
            let _ = write!(text, " {event}");
        }
        text
    }
}

/// Spans recorded on this thread for as long as the capture is held.
pub struct TraceCapture {
    exporter: InMemorySpanExporter,
    provider: SdkTracerProvider,
    _guard: DefaultGuard,
}

impl TraceCapture {
    /// Starts capturing.
    #[must_use]
    pub fn start() -> Self {
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let guard = install(&provider);
        Self {
            exporter,
            provider,
            _guard: guard,
        }
    }

    /// Every span that has finished so far.
    #[must_use]
    pub fn spans(&self) -> Vec<CapturedSpan> {
        let _ = self.provider.force_flush();
        self.exporter
            .get_finished_spans()
            .unwrap_or_default()
            .iter()
            .map(captured)
            .collect()
    }
}

/// A subscriber on this thread whose every export fails, the way it does when the
/// collector is down.
pub struct FailingExport {
    attempts: Arc<AtomicUsize>,
    _provider: SdkTracerProvider,
    _guard: DefaultGuard,
}

impl FailingExport {
    /// Starts failing.
    #[must_use]
    pub fn start() -> Self {
        let attempts = Arc::new(AtomicUsize::new(0));
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(Unreachable {
                attempts: Arc::clone(&attempts),
            })
            .build();
        let guard = install(&provider);
        Self {
            attempts,
            _provider: provider,
            _guard: guard,
        }
    }

    /// How many exports were attempted and refused.
    #[must_use]
    pub fn attempts(&self) -> usize {
        self.attempts.load(Ordering::SeqCst)
    }
}

/// Installs the process's metrics recorder, so a test can render what was recorded.
///
/// # Errors
///
/// Returns an error when another recorder is already installed.
pub fn install_metrics() -> Result<(), Box<dyn std::error::Error>> {
    crate::install_prometheus_recorder()
}

#[derive(Debug)]
struct Unreachable {
    attempts: Arc<AtomicUsize>,
}

impl SpanExporter for Unreachable {
    async fn export(&self, _batch: Vec<SpanData>) -> OTelSdkResult {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        Err(OTelSdkError::InternalFailure(
            "the collector is unreachable".to_string(),
        ))
    }
}

/// A dispatcher kept registered for the life of the process.
///
/// `tracing-core` caches whether each callsite is of interest. While exactly one
/// dispatcher is registered, it decides that for a callsite by asking only the
/// dispatcher of whichever thread hits the callsite first. In a test binary that
/// thread is often another test's, with no subscriber, and the callsite is then cached
/// as of no interest for every thread: a capture on this thread silently sees
/// nothing. With a second dispatcher always registered, every callsite is decided by
/// asking all of them.
static ANCHOR: OnceLock<Dispatch> = OnceLock::new();

fn install(provider: &SdkTracerProvider) -> DefaultGuard {
    ANCHOR.get_or_init(|| Dispatch::new(tracing_subscriber::registry().with(LevelFilter::INFO)));
    let subscriber = tracing_subscriber::registry()
        .with(LevelFilter::INFO)
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("capsulet-test")));
    tracing::subscriber::set_default(subscriber)
}

fn captured(span: &SpanData) -> CapturedSpan {
    let parent = span.parent_span_id;
    CapturedSpan {
        name: span.name.to_string(),
        trace_id: span.span_context.trace_id().to_string(),
        span_id: span.span_context.span_id().to_string(),
        parent_span_id: (parent != SpanId::INVALID).then(|| parent.to_string()),
        kind: match span.span_kind {
            SpanKind::Client => "client",
            SpanKind::Server => "server",
            SpanKind::Producer => "producer",
            SpanKind::Consumer => "consumer",
            SpanKind::Internal => "internal",
        }
        .to_string(),
        error: match &span.status {
            Status::Error { description } => Some(description.to_string()),
            Status::Ok | Status::Unset => None,
        },
        attributes: span
            .attributes
            .iter()
            .map(|pair| {
                (
                    pair.key.as_str().to_string(),
                    pair.value.as_str().to_string(),
                )
            })
            .collect(),
        events: span
            .events
            .iter()
            .map(|event| {
                let mut text = event.name.to_string();
                for pair in &event.attributes {
                    let _ = write!(text, " {}={}", pair.key.as_str(), pair.value.as_str());
                }
                text
            })
            .collect(),
    }
}
