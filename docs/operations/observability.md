<!-- capsulet-claims: CAP-JOB-001, CAP-WORKFLOW-001, CAP-AUTOMATION-001, CAP-OBSERVABILITY-001, CAP-OBSERVABILITY-002, CAP-HELM-001 -->
# Observability

Capsulet services emit JSON logs and Prometheus metrics by default. Distributed tracing is enabled when `OTEL_EXPORTER_OTLP_ENDPOINT` is set.

## Logs

- `RUST_LOG` controls module filtering.
- `CAPSULET_LOG_LEVEL` is honored when `RUST_LOG` is not set.
- Every API response includes `x-request-id`; incoming `x-request-id` is preserved.

## Metrics

The API, scheduler, worker, evaluator and graph worker expose `/metrics`.

- `capsulet_http_requests_total`
- `capsulet_http_request_errors_total`
- `capsulet_http_request_duration_seconds`
- `capsulet_service_ticks_total`
- `capsulet_service_tick_duration_seconds`
- `capsulet_admission_decisions_total`
- `capsulet_job_runs`
- `capsulet_workflow_runs`
- `capsulet_trigger_events`
- `capsulet_job_queue_depth`
- `capsulet_job_queue_oldest_age_seconds`
- `capsulet_job_lease_age_seconds`
- `capsulet_execution_pool_saturation`
- `capsulet_job_retry_exhausted_runs`
- `capsulet_scheduler_lag_seconds`
- `capsulet_workflow_critical_path_latency_seconds`
- `capsulet_stuck_workflow_runs`
- `capsulet_trigger_runtime_failures`
- `capsulet_graph_worker_node_attempts_total` (`kind`, `outcome`, `failure`)
- `capsulet_graph_worker_effect_attempts_total` (`kind`, `outcome`)
- `gen_ai_client_operation_duration_seconds` (`gen_ai_operation_name`, `gen_ai_provider_name`,
  `gen_ai_request_model`, and `error_type` when the call failed)
- `gen_ai_client_token_usage` (the same, with `gen_ai_token_type` of `input` or `output`)
- `capsulet_model_usage_unmeasured_total` (`gen_ai_operation_name`, `gen_ai_provider_name`,
  `gen_ai_token_type`): calls whose provider did not report that count. A missing count is never
  recorded as zero tokens.

Every label on the graph worker's metrics comes from a closed set, so no run, tenant or project
identifier can become a label. `kind` is one of the node kinds the agent executor runs, or `other`.
`gen_ai_request_model` is the exception that comes from a definition, so it passes through the first
32 distinct model names a process sees and reports every later one as `_other`; the exact model is
always on the trace.

The `capsulet_http_*` metrics are labelled by `method`, `status_class`, and `route`, which is the
route template (`/v1/agent-runs/{id}`), or `unmatched` when no route matched. It is never the request
path: a path carries ids, and a client can send any path it likes, so a label taken from it would
have no bound on its values.

Grafana and alerting starter assets live under `ops/observability/`. Helm can
package the same assets with:

```yaml
serviceMonitor:
  enabled: true
prometheusRules:
  enabled: true
grafanaDashboards:
  enabled: true
```

The dashboard covers API request health, queue/backpressure depth, worker lease
age, pool saturation, workflow latency, stuck workflows, retry exhaustion,
admission rejection, and trigger runtime failures. Alert thresholds are exposed
under `prometheusRules.*` in the chart values.

## Tracing

Set `OTEL_EXPORTER_OTLP_ENDPOINT` to an OTLP collector endpoint. Standard OTEL protocol variables such as `OTEL_EXPORTER_OTLP_PROTOCOL` are passed through to the exporter.

### Sampling and backpressure

Sampling uses the standard SDK variables. The default is `parentbased_always_on`, which keeps every
trace; `OTEL_TRACES_SAMPLER=parentbased_traceidratio` with `OTEL_TRACES_SAMPLER_ARG=0.1` keeps one
in ten. Parent-based sampling matters for agent runs: a worker's spans follow the decision made for
the request that started the run, so a run is traced whole or not at all.

Spans are exported by a batch processor on its own thread. Its queue holds `OTEL_BSP_MAX_QUEUE_SIZE`
spans (2048 by default) and it exports every `OTEL_BSP_SCHEDULE_DELAY` milliseconds (5000) in batches
of `OTEL_BSP_MAX_EXPORT_BATCH_SIZE` (512), giving up on one after `OTEL_BSP_EXPORT_TIMEOUT` (30000).
When the queue is full a new span is dropped and counted, never waited for. Nothing in Capsulet reads
telemetry back, so a collector that is slow or down loses spans and changes no decision, retry or
verdict. `a_collector_that_is_down_changes_nothing_about_how_a_run_ends` in
`crates/graph-worker/tests/agent_runs.rs` runs the same agent with and without every export failing
and compares how each ended.

### Agent run traces

The semantic conventions for generative AI are pinned to
[release 1.41.0](https://github.com/open-telemetry/semantic-conventions/tree/v1.41.0/docs/gen-ai),
the last release of the main conventions repository that carries those pages. Every exported span's
resource carries `capsulet.telemetry.genai_semconv=1.41.0`, so a backend can tell which revision the
attribute names follow.

An agent run outlives the request that started it and is advanced by whichever graph worker leases
it. The run therefore records that request's W3C `traceparent` (`ir_runs.trace_parent`, nothing else
from the request), and every lease continues that trace. One run is one trace however many leases and
workers carry it:

```text
http.request                          POST /v1/agents/{id}/runs
└─ capsulet.lease                     one per lease, on any worker
   ├─ capsulet.node {node}            one per node attempt
   │  ├─ text_completion {model}      a model call (CLIENT)
   │  ├─ embeddings {model}           an embedding call (CLIENT)
   │  └─ retrieval capsulet.claims    a lookup in the project's claims (CLIENT)
   └─ execute_tool {effect}           an effect attempt: a job or a proposed claim (INTERNAL)
```

A run started while nothing was being exported has no stored context, and each of its leases is the
root of a trace of its own. `capsulet.run_id` joins them.

| Span | Attributes |
| --- | --- |
| `capsulet.lease` | `capsulet.tenant_id`, `capsulet.project_id`, `capsulet.run_id`, `capsulet.definition_digest`, `capsulet.lease.epoch`, `capsulet.worker_id`, `capsulet.lease.progress`; and, as of the lease's last fold, `capsulet.run.status` and `capsulet.run.spent.{tokens,cost_micro_units,effects,unmeasured}` |
| `capsulet.node {node}` | `capsulet.node.id`, `capsulet.node.kind`, `capsulet.loop.region`, `capsulet.loop.iteration`, `capsulet.event.started_position`, `capsulet.event.finished_position`, `capsulet.node.outcome`, `capsulet.usage.{tokens,cost_micro_units,unmeasured}` |
| `text_completion {model}`, `embeddings {model}` | `gen_ai.operation.name`, `gen_ai.provider.name`, `gen_ai.request.model`, `gen_ai.request.temperature`, `gen_ai.request.max_tokens`, `gen_ai.usage.input_tokens`, `gen_ai.usage.output_tokens`, `capsulet.usage.latency_ms`, `capsulet.usage.cost_micro_units` |
| `retrieval capsulet.claims` | `gen_ai.operation.name`, `gen_ai.data_source.id`, `gen_ai.request.top_k`, `capsulet.retrieval.window`, `capsulet.retrieval.returned` |
| `execute_tool {effect}` | `gen_ai.operation.name`, `gen_ai.tool.name`, `gen_ai.tool.type` (`extension` for a job, `datastore` for a claim), `capsulet.node.id`, `capsulet.node.kind`, `capsulet.effect.attempt`, `capsulet.effect.recovered`, `capsulet.event.claimed_position`, `capsulet.effect.outcome` |

The `capsulet.event.*` positions are positions in the run's log (`ir_run_events.position`), so a span
leads straight to the durable record of what it describes. The graph worker issues no certificates;
a certificate's subject names the run and the definition digest, which are `capsulet.run_id` and
`capsulet.definition_digest` on every lease.

Token counts a provider did not report are left off the span rather than written as zero, and
`capsulet.usage.unmeasured` says so, by the same rule the run's own accounting uses. A failure is an
error status, and its `error.type` is its kind: a node's or effect's failure kind
(`verifier_unavailable`, `schema_mismatch`, ...) or `uncertain` for an effect nobody can say happened,
which is also the whole status description; a model error kind (`unreachable`, `rejected`,
`malformed`), described without the provider's response body (`refused with status 503`); `storage`
for a retrieval the store refused; or, on a lease, the worker error that stopped it (`storage`,
`unfoldable`, `inconsistent`, ...), described by its message.

### What a trace never carries

No span carries content: not a prompt, a completion, an embedding, a retrieval query or what it
found, an input or state value, a failure's detail, or a provider's error body (which can repeat the
prompt). Those stay in the run's log and value store, where the project's access rules apply. The
conventions' opt-in content attributes (`gen_ai.input.messages`, `gen_ai.output.messages`,
`gen_ai.system_instructions`, `gen_ai.retrieval.query.text`, `gen_ai.tool.call.arguments` and
`gen_ai.tool.call.result`) are not implemented, so there is nothing to switch on. The tests in
`crates/graph-worker/tests/agent_runs.rs` search every exported span for the text a run was given and
produced.

Some attribute values come from a tenant's definition rather than from Capsulet. Node, effect and
region ids are identifiers, so their length and characters are already restricted. A model name is
free text, so it is cut to 128 bytes before it becomes a span name, an attribute or a metric label.

### Changing the conventions

Moving the pin is a deliberate change: `crates/observability/src/genai.rs`, the attribute names in
`crates/graph-worker/src/telemetry.rs`, the tests that assert them and this page change in one
commit, and a renamed attribute is listed here as breaking for dashboards that query it. `capsulet.*`
attributes are only ever added; one is removed or renamed only with a note here.

Not yet traced: the API does not adopt an incoming `traceparent` header, so each request starts its
own trace; runs started through the IR run API rather than as agent runs do not record trace context,
so their leases are traced separately; and the graph worker's metrics are Prometheus only, not OTLP.

### Other service spans

- `http.request` records request ID, method, route template (`http.route`), path (`url.path`), status code, and elapsed milliseconds.
- `worker.drain`, `worker.run_task`, and `worker.tick` record worker loop, task, run, execution pool, and outcome fields.
- `worker.kubernetes_reconcile` records active run counts and orphaned Kubernetes job deletions.
- `scheduler.tick` records triggered automations, advanced workflow runs, and outcome.
- `evaluator.tick`, `evaluator.trigger_group`, and `evaluator.retention_cleanup` record trigger evaluation and retention outcomes.
