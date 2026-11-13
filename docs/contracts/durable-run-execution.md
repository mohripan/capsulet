# Durable Run Execution

Status: experimental. How a verified-computation IR run is stored, advanced, and recovered, and
which of those statements a gate actually checks.

## The log is the run

A run's state is a fold over `ir_run_events`. Status, progress through the graph, what a loop has
spent, and which effects are outstanding are all computed from those events; none of them is a
field a component updated. `capsulet_runtime::RunState::fold` is the only way to build a run's
state, and both a worker starting fresh and a worker recovering after a crash call it.

Two columns are projections kept for querying rather than answers in their own right:
`ir_runs.status` and the `ir_effect_claims` ledger. Both are maintained by database triggers from
the events, and both have a test that folds the log and asserts the projection agrees. If the two
ever disagree, that test fails rather than a reader seeing a stale answer.

Checked by: `verify --gate postgres` (`crates/postgres/tests/ir_runs.rs`) and `verify --gate ir`
(`crates/runtime/tests/`).

## One orchestration path

Only the graph worker advances an IR run. The scheduler continues to own compatibility job DAGs and
workflow runs; it does not read or write IR run events, and neither does the evaluator.

This is enforced rather than agreed: `scripts/tests/check-contracts.ps1` fails when any crate other
than `capsulet-graph-worker` calls `append_ir_run_event`, `lease_next_ir_run`, or
`consume_ir_run_signal`, and when the scheduler or evaluator mentions `ir_run_events`, the decision
core, or the worker type.

Creating a run is not executing one. `POST /v1/ir/runs` writes the run and its admission event and
leaves it `queued` with no lease, so the API enqueues and a worker picks it up. The assurance mode
comes from the definition rather than the request: letting a caller choose it per run would let
anybody downgrade `enforce` to `observe` at the moment it mattered. `GET /v1/ir/runs/{id}/events`
returns the log itself rather than a summary, because the log *is* the run and a caller checking a
claim has to see the events it was folded from.

An agent run takes the same path. `POST /v1/agents/{id}/runs` compiles the agent, and writes the
definition version, the run, its admission and its inputs in one transaction. Cancelling a run and
answering a gate both land in the signal inbox, and the worker is what acts on them.

Automations do not yet target IR definitions; when they do, that is the path they take.

Checked by: `verify --gate claims`.

## Fencing

Every lease bumps the run's epoch, and every event names the epoch it was written under. A worker
that pauses long enough to lose its lease names the old epoch, its write matches no row, and it
finds out it is no longer the owner instead of corrupting a run somebody else is advancing.

A heartbeat extends a lease without changing the epoch — it says "still alive", not "took over" —
and returns whether the lease was still held, which is the earliest warning a worker gets.

Checked by: `verify --gate postgres`.

## Effects happen once

A protected effect is claimed before it is attempted and finalized once it is known to have
happened. A claim with no resolution after a crash means nobody knows, and what happens next follows
from the idempotency the IR declared:

| Declared | After a crash |
| --- | --- |
| `idempotent` | Retried under a new attempt. |
| `keyed` | Retried under the *same* attempt, so the key the far side already saw is presented again. |
| `non_idempotent` | The run stops with `effect_uncertain`. Nobody guesses. |

A claim resolves three ways, and the last two are deliberately not the same. `finalized` means it
happened. `abandoned` means the far side refused outright, so it definitely did not happen and the
node's failure routes like any other. `uncertain` means nobody knows, and the run stops. Collapsing
the last two would make every refusal look like the one case that has to stop everything, which is
how a system learns to ignore the case that matters.

A key source this runtime cannot supply is refused before the run starts rather than at the moment
the effect is due. The supported sources are `run_id`, `run_id+node_id`, and
`run_id+node_id+attempt`.

Checked by: `verify --gate ir` and `verify --gate postgres`.

## Loops

The worker drives a loop region: it opens an iteration, runs the body, reads what the body reported,
and decides whether to go round again. Opening an iteration resets the region's nodes so they run
again, and the event names them so the fold can do that without consulting the definition.

Three rules are worth stating because the obvious implementation gets each of them wrong.

**A continuation is a value, not a digest.** Node outputs travel as digests so a large one does not
end up in the log, and nobody can evaluate a loop condition from a digest. The ports the IR declares
as decision-relevant — a continuation, an invariant, a progress measure — are recorded as exact
values instead. An executor that reports none of them for a node the loop depends on stops the run
rather than having a value assumed for it: assuming `true` loops forever on a check that never ran,
and assuming `false` reports a loop as finished when nothing checked whether it was.

**The iteration count is checked when one is about to start.** Not continuously: a loop stopped
half-way through its last permitted iteration has thrown away the work it did and left no record
that it ran. And not before the continuation is read: a loop whose condition has gone false did not
exhaust anything, so reporting a budget as the reason it stopped would be a false statement about a
loop that finished its work. The other budgets — wall time, tokens, cost, effects — are spent
continuously and do stop a loop wherever it is.

**A node after a loop waits for the loop, not for an iteration.** A region member finishes once per
iteration, and finishing is not being done. Anything downstream that started on the strength of one
iteration's output would be acting on a value the loop was still working on, so a predecessor inside
a loop counts for that loop's own nodes at once and for everybody else only once the loop has
stopped.

Checked by: `verify --gate postgres` (`crates/graph-worker/tests/loops.rs`) and `verify --gate
chaos`, which drives a real loop through every kill point.

## Stopping

- **Cancellation** is a request. The run stops at the next point where stopping is safe, which is
  never in the middle of a claimed effect.
- **Timeouts** are measured from the time the log recorded the start, against a time the caller
  supplies. A worker that inherits a node another worker left running sees the same deadline the
  dead one did.
- **Compensation** runs before the run ends, for reversible effects that actually happened, in the
  reverse of the order they happened. An irreversible effect has no compensation and is not
  pretended to have one.
- **Escalation** suspends the run on a human gate. A gate opens only for somebody who holds the
  authority it names.

Checked by: `verify --gate ir` (`crates/runtime/tests/failure.rs`) and `verify --gate postgres`.

## What runs

Agents do. Starting an agent run compiles the agent into a definition, and the deployed worker's
executor runs what it says: deterministic nodes, local-model nodes, memory reads, validators, and job
and memory-write effects. See [ADR 0019](../adr/0019-agent-runs-execute-on-the-durable-path.md).

A definition registered directly runs its verifier nodes, and fails any other node with
`verifier_unavailable`. A run of one takes its graph inputs on `POST /v1/ir/runs` as `inputs`, by
the names the definition declares, each any JSON value. They are stored as values the run holds and
bound in one `inputs_bound` event, in the transaction that creates the run, so no worker leases it
without them. A name the definition does not declare is refused with `400`.

A verifier node (`kind: verifier`) names what decides it in its `capsulet.kind` parameter:

- `pack`: the validators of the pack its `capsulet.pack` parameter names. The definition has to pin
  that pack, and the project has to hold it as pinned. Each input is a JSON object from a relative
  path to that file's text, and the validators run in containers through the verifier host.
- `claim_reasoning`: the kernel's claim-reasoning family, over the proposal on its `proposal` input
  and the snapshot document on its `snapshot` input. It decides the obligations its
  `capsulet.decides` parameter names: discharged, mechanically, when the kernel accepts, residual
  when it is conditional, and failed when it rejects.

Either leaves on its `decision` output what it decided and only that: the obligations, the record of
every checker it ran, and the evidence those rest on, with its bytes. A worker with no container
runtime runs a pack node and decides nothing: each obligation stays residual, saying its validator
could not be started.

`POST /v1/ir/runs/{id}/certificate` (`jobs:run`) seals a finished run's certificate, or returns the
one already sealed. It is sealed from what the log names: the run's inputs, outputs and loops, and
each verifier node's decision. Every obligation the definition's contracts state is in it, as the
last node to decide it left it, or residual when no node did. So the same log always seals the same
certificate, and a run that was interrupted and resumed seals the one an uninterrupted run would.
The evidence is stored with it, so `GET /v1/assurance/certificates/{id}/bundle` reads it back. A run
that has not finished is refused with `409 run_not_finished`.

A worker holds its lease for as long as a node takes, by heartbeating while the work runs. Without
that, any node slower than its lease is taken over and run a second time.

A validator's `node_finished` carries its verdict as control readings:

- `verdict.passed`: whether its rules held;
- `verdict.waived`: present, and true, when a person waived a failure at a gate.

That makes a check's outcome part of the run's record rather than its content. A bundle, its
replay and the run's explanation can say whether each check held without carrying the value it
checked. A log written before verdicts were recorded reads as `verdict_unrecorded`.

Checked by: `verify --gate postgres` (`crates/graph-worker/tests/agent_runs/` and `execution.rs`),
and, for verifier nodes and sealing, `verify --gate m4` (`crates/graph-worker/tests/m4_gate.rs`).

## What is not claimed

- Directly registered definitions have no node providers or effect transports beyond verifier
  nodes. Their other nodes run only when compiled from an agent.
- An agent run declares no contracts, so it is `observe` mode and reports `unverified`.
- A pack node's validators need a container runtime, and nothing on the deployed stack holds one.
  The M4 gate drives its runs with the worker on the host, where Docker is. A verifier transport
  the deployed stack can use, such as Kubernetes jobs, is not built yet.
