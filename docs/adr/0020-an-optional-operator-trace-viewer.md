# ADR 0020: An Optional, Operator-Only Trace Viewer

Status: Accepted

## Context

Agent runs export OpenTelemetry traces over OTLP whenever `OTEL_EXPORTER_OTLP_ENDPOINT` is set
([observability](../operations/observability.md#agent-run-traces)). The model, retrieval and tool
spans follow the generative-AI conventions pinned at 1.41.0, and every lease span carries the run,
project, tenant and definition digest. No span carries content: no prompt, completion, query, value or
failure detail. Somebody looking at those traces needs something to look with, and #105 asked for one
reusable backend to be chosen. It should be chosen by measurement, not built, and never made the
authority for a run's correctness.

Three agent-observability backends were candidates, with two plain trace stores as baselines. Their
licences, dependencies, isolation and retention were read from their own documentation and
repositories on 2026-11-12. Two were then run against the same synthetic agent run
(`scripts/trace-viewer-spike.ps1`):

1. The stack's API and graph worker export OTLP/HTTP to the viewer.
2. A deterministic agent runs with a marker in its input.
3. The run's bundle names its trace id.
4. The viewer is asked for that trace.

| | Jaeger 2.23.0 | MLflow 3.18.0 | Langfuse 4.79 | Phoenix 21.0 |
| --- | --- | --- | --- | --- |
| Licence | Apache-2.0 | Apache-2.0 | MIT core; `ee/` directories under a commercial licence, shipped in the same images | Elastic License 2.0, not OSI-approved; no offering it as a managed service |
| OTLP | gRPC and HTTP, native | HTTP only, at `/v1/traces`, with an `x-mlflow-experiment-id` header | HTTP only, with Basic auth per project | HTTP (protobuf only) and gRPC |
| Self-hosted pieces | one container | one container, or with Postgres and object storage | web, worker, Postgres, Redis, ClickHouse 25.12+, S3; 4 cores, 16 GiB recommended | one container, or with Postgres |
| Image, measured | 54 MB | 289 MB | not run | not run |
| Memory after one run, measured | 15 MiB | 2.4 GiB | | |
| Ready, measured | 0.4 s | 6.6 s | | |
| Found the run's trace by its id, with Capsulet's attributes and none of its content | yes | yes | | |
| Authentication | none | `--app-name basic-auth`, but the official image lacks its `flask_wtf` dependency, so it does not start; two auth bypasses fixed this year (CVE-2026-2652, CVE-2026-8147) | per-project keys; project-level RBAC is Enterprise | instance-wide roles; no project isolation on one instance |
| Retention | a time-to-live in the Badger store; the default memory store forgets on restart | no TTL; a deletion API | per-project retention is Enterprise | per-project, by age or count |
| Calls home | no | anonymised telemetry on by default | telemetry on by default | analytics on by default |

## Decision

**Jaeger is the optional trace viewer, pinned by digest
(`jaegertracing/jaeger:2.23.0@sha256:3823829974ccfc2485d2e48b165579091f32295022ae80ca49704e2f4834116e`),
and it is for operators only.**

- **No candidate isolates projects for free with the images it ships**, and Capsulet does not
  delegate isolation anyway.
  - Jaeger and Phoenix have no project isolation at all.
  - MLflow's workspaces are documented as not a hard boundary, and its only authentication cannot
    run in the official image.
  - Langfuse keeps project-level access and retention for its Enterprise edition.

  So the viewer sits on the operator's network, unauthenticated like most trace stores, and is
  never offered to tenants. A tenant sees a run through Capsulet's API, the run's bundle and its
  page (`capsulet-replay --html`), where the project's access rules apply. A run's trace id is
  disclosed only to a caller allowed to export its bundle, so knowing which trace is whose stays
  behind Capsulet's authorisation.
- **An LLM-specific interface has little to show here.** It earns its weight by showing prompts and
  completions, and Capsulet's spans carry neither. What remains is the trace tree and its
  attributes, which a plain trace store shows as well. The cost difference is two orders of
  magnitude in memory.
- **The integration is plain OTLP.** It uses the standard `OTEL_EXPORTER_OTLP_*` variables and no
  code of Jaeger's. The same spike runs against MLflow by changing a parameter, which is the evidence
  that the contract is exporter-neutral. Any OTLP backend an operator already runs works the same
  way.
- **Nothing depends on it.** It is not in `compose.yaml` or the chart, and no gate in a profile needs
  it. The `trace-viewer` gate, in no profile, runs the spike on demand. Certificates, bundles and the
  run's log remain the record of what a run did; a viewer is a convenience for reading the traces.

**MLflow is deferred to the MLOps milestone**, where its experiment and model lineage is the reason
to adopt it (#108, #110). That evaluation has to start from what this one measured: its memory, the
authentication its official image cannot enable, and its soft workspace boundary.

**Langfuse and Phoenix are rejected.** Langfuse's images carry commercially licensed code, and the
isolation and retention a multi-project installation needs are in that code. Phoenix's licence
restricts the very kind of service a self-hosted platform may be operated as, and it cannot isolate
projects on one instance.

## Consequences

- Operators get a viewer that costs almost nothing to run. `docs/operations/observability.md`
  says how to point the services at it and how to look up a run's trace.
- The viewer adds no authentication or isolation. It is not exposed beyond the operator's
  network, and for persistence the Badger store's time-to-live sets the retention.
- The pin moves deliberately: the image in the spike and the docs change in one commit, and the
  `trace-viewer` gate is run against it.
- If spans ever carried content, which would need an ADR of its own, this decision would have to be
  revisited, because the viewer would then hold data that needs the project boundary it does not
  have.
