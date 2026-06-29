# Typed Hypergraph RAG Runtime Implementation Plan

> **For agentic workers:** Implement this task-by-task with test-first changes. The old workflow DAG model is not the design center; treat it as a shape that can be compiled into the new typed hypergraph runtime or removed where replacement is cleaner.

**Goal:** Replace Capsulet's workflow-DAG-centered execution model with a typed hypergraph runtime that can run an open-loop RAG agent through typed nodes, ports, hyperedges, planner-selected actions, state snapshots, budgets, termination policy, providers, and replayable traces.

**Architecture:** `capsulet-core` owns the typed hypergraph domain model and validation. `capsulet-application` owns graph/agent use cases and provider ports. PostgreSQL stores graph definitions, nodes, ports, hyperedges, agent definitions, runs, state snapshots, and trace events. The worker/application runtime advances one durable planner/action cycle at a time. The Python SDK compiles RAG agents into typed hypergraph definitions. The dashboard starts with trace inspection, not visual authoring.

**Tech Stack:** Rust 1.96 workspace, Axum, SQLx/PostgreSQL, Tokio, Next.js/React/TypeScript, Python SDK, Docker Compose.

---

### Task 1: Replace the core graph domain with typed hypergraphs

**Files:**
- Create: `crates/core/src/domain/graph.rs`
- Create: `crates/core/src/domain/agent.rs`
- Modify: `crates/core/src/domain/mod.rs`
- Modify: `crates/core/src/lib.rs`
- Modify or remove: `crates/core/src/domain/workflow_graph.rs`
- Modify: `crates/core/src/domain/workflow.rs`

- [ ] Add failing domain tests for typed nodes, typed ports, multi-source hyperedges, multi-target hyperedges, state-field writes, declared planner actions, transition policies, permitted cycles, forbidden cycles, and deterministic static-policy traversal.
- [ ] Add `GraphId`, `AgentId`, `AgentRunId`, `TraceEventId`, `NodeId`, `PortId`, `HyperedgeId`, `ActionId`, and typed value objects in the domain.
- [ ] Add `GraphDefinition`, `GraphNode`, `GraphPort`, `GraphHyperedge`, `GraphTransitionPolicy`, `AgentDefinition`, `AgentBudget`, `AgentTerminationPolicy`, `AgentRunStatus`, `AgentStateSnapshot`, and `AgentTraceEvent`.
- [ ] Define semantic node kinds for v1 RAG: `planner`, `query_normalizer`, `embedding`, `retriever`, `reranker`, `prompt_builder`, `llm`, `validator`, `memory_read`, `memory_write`, and `return`.
- [ ] Define initial port value types: `UserQuery`, `ConversationContext`, `NormalizedQuery`, `EmbeddingVector`, `RetrievedDocuments`, `RankedDocuments`, `Prompt`, `ModelResponse`, `ValidationResult`, and `FinalAnswer`.
- [ ] Validate that every hyperedge endpoint exists, source endpoints are output ports or readable state fields, target endpoints are input ports or writable state fields, and declared value types are compatible.
- [ ] Validate transition policy separately from dataflow: static deterministic traversal, planner-selected action traversal, cycle allowance, terminal states, and max-step requirements.
- [ ] Add a compatibility compiler from the existing `WorkflowDefinition` shape into a constrained `GraphDefinition`, then decide in later tasks whether workflow-specific domain types stay as facades or are removed.
- [ ] Run `cargo test -p capsulet-core`; expect the new domain tests to pass.

### Task 2: Move application use cases to graph-first services

**Files:**
- Create: `crates/application/src/graphs.rs`
- Create: `crates/application/src/agents.rs`
- Create: `crates/application/src/providers.rs`
- Modify: `crates/application/src/lib.rs`
- Modify: `crates/application/src/ports.rs`
- Modify: `crates/application/src/commands.rs`

- [ ] Add failing application tests with fake repositories and fake providers for graph creation, agent creation, manual run creation, one-cycle advancement, budget stop, validator stop, provider failure, and cancellation.
- [ ] Add graph repository ports for storing graph definitions, nodes, ports, hyperedges, and transition policy as one definition transaction.
- [ ] Add agent repository ports for agent definitions, agent runs, state snapshots, and append-only trace events.
- [ ] Add provider ports for planner, embedding, retrieval, rerank, chat completion, validation, and memory.
- [ ] Add `CreateGraph`, `CreateAgent`, `StartAgentRun`, `AdvanceAgentRun`, `CancelAgentRun`, and `ReadAgentTrace` use cases.
- [ ] Implement one durable advancement contract: load latest state, check budget, call planner or static transition selector, execute selected action, validate output, append trace events, persist new state, decide terminal status.
- [ ] Keep provider implementations fake/in-memory in this task; production adapters come later.
- [ ] Run `cargo test -p capsulet-application`; expect application tests to pass.

### Task 3: Replace persistence with graph and agent tables

**Files:**
- Create: `migrations/20260629190000_typed_graphs_and_agents.sql`
- Modify: `crates/postgres/src/lib.rs`
- Create: `crates/postgres/src/graphs.rs`
- Create: `crates/postgres/src/agents.rs`
- Modify: `crates/postgres/src/rows.rs`
- Modify: `crates/postgres/src/tests.rs`
- Modify or deprecate: `crates/postgres/src/workflows.rs`

- [ ] Add SQLx integration tests for saving/loading graph definitions with nodes, ports, hyperedges, transition policy, and agent bindings.
- [ ] Add tables for graph definitions, graph nodes, graph ports, graph hyperedges, graph transition policies, agent definitions, agent runs, agent state snapshots, and agent trace events.
- [ ] Store structured graph and agent details in normalized rows where the runtime needs querying, and JSONB where shape-specific payloads are versioned or provider-specific.
- [ ] Enforce graph ownership, duplicate IDs, endpoint uniqueness, and cascade deletion with database constraints where practical.
- [ ] Add append-only trace persistence with monotonic sequence numbers per run.
- [ ] Add latest-state lookup by `(agent_run_id, state_version)`.
- [ ] Implement application repository ports in `capsulet-postgres`.
- [ ] Decide whether existing workflow tables are migrated into graph tables or left unused for a short transitional window; document the choice in this plan's implementation notes before proceeding.
- [ ] Run PostgreSQL tests with `CAPSULET_TEST_DATABASE_URL`; expect graph and agent round trips to pass.

### Task 4: Add graph and agent API contracts

**Files:**
- Modify: `crates/api/src/models.rs`
- Modify: `crates/api/src/http.rs` or split into `crates/api/src/http/graphs.rs` and `crates/api/src/http/agents.rs`
- Modify: `crates/api/src/state.rs`
- Modify: `crates/api/src/store.rs`
- Modify: `crates/api/src/tests.rs`
- Modify: `crates/api/openapi.json`
- Modify: `docs/api.md`

- [ ] Add failing API tests for creating graphs, rejecting invalid hyperedges, creating agents, starting manual agent runs, reading run detail, reading trace events, and cancelling runs.
- [ ] Add graph endpoints: `POST /v1/graphs`, `GET /v1/graphs`, `GET /v1/graphs/{id}`, and `POST /v1/graphs/{id}/runs` if useful for non-agent static graphs.
- [ ] Add agent endpoints: `POST /v1/agents`, `GET /v1/agents`, `GET /v1/agents/{id}`, `POST /v1/agents/{id}/runs`, `GET /v1/agent-runs`, `GET /v1/agent-runs/{id}`, `POST /v1/agent-runs/{id}/cancel`, and `GET /v1/agent-runs/{id}/trace`.
- [ ] Map domain validation errors to precise HTTP 400 responses with node, port, hyperedge, provider, budget, or transition-policy context.
- [ ] Keep auth and audit behavior consistent with existing mutation routes.
- [ ] Regenerate or update OpenAPI and docs.
- [ ] Run `cargo test -p capsulet-api`; expect all API tests to pass.

### Task 5: Implement the runtime loop with fake providers

**Files:**
- Create: `crates/worker/src/agent_runtime.rs` or `crates/application/src/agents/runtime.rs`
- Modify: `crates/worker/src/worker.rs`
- Modify: `crates/worker/src/runtime.rs`
- Modify: `crates/worker/src/tests.rs`
- Modify: `crates/runner/src/lib.rs` only if the execution contract needs a graph action boundary

- [ ] Add failing runtime tests for accepted answer, validator rejection followed by another action, max steps, max tokens, max cost, max wall-clock time, provider timeout, invalid provider output, cancellation, and resume after simulated worker loss.
- [ ] Add a poller or reconciler path for queued/running agent runs.
- [ ] Implement fake planner behavior for deterministic tests: choose actions from the graph based on state and scripted provider responses.
- [ ] Implement fake embedding, retrieval, rerank, chat, validation, and memory providers.
- [ ] Append trace events for `run_started`, `planner_called`, `action_selected`, `node_started`, `provider_called`, `node_completed`, `state_updated`, `validator_completed`, `budget_checked`, `run_stopped`, and `run_failed`.
- [ ] Persist state snapshots after each completed action cycle.
- [ ] Ensure worker crash recovery resumes from the latest completed state snapshot and does not replay already-completed trace events as new decisions.
- [ ] Run `cargo test -p capsulet-worker -p capsulet-application`; expect runtime tests to pass.

### Task 6: Replace Python SDK workflow-first authoring with RAG agent compilation

**Files:**
- Modify: `sdk/python/src/capsulet/__init__.py`
- Create: `sdk/python/src/capsulet/agent.py`
- Modify: `sdk/python/src/capsulet/client.py`
- Modify or deprecate: `sdk/python/src/capsulet/workflow.py`
- Create: `sdk/python/tests/test_agent.py`
- Modify: `sdk/python/README.md`
- Create: `examples/agents/rag_agent.py`

- [ ] Add failing SDK tests for `rag_agent` compilation, provider references, budget serialization, termination policy serialization, typed nodes, typed ports, hyperedges, and deterministic JSON output.
- [ ] Add SDK types for provider references, RAG agent builder, `AgentBudget`, termination policy, graph node specs, ports, hyperedges, and compiled `AgentSpec`.
- [ ] Add client methods for creating agents and starting/reading agent runs.
- [ ] Deprecate or rework workflow SDK examples so they do not imply DAGs are the primary model.
- [ ] Add an example RAG agent that uses fake/local provider bindings.
- [ ] Run Python SDK tests; expect all SDK tests to pass.

### Task 7: Update dashboard for agent trace inspection

**Files:**
- Modify: `dashboard/app/lib/api.ts`
- Create: `dashboard/app/agents/page.tsx`
- Create: `dashboard/app/agent-runs/page.tsx`
- Create: `dashboard/app/agent-runs/[id]/page.tsx`
- Create focused components under `dashboard/app/` as needed
- Modify: `dashboard/app/components.tsx`
- Modify: `dashboard/app/globals.css`
- Add or update tests under `dashboard/tests/`

- [ ] Add dashboard tests for listing agents, starting an agent run, viewing trace timeline, viewing state snapshots, viewing provider calls, viewing validator output, and showing terminal stop reason.
- [ ] Add API client types for graphs, agents, agent runs, trace events, state snapshots, provider metadata, and budget summaries.
- [ ] Add agent list and manual run creation UI.
- [ ] Add run detail view with trace timeline, selected event detail, typed state snapshot, provider metadata, budget usage, validator scores, and stop reason.
- [ ] Keep dashboard authoring minimal; do not build a visual graph editor in this task.
- [ ] Run `npm test`, `npm run build`, and relevant Playwright tests in `dashboard`.

### Task 8: Remove or demote old DAG-centered product surfaces

**Files:**
- Modify: `README.md`
- Modify: `ARCHITECTURE.md`
- Modify: `docs/architecture.md`
- Modify: `docs/design/workflow-dag-graph-plan.md`
- Modify: `dashboard/app/workflows/page.tsx`
- Modify: `dashboard/app/automations/page.tsx`
- Modify existing workflow/automation API docs and tests as needed

- [ ] Identify every user-facing page, doc, example, endpoint, and SDK path that presents workflow DAGs as the main product model.
- [ ] Replace product language with typed hypergraph and agent runtime language.
- [ ] Either remove old workflow creation surfaces or make them clearly compile into typed graphs.
- [ ] Update automations language so triggers can target agent/graph runs, not only workflow runs.
- [ ] Keep only compatibility tests that protect intentional migration paths; delete tests that lock in the old DAG model as the core.
- [ ] Run docs link checks or a repository-wide `rg` audit for stale DAG-first language.

### Task 9: Verification and cutover

**Files:**
- Verify: `Cargo.toml`
- Verify: `compose.yaml`
- Verify: `dashboard/package.json`
- Verify: `sdk/python/pyproject.toml`
- Modify deployment or Compose files only if the new runtime requires concrete service wiring.

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`.
- [ ] Run `cargo test --workspace --all-targets --locked`.
- [ ] Run PostgreSQL integration tests with `CAPSULET_TEST_DATABASE_URL`.
- [ ] Run Python SDK tests.
- [ ] Run dashboard unit/build/e2e tests.
- [ ] Start Docker Compose, create a fake-provider RAG agent through the SDK or API, run it, inspect trace events, and verify explicit stop reason.
- [ ] Verify cancellation and resume behavior through API tests or local smoke.
- [ ] Run `helm lint charts/capsulet` and `helm template capsulet charts/capsulet`.
- [ ] Record exact commands, outcomes, and any environmental blockers in the final handoff.

