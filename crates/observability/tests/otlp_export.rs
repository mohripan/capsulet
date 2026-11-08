//! Spans leave the process the way a deployment sends them.
//!
//! The parent test listens on a local port, standing in for a collector, and runs
//! this binary again as a child with `OTEL_EXPORTER_OTLP_ENDPOINT` pointing at it.
//! The child initialises observability exactly as a service does, records a span, and
//! shuts down. What arrives is checked as bytes: OTLP over HTTP carries strings as
//! they are, so a span name or an attribute value is findable without decoding.
//!
//! The child is a separate process because the endpoint is read from the environment
//! and the global subscriber can be installed once per process.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::time::Duration;

const CHILD: &str = "CAPSULET_OTLP_EXPORT_CHILD";

#[test]
#[ignore = "run as a child of `spans_reach_a_collector_over_otlp_http`"]
fn export_one_span_and_shut_down() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    capsulet_observability::init("capsulet-otlp-export-test").expect("observability starts");
    {
        let span = tracing::info_span!(
            "capsulet.lease",
            capsulet.run_id = "run_otlp_export",
            "error.type" = "storage"
        );
        let _entered = span.enter();
    }
    // Flushes the batch processor; a service does the same on its way out.
    capsulet_observability::shutdown();
}

#[test]
fn spans_reach_a_collector_over_otlp_http() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let address = listener.local_addr().expect("an address");
    let collector = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("the exporter connects");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("a timeout");
        let mut received = Vec::new();
        let mut chunk = [0_u8; 8192];
        // Read the head, then as much body as it declares.
        loop {
            let read = stream.read(&mut chunk).expect("the request is readable");
            if read == 0 {
                break;
            }
            received.extend_from_slice(&chunk[..read]);
            if let Some(end) = find(&received, b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&received[..end]).to_ascii_lowercase();
                let length = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if received.len() >= end + 4 + length {
                    break;
                }
            }
        }
        stream
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n")
            .expect("the answer is writable");
        received
    });

    let child = Command::new(std::env::current_exe().expect("this test binary"))
        .args([
            "--exact",
            "export_one_span_and_shut_down",
            "--ignored",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .env("OTEL_EXPORTER_OTLP_ENDPOINT", format!("http://{address}"))
        .env("OTEL_EXPORTER_OTLP_PROTOCOL", "http/protobuf")
        .output()
        .expect("the child runs");
    assert!(
        child.status.success(),
        "the child failed: {}",
        String::from_utf8_lossy(&child.stderr)
    );

    let received = collector.join().expect("the collector finished");
    let head = String::from_utf8_lossy(&received[..find(&received, b"\r\n\r\n").unwrap_or(0)]);
    assert!(head.starts_with("POST /v1/traces"), "{head}");
    for expected in [
        "capsulet.lease",
        "run_otlp_export",
        "capsulet-otlp-export-test",
        "capsulet.telemetry.genai_semconv",
        capsulet_observability::genai::SEMCONV_VERSION,
    ] {
        assert!(
            find(&received, expected.as_bytes()).is_some(),
            "`{expected}` was not exported"
        );
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
