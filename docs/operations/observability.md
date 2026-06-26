# Observability

Capsulet services emit JSON logs and Prometheus metrics by default. Distributed tracing is enabled when `OTEL_EXPORTER_OTLP_ENDPOINT` is set.

## Logs

- `RUST_LOG` controls module filtering.
- `CAPSULET_LOG_LEVEL` is honored when `RUST_LOG` is not set.
- Every API response includes `x-request-id`; incoming `x-request-id` is preserved.

## Metrics

The API, scheduler, worker, and evaluator expose `/metrics`.

- `capsulet_http_requests_total`
- `capsulet_http_request_errors_total`
- `capsulet_http_request_duration_seconds`
- `capsulet_service_ticks_total`
- `capsulet_service_tick_duration_seconds`
- Existing database-backed gauges for jobs, workflows, and triggers

Grafana and alerting starter assets live under `deploy/observability/`.

## Tracing

Set `OTEL_EXPORTER_OTLP_ENDPOINT` to an OTLP collector endpoint. Standard OTEL protocol variables such as `OTEL_EXPORTER_OTLP_PROTOCOL` are passed through to the exporter.

Key service spans:

- `http.request` records request ID, method, route, status code, and elapsed milliseconds.
- `worker.drain`, `worker.run_task`, and `worker.tick` record worker loop, task, run, execution pool, and outcome fields.
- `worker.kubernetes_reconcile` records active run counts and orphaned Kubernetes job deletions.
- `scheduler.tick` records triggered automations, advanced workflow runs, and outcome.
- `evaluator.tick`, `evaluator.trigger_group`, and `evaluator.retention_cleanup` record trigger evaluation and retention outcomes.
