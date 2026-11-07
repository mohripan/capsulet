# Agent Run Execution Plan

Written 2026-11-08. Tracks the gap named by `CAP-AGENT-002`: creating an agent run persists queued
work and nothing ever executes it.

## What is true today

- `POST /v1/agents/{id}/runs` writes an `agent_runs` row with status `queued`. No process reads it.
- `capsulet-application::AgentRuntime` can execute a run in-process, and only tests call it.
- `capsulet-graph-worker` is durable, fenced and chaos-tested, but it ships an executor that refuses
  every node, it is not in `compose.yaml`, not in the Helm chart, and has no health endpoint.
- Graph, agent and agent-run writes check ownership, write, then assign ownership in a second
  statement. Lists read 100 rows and then filter, so foreign rows consume the limit (#97).
- `POST .../runs` with an id that already exists overwrites that run and resets it to `queued`.
- `from_agent` produces a definition that admits only when the graph has no state fields and no
  effects, and whose boundary nodes have no ordering against its body, so it could not run.
- A graph node has no configuration: nothing says which model, template or rule a node uses.
- Nothing can request cancellation or answer a human gate, because no route delivers those signals.
- A worker does not heartbeat during a node, so any node slower than its lease is run twice.

## Decisions

**Agent runs execute on the IR path.** Starting an agent run compiles the agent through
`capsulet-ir-adapters::from_agent`, registers that definition version, and enqueues an IR run, all
in one transaction. The graph worker advances it. A second worker built on `AgentRuntime` would be
the competing orchestration path ADR 0018 exists to remove, and it would need its own leases,
fencing and recovery.

**`agent_runs` stays the product record; the log stays the truth.** The row links to its IR run.
Status, state version and final state are read from the log and value store, never stored twice.

**Run inputs are in the log.** A new `inputs_bound` event pins each input by digest, immediately
after admission and in the same transaction.

**Values are content-addressed and project-scoped.** Node outputs travel as digests, as they already
do. The bytes live in `ir_run_values`, keyed by tenant, project and digest, with a size limit, so one
project cannot read another's values even when both produced identical bytes.

**What a node does is pinned by the definition digest.** Source graph nodes gain string-valued
`config`, which the adapter carries to `parameters` on the IR node. Changing a prompt template changes
the digest.

**Usage is an event, written before the node finishes.** `resources_consumed` records model identity,
tokens and cost where known. Unknown stays unknown rather than becoming zero. A crash after the model
answered and before the node finished still spent those tokens, so an event is more honest than a
field on `node_finished`.

**Validators drive the loop.** The adapter wires each validator's result into `agent-continue`, a
deterministic node reporting the loop's continuation. No validator means one pass, matching what the
in-process runtime did. A validator that fails under `validator_pass` goes round again, carrying state
fields forward, until it passes or a budget stops it.

**Stopping is reported honestly.** A run ended by a budget, non-progress or a safety failure is
`stopped`, not `failed` and never `succeeded`. Run status is not an assurance verdict: these runs are
`observe` mode and `unverified`.

**Model calls are configuration, not a dependency.** Local Ollama is the only provider. With none
configured, a model node fails with `verifier_unavailable` and the run says why. No hosted service,
no paid account, no GPU.

## Out of scope for this plan

- Planner-mode transitions, where a model chooses the next node. Such a graph is refused at run start
  with a named reason rather than being run statically and looking like it worked.
- The M4 verifier protocol, domain packs, and container validators.
- Removing `AgentRuntime`. It stays until nothing cites it, and no longer describes production.

## Tasks

- [x] 1. Atomic, SQL-scoped graph, agent and agent-run persistence (#97).
- [ ] 2. IR and runtime: `parameters`, `inputs_bound`, `resources_consumed`, cancel signal, value check.
- [ ] 3. Worker: resolved node inputs, heartbeat during work, usage accounting, cancel handling.
- [ ] 4. Runnable `from_agent`: state fields through the boundary nodes, ordering, effects, grants.
- [ ] 5. Value store and the agent executor with its providers.
- [ ] 6. API: start, read, events, cancel and decide on agent runs through the IR path.
- [ ] 7. Deployment: health endpoints, Compose, Helm, images, a real end-to-end smoke test.
- [ ] 8. Dashboard: agent runs and their history.
- [ ] 9. Docs and claims: invert `CAP-AGENT-002`, add ADR 0019.

## Done means

A run started over HTTP against a stack brought up from `compose.yaml` reaches a terminal status
through the deployed worker, survives that worker being killed, can be cancelled, and reports what
each node did and what it spent. `CAP-AGENT-002` no longer says otherwise, and a gate proves it.
