# Capsulet

Capsulet is a local-first AI memory platform in progress. It turns documents, conversations, code, and tools into governed graph memory for private AI agents. The implemented foundation now includes typed agent execution graphs plus a claim-first memory substrate: sources, evidence, entities, claims, events, relationships, memory contracts, nested subgraphs, canonical identity, review queues, and contradiction handling. The workflow/job runner stack remains as the deterministic execution substrate for tools and compatibility use cases.

![Capsulet workflow dashboard](docs/images/capsulet-dashboard.png)

## What works today

- typed agent graph definitions with nodes, ports, hyperedges, transition policies, and static execution order
- agent definitions with graph references, step/token/time/cost budgets, and termination policies
- queued agent runs with durable state snapshots and semantic trace events
- application-level agent runtime execution with pluggable node adapters, budget enforcement, failure handling, and validator-pass completion
- claim-first memory records for sources, evidence, entities, claims, events, relationships, and contract DSL summaries
- nested memory subgraphs with owner, schema, permissions, summary-claim, summary-trace, membership, activation, canonical entity, graph attachment, and explicit cross-subgraph edge primitives
- deterministic local ingestion that proposes candidate claims and evidence-backed memory from uploaded text
- human review queues for candidate claims, entity-resolution proposals, and conflicting active claims
- reusable Python job definitions with JSON input schemas and retry policies
- compatibility workflow DAGs with parallel roots, fan-out, and fan-in dependencies
- manual, timezone-aware cron, read-only SQL, signed webhook, and isolated custom-plugin triggers
- bearer authentication with viewer/operator/admin authorization and durable mutation auditing
- durable PostgreSQL agent, graph, job, attempt, workflow-step, log, and artifact metadata
- stub, trusted local process, WASI Python, and Kubernetes Job runners
- S3-compatible or filesystem artifact storage
- enforced execution-pool concurrency, cancellation, timeouts, delayed retry, and stale-lease recovery
- owner-bound worker heartbeats that prevent stale workers from finalizing reassigned work
- workflow resume from successful step checkpoints after failure or timeout
- Kubernetes Job reattachment after worker failure
- API, worker, scheduler, and evaluator health and Prometheus metrics endpoints
- configurable artifact, log, trigger-event, and audit retention cleanup
- Docker Compose for local use and a Helm chart for Kubernetes

## Foundation: Agent Graphs and Memory Graphs

Capsulet is centered on governed AI memory, not plain workflow orchestration. There are two graph layers:

- **Agent execution graph:** the implemented foundation. Nodes describe actions such as prompt building, retrieval, model inference, validation, and memory operations; ports and hyperedges describe how typed values move between nodes and run state.
- **Memory graph:** the governed knowledge layer. It models claims, entities, events, relationships, evidence, permissions, confidence, source authority, contradictions, temporal validity, and nested memory contexts.

An agent definition binds a graph to an enterprise control envelope: max steps, tokens, runtime seconds, cost, and explicit termination conditions. An agent run carries a JSON state document. The runtime executes graph nodes through pluggable adapters, writes state snapshots after node completion, appends trace events, and stops on success, failure, or budget/termination policy.

```text
agent graph -> agent definition -> agent run -> node adapter -> state snapshot
                                      |              |
                                      v              v
                               trace events    model/tool/vector work
```

The memory graph is claim-first. A claim records what was said, who or what it is about, where it came from, when it was observed or valid, its confidence, and its review state. Nested subgraphs provide bounded memory contexts for teams, projects, customers, incidents, or personal modules. Each active subgraph must have an owner, schema, permissions, summary claim, and traceability from that summary back to inner claims or evidence. Canonical entities connect local entities across subgraphs without erasing context-specific disagreement.

```text
raw text -> source/evidence -> candidate claims -> review -> active memory
                                           |             |
                                           v             v
                                  entity resolution   conflict inbox
```

## How execution stays durable

PostgreSQL is the source of truth. Agent graph definitions, agent definitions, agent runs, state snapshots, and trace events are persisted durably. The runtime can be driven by a worker in later slices without changing the graph or run model.

The existing job runner path remains durable as the lower-level execution substrate. A worker atomically leases a queued job, records the attempt, and renews the lease while execution is active. If the worker disappears, the expired lease is requeued. Lease ownership is checked during heartbeat and finalization, so an old worker cannot overwrite a newer attempt.

Compatibility workflow nodes still have durable step runs. Successful nodes are checkpoints: their metadata and artifacts remain complete even when another branch fails. Calling the resume endpoint removes only unsuccessful attempts and lets the scheduler reconstruct the missing runnable nodes from the saved graph state.

## Local Docker Compose

Prerequisites: Docker with Compose v2.

```sh
docker compose up --build -d
docker compose ps
```

Open the dashboard at <http://127.0.0.1:3000> and the API at <http://127.0.0.1:8080>.

The local stack includes PostgreSQL, MinIO, Mailpit, API, worker, scheduler, evaluator, and dashboard. Compose waits on dependency health and restarts long-running services after failure. Sign in with the development token `capsulet-local-admin-token-change-me`; replace it before exposing the stack.

Useful checks:

```sh
curl http://127.0.0.1:8080/livez
curl http://127.0.0.1:8080/readyz
curl http://127.0.0.1:8080/metrics
docker compose logs -f api worker scheduler evaluator
```

Stop the stack without deleting persisted volumes:

```sh
docker compose down
```

## Agent execution graph API

Create typed graphs, bind them to agents, and start agent runs:

```sh
curl -H 'Authorization: Bearer <token>' -X POST http://127.0.0.1:8080/v1/graphs
curl -H 'Authorization: Bearer <token>' -X POST http://127.0.0.1:8080/v1/agents
curl -H 'Authorization: Bearer <token>' -X POST http://127.0.0.1:8080/v1/agents/<agent-id>/runs
```

Read graph, agent, and run state:

```sh
curl -H 'Authorization: Bearer <token>' http://127.0.0.1:8080/v1/graphs
curl -H 'Authorization: Bearer <token>' http://127.0.0.1:8080/v1/agents
curl -H 'Authorization: Bearer <token>' http://127.0.0.1:8080/v1/agent-runs
```

## Memory Studio API

Memory APIs are tenant/project scoped and claim-first. They support direct authoring, deterministic ingestion, review, and nested graph governance:

```sh
curl -H 'Authorization: Bearer <token>' http://127.0.0.1:8080/v1/memory/sources
curl -H 'Authorization: Bearer <token>' http://127.0.0.1:8080/v1/memory/claims
curl -H 'Authorization: Bearer <token>' http://127.0.0.1:8080/v1/ingestion/review/claims?status=candidate
curl -H 'Authorization: Bearer <token>' http://127.0.0.1:8080/v1/memory/entity-resolutions?status=proposed
curl -H 'Authorization: Bearer <token>' http://127.0.0.1:8080/v1/memory/conflicts?status=candidate
```

The dashboard Memory Studio exposes ingestion, claim review, entity resolution, nested graph activation, and the conflict inbox as the first product surface for governing memory before agents consume it.

## Compatibility workflow recovery API

Resume a failed or timed-out workflow run:

```sh
curl -H 'Authorization: Bearer <token>' -X POST http://127.0.0.1:8080/v1/workflow-runs/<run-id>/resume
```

The response contains the workflow run and preserved successful step runs. Active, queued, cancelled, removed, and successful runs are rejected to avoid ambiguous recovery.

## Kubernetes with Helm

The chart installs the API, worker, scheduler, evaluator, dashboard, migration job, separated control-plane/execution service accounts, default-deny execution network policy, services, configuration, and optional bundled PostgreSQL and MinIO.

```sh
helm lint charts/capsulet
kubectl create namespace capsulet
kubectl create secret generic capsulet-api-auth \
  --namespace capsulet \
  --from-literal='tokens=[{"name":"cluster-admin","role":"admin","token":"replace-with-at-least-32-random-characters"}]'
helm install capsulet charts/capsulet \
  --namespace capsulet \
  --set api.auth.existingSecret=capsulet-api-auth

kubectl wait --for=condition=available deployment \
  --all --namespace capsulet --timeout=5m
kubectl port-forward service/capsulet-dashboard 3000:80 --namespace capsulet
```

Production deployments should use external managed PostgreSQL and object storage, immutable image tags, network policies, and dedicated execution capacity. Capsulet constrains execution pods but does not claim to be a complete sandbox for hostile code.

See [installation](docs/installation.md), [Helm values](docs/helm-values.md), and [worker/runner design](docs/worker-runner.md) for configuration details.

## Repository layout

```text
crates/
  application/  application services, agent runtime use cases, and ports
  api/          HTTP control plane
  core/         domain types, state machines, and validation rules
  postgres/     SQLx persistence and migrations
  runner/       runner contracts plus stub, process, WASI, and Kubernetes adapters
  scheduler/    compatibility automation triggering and DAG reconciliation
  evaluator/    durable trigger evaluation and retention cleanup
  worker/       lower-level job runtime loop, runner selection, and health endpoints
  storage/      filesystem and S3-compatible object storage
  cli/          command-line client
dashboard/      Next.js dashboard
sdk/python/     decorator-based Python workflow SDK
charts/capsulet Helm chart
migrations/     PostgreSQL schema history
```

## Development and verification

### Python workflow authoring

Python workflows are still available as a compatibility authoring path and as a useful substrate for deterministic agent tools. They can be authored as decorated Python functions or as Python cells in the dashboard notebook. The [CSV artifact pipeline](examples/workflows/csv_pipeline.py) creates a CSV in one task, passes it to a dependent task, and downloads the transformed artifact. See the [example instructions](examples/workflows/README.md) for the end-to-end commands.

Rust is pinned to version 1.96.0. The dashboard requires Node.js 20 or newer.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --locked

cd dashboard
npm ci
npm test
npm run build
npm run test:e2e
```

Database integration tests run when `CAPSULET_TEST_DATABASE_URL` is set. Full local and Kubernetes validation steps are documented in [development](docs/development.md).

## Documentation

- [Architecture](ARCHITECTURE.md)
- [API](docs/api.md)
- [Development](docs/development.md)
- [Installation](docs/installation.md)
- [Persistence](docs/persistence.md)

## License

Apache-2.0. See [LICENSE](LICENSE).
