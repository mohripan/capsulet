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

Creating a run is not executing one. `create_ir_run` writes the run and its admission event and
leaves it `queued` with no lease, so anything that wants a run to happen enqueues it and a worker
picks it up. Automations do not yet target IR definitions; when they do, that is the path they take.

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

## What is not claimed

- Node execution and effect transports are not implemented. The deployed worker ships an executor
  that refuses every node with `verifier_unavailable`; providers arrive in M4.
- Region entry and exit semantics beyond loop budgets are not implemented. A loop that stops for any
  reason other than its condition becoming false fails the run.
- A certificate is assembled from the log, but obligations and verifier records are supplied by the
  caller. Nothing here produces them, because nothing here checks anything yet.
