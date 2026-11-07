# ADR 0019: Agent Runs Execute on the Durable Path

Status: Accepted

## Context

[ADR 0018](0018-durable-graph-runtime.md) gave Capsulet a worker that could recover from anything and
could run nothing: the deployed executor refused every node. Separately, `POST /v1/agents/{id}/runs`
wrote a queued row that no process ever read, and `AgentRuntime` could execute a run in-process only
because tests called it. So the platform's central promise, an agent run that does work, was met
nowhere outside a test.

Looking at what stood between those two pieces found more than a missing wire:

- `from_agent` produced a definition that admitted only with no state fields and no effects, and whose
  boundary nodes had no ordering against the body, so its exit could run first and close the iteration
  with the work undone.
- A graph node had no way to say what it did: no model, no template, no rule.
- A worker heartbeat only between steps, so any node slower than its lease was taken over and run again.
- Nothing could request cancellation or answer a gate, and answering a gate would have asked again
  forever, because the failure that raised it stayed unresolved.
- A definition with an integer port could be stored and never read back.

## Decision

### Agent runs are IR runs

Starting an agent run compiles the agent, registers that definition version, and enqueues an IR run,
in one transaction. The graph worker advances it like any other. A second worker built on
`AgentRuntime` would have needed its own leases, fencing and recovery, and would have been the
competing orchestration path ADR 0018 exists to remove.

`agent_runs` stays the product record and links to its IR run. Status, state version, usage and final
state are read from the log. The status column is what the row said when it was created, and nothing
reads it for a run that has a log.

### What a node does is part of the definition

Graph nodes gain string-valued `config`, which becomes `parameters` on the IR node. It is omitted when
empty, so every definition that has none keeps the digest it always had. Changing a prompt makes a new
version, and a run can always say which prompt it ran.

### The loop's shape comes from the adapter

```text
inputs -> agent-enter -> (the graph) -> agent-continue -> agent-leave -> outputs
```

A state field read comes out of `agent-enter`, and one written goes into `agent-leave`, because a value
may only cross a region through its boundary. A field both read and written is loop state, so the next
iteration starts from what the last one left. That is how a retry sees its own last attempt.

`agent-continue` is deterministic. It reads every validator's result and reports the continuation. A
graph with no validator runs once. A failed check goes round again under `validator_pass`, and without
that policy it fails the run: carrying past a failed check would make success say something false.

The progress measure is how many different results the loop has produced, so a loop landing on a result
it already produced has gone nowhere however busy it looked.

### Values are content-addressed and project-scoped

The log carries digests, as it always did. The bytes live in `ir_run_values`, keyed by tenant, project
and digest. Two projects holding identical bytes each hold their own row, so a digest is never a way to
read what another project stored. Every value a node produces is checked against its port's schema
before it is stored, which is the first time port types have constrained anything but the wiring.

Inputs are pinned in the log by an `inputs_bound` event, written with the admission.

### Usage is an event, and unknown stays unknown

`resources_consumed` is written as soon as a node reports what it spent, before the node finishes. A
crash between the two must not hand the budget back, because the model was still paid for. A provider
that reports no token count is counted in `unmeasured` rather than as zero: those are different
statements, and only one is true.

### An effect happens once per run, and its key is what makes that so

A job node's run id is derived from the claim's idempotency key, so a retry after a crash finds the run
it already created. A memory write does the same with its claim id, and proposes a candidate only: an
agent's output is not active memory until somebody with the authority says so. A later iteration reuses
what the first produced rather than repeating the effect.

A memory write now goes through the claim protocol too. The decision core had only done that for
`NodeKind::Effect`, so a memory write ran unclaimed, and a retry after a crash would have written twice.

### A job that fails is an effect that happened

A job that ran and failed fails its node with the claim finalized, not abandoned. `abandoned` means it
definitely did not happen, and a job that ran did.

### Stopping is reported as what it is

`stopped` covers a budget, non-progress, a safety failure, and a person saying no. None of those is a
success, and calling them `failed` would blame the work for a limit somebody set. Run status is never
an assurance verdict: these runs are `observe` mode and report `unverified`.

## What was rejected

**A dedicated agent worker over `AgentRuntime`.** Quicker, and it would have duplicated everything ADR
0018 built while leaving two answers to where a run is.

**A default model when a node names none.** The digest would stop pinning what ran. A model node that
names no model is refused at run start.

**Running a planner graph in static order.** It would finish and look like it worked. Planner
transitions are refused at run start until a model choosing the next node has a representation.

**Failing a run for a typed input nothing feeds, at the moment the node runs.** That is a bad agent
reported late. It is refused at run start, by name. A `json` input may stay unconnected, because it
declares no structure and a job taking no data is ordinary.

**Hosted model providers.** Local Ollama is the only provider. With none configured, model nodes fail
and say so, and everything deterministic still runs. Reaching a hosted service is a separate decision
about cost and about what data leaves the installation.

## Consequences

- `CAP-AGENT-002` no longer describes a gap. A run started over HTTP against the Compose stack is
  executed by the deployed worker, survives that worker being killed, and can be cancelled.
- Retrieval is lexical and says so: it finds shared words and nothing about meaning. That is what can
  run with nothing configured, and it is deterministic. Better retrieval belongs behind the same node.
- `AgentRuntime` stays until nothing cites it. It no longer describes production.
- Compatibility job DAGs keep their own path, as ADR 0018 said they would.
