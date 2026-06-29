# Typed Agent RAG Runtime Design

## Context

Capsulet is currently a Kubernetes-native automation control plane for durable Python jobs, workflow DAGs, triggers, logs, artifacts, retries, execution pools, and operational inspection. The project has not been published yet, so the product direction can change without preserving every internal API.

The current DAG model is useful for deterministic jobs, but it is not enough for AI agent behavior. A RAG agent needs typed dataflow, shared state, provider calls, validation, dynamic next-action decisions, bounded loops, and replayable traces. The new direction is to evolve Capsulet into an AI agent orchestration control plane while reusing the durable execution substrate that already exists.

## Goals

- Add a new typed `AgentDefinition` product layer for AI-native cyclic graphs.
- Build the first milestone around an open-loop RAG answer agent.
- Keep Python SDK authoring as the primary v1 authoring surface.
- Support hybrid providers: local/mock adapters for development and production bindings through configuration.
- Require every agent run to have both budget limits and semantic termination criteria.
- Record replayable, inspectable agent traces with state snapshots, provider metadata, validator results, and stop reasons.
- Reuse existing durability, worker execution, cancellation, logs, artifacts, auth, and dashboard inspection where practical.

## Non-Goals

- Do not replace existing workflow DAGs. Workflows remain the deterministic job orchestration model.
- Do not build a full visual graph authoring experience in v1.
- Do not hard-code one LLM, embedding model, vector database, reranker, validator, or memory backend into the domain model.
- Do not implement a mathematically general hypergraph engine in the first milestone.
- Do not make the model decide arbitrary code execution targets outside graph-declared actions.
- Do not make enterprise governance complete in v1. The architecture should allow policy, audit, RBAC, budgets, and provider controls to deepen later.

## Recommended Approach

Introduce a typed agent graph layer above the current durable execution substrate.

The agent layer owns AI-specific semantics: node kinds, typed ports, shared agent state, planner-visible actions, provider bindings, budget policy, termination policy, and trace events. The existing platform continues to own durable storage, workers, logs, artifacts, cancellation, authentication, and operational views.

This avoids two weaker alternatives:

- A separate agent runtime beside workflows would duplicate scheduling, persistence, logs, cancellation, and dashboard plumbing.
- Encoding agent loops inside existing DAGs would fight the acyclic graph model and make open-loop planner decisions awkward.

## Architecture

Capsulet becomes an AI agent orchestration control plane, not only an automation/job runner.

```text
AgentDefinition
  -> AgentGraph
      -> typed nodes and ports
      -> allowed actions and transitions
      -> provider bindings
      -> memory and retrieval bindings
      -> budget policy
      -> termination policy

AgentRun
  -> AgentTraceEvents
      -> planner decision
      -> node execution
      -> observation
      -> validation
      -> next state or terminal stop
```

The first implementation should add agents beside existing workflows. Existing workflows continue to support deterministic DAG execution. Agents handle AI state machines and open loops.

## Core Model

The agent graph is typed at three levels.

### Node Kinds

V1 RAG node kinds:

- `planner`
- `query_normalizer`
- `embedding`
- `retriever`
- `reranker`
- `prompt_builder`
- `llm`
- `validator`
- `memory_read`
- `memory_write`
- `return`

These are semantic node kinds, not direct vendor implementations.

### Ports

Nodes declare typed inputs and outputs. Initial port value types:

- `UserQuery`
- `ConversationContext`
- `NormalizedQuery`
- `EmbeddingVector`
- `RetrievedDocuments`
- `RankedDocuments`
- `Prompt`
- `ModelResponse`
- `ValidationResult`
- `FinalAnswer`

The backend validates that node outputs can satisfy downstream inputs and state updates.

### State

Each agent run owns structured state. Initial state fields:

- original user query
- normalized or reformulated query
- conversation context
- retrieved documents
- ranked documents
- prompt
- candidate answer
- validation scores and rationale
- action history
- token usage
- approximate cost
- loop counters
- stop reason

### Graph Shape

V1 should use a pragmatic graph model rather than a fully general hypergraph:

```text
AgentGraph
  nodes: typed nodes
  edges: static data dependencies
  actions: planner-visible callable nodes
  state_schema: typed shared state contract
  transition_policy: allowed actions from each state
```

This gives the useful behavior of a hypergraph because one node can read and write multiple typed state fields. It keeps the first implementation smaller and easier to validate.

## Runtime Loop

The open loop advances one decision step at a time:

```text
planner reads AgentState
planner selects next allowed action
runtime executes selected node or provider
runtime validates typed output
runtime appends trace event
runtime updates AgentState
termination policy decides stop or continue
```

The planner is constrained by the graph. It can choose from registered actions, but it cannot call arbitrary code or undeclared providers.

Each planner/action cycle is the durable unit of progress. If the worker dies, Capsulet resumes from the last completed trace event and state snapshot.

## First RAG Agent

The first supported agent is an open-loop RAG answer agent:

```text
User query
  -> planner
      -> normalize query
      -> retrieve documents
      -> rerank documents
      -> build prompt
      -> call LLM
      -> validate answer
      -> optionally reformulate, retrieve, or call LLM again
      -> return answer
```

The v1 success criterion is not a perfect answer. It is that Capsulet can compile, run, inspect, replay, and bound a typed RAG agent loop.

## Provider Strategy

Providers are hybrid:

- local/mock providers for development and deterministic tests;
- provider interfaces for embedding, vector search, rerank, chat completion, validation, and memory;
- production bindings by configuration;
- provider metadata captured in trace events.

Trace metadata should include provider name, model name where relevant, latency, token usage, approximate cost, request id if available, and error class.

Provider calls should use idempotency keys where the provider supports them. Retries must not silently double-submit expensive or externally visible operations.

## Budget And Termination

Every agent definition must include both budget limits and semantic termination criteria.

Budget envelope:

- max steps
- max wall-clock seconds
- max tokens
- max approximate cost

Goal and validator envelope:

- answer accepted
- safety failed
- confidence too low
- no useful progress
- explicit escalation

Every terminal run records a concrete stop reason, such as `answer_accepted`, `budget_exceeded`, `safety_failed`, `no_progress`, `provider_failed`, or `human_escalation_required`.

## Python SDK Authoring

The primary v1 authoring surface is the Python SDK.

The SDK should let users define a typed RAG agent without hand-writing low-level JSON:

```python
agent = rag_agent(
    name="Support Answer Agent",
    retriever=vector_search("support_docs"),
    reranker=provider("rerank.default"),
    model=chat_model("chat.default"),
    validator=answer_validator(min_score=0.82),
    memory=conversation_memory("support"),
    budget=AgentBudget(max_steps=12, max_tokens=12000, max_seconds=90),
)
```

The SDK compiles this into an `AgentDefinition` request. The backend validates node kinds, ports, provider bindings, graph rules, budget policy, and termination policy.

## API Shape

Initial agent endpoints:

- `POST /v1/agents`
- `GET /v1/agents`
- `GET /v1/agents/{id}`
- `POST /v1/agents/{id}/runs`
- `GET /v1/agent-runs`
- `GET /v1/agent-runs/{id}`
- `POST /v1/agent-runs/{id}/cancel`
- `GET /v1/agent-runs/{id}/trace`

The API should keep request and response models explicit. Do not expose internal SQL rows or provider-specific SDK objects.

## Dashboard Scope

Dashboard v1 is for inspection and debugging, not primary authoring.

Required views:

- list agent definitions;
- start a manual agent run;
- inspect agent trace timeline;
- view state snapshots;
- view provider calls;
- view validator scores and rationale;
- view cost/token usage;
- view terminal stop reason;
- link node executions to logs and artifacts where applicable.

Existing automations should eventually trigger agents, but v1 can start with manual runs. Scheduled and webhook agent triggers can reuse the current automation trigger machinery later.

## Trace Events

Agent runs should record append-only trace events:

- `run_started`
- `planner_called`
- `action_selected`
- `node_started`
- `provider_called`
- `node_completed`
- `state_updated`
- `validator_completed`
- `budget_checked`
- `run_stopped`
- `run_failed`

Trace events must include enough data to replay the run at the semantic level: prior state version, action inputs, action outputs, provider metadata, validation outcome, budget snapshot, and stop decision.

Large prompts, responses, retrieved document payloads, and logs can use object storage with PostgreSQL metadata, matching the existing artifact/log boundary.

## Failure Handling

- Typed output mismatch: fail the node; allow planner recovery only if policy explicitly allows it.
- Provider timeout: retry according to provider policy, then return to planner or fail.
- Budget exceeded: terminal stop with `budget_exceeded`.
- Validator rejects answer: continue only if budget remains and progress policy allows.
- Unsafe output: terminal stop or human escalation according to policy.
- Worker crash: resume from the latest completed state snapshot and trace event.

## Testing Strategy

Start in domain and SDK tests.

Domain tests:

- rejects unknown node kinds;
- rejects invalid port wiring;
- rejects missing provider bindings;
- rejects missing budget policy;
- rejects missing termination policy;
- rejects planner actions not declared in the graph;
- accepts the canonical RAG graph.

SDK tests:

- compiles a simple RAG agent into deterministic JSON;
- includes budget and termination policy;
- produces stable provider binding references;
- rejects invalid authoring inputs before sending to the API.

Runtime tests:

- stops on accepted answer;
- stops on max steps;
- stops on max tokens;
- stops on max wall-clock time;
- stops on max cost;
- records replayable trace events;
- resumes from the last completed trace event after simulated worker loss;
- uses fake providers for deterministic success, timeout, invalid output, and validator rejection paths.

API tests:

- creates an agent definition;
- rejects invalid graph definitions;
- starts a manual agent run;
- returns agent trace events;
- cancels a non-terminal agent run.

Dashboard tests should cover trace rendering once the backend shape is stable.

## Suggested Implementation Slices

1. **Domain model**
   Add agent IDs, node kinds, typed ports, graph validation, budget policy, termination policy, run status, and trace-event value objects in `capsulet-core`.

2. **SDK compiler**
   Extend the Python SDK with `rag_agent`, provider references, budget types, and deterministic compilation tests.

3. **Persistence**
   Add PostgreSQL tables for agent definitions, agent runs, agent state snapshots, and trace events.

4. **API**
   Add agent definition CRUD-lite, manual run creation, run detail, cancellation, and trace reads.

5. **Runtime loop**
   Add a worker/application service that advances one planner/action cycle at a time using fake providers first.

6. **Provider interfaces**
   Introduce provider ports and local/mock adapters before production-specific adapters.

7. **Dashboard inspection**
   Add list, run detail, trace timeline, state snapshot, provider-call, validator, budget, and stop-reason views.

## Acceptance Criteria

- A Python SDK RAG agent compiles into a valid `AgentDefinition`.
- The API stores and returns agent definitions.
- A manual agent run can execute through a bounded open loop with fake providers.
- Every run has required budget and termination policy.
- Every run records trace events and state snapshots.
- Runs stop with explicit stop reasons.
- Invalid graph, provider, port, budget, and termination definitions are rejected.
- The dashboard can inspect a completed agent run trace.
- Existing workflow DAG behavior remains available and tested.

