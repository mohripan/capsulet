# ADR 0018: Durable Graph Runtime

Status: Accepted

## Context

[ADR 0017](0017-verified-computation-ir-v1.md) added a representation. M2 could describe a workflow,
admit it, decide an assurance verdict, seal a certificate, and replay one offline. It could not
execute anything.

The state it left behind made "durable" unfalsifiable in the same way "verified" had been. Agent-run
creation persisted queued work with no worker to advance it. Graph execution was synchronous and in
process, so a request that died took the run with it. There was a record of a run's current status
and none of why it was there. Nothing claimed an effect before performing it, so a retry after a
crash could not tell "not yet done" from "done, and the acknowledgement was lost". Loops lived
inside node implementations, so their budgets could not be enforced across a restart. The scheduler
and the evaluator each advanced work along their own path.

M3 exists to make "kill the control plane at any point and restart it" an ordinary thing to do.

## Decision

### The log is the run, and recovery is the ordinary path

A run's state is a fold over an append-only event log. Status, progress through the graph, what a
loop has spent, which effects are outstanding — every one of them is computed from the events, and
`RunState::fold` is the only way to build one.

A worker starting fresh and a worker recovering after a crash call the same function over the same
events. That is the point: recovery has to reconstruct exactly what the dead worker knew, and the
only way to be sure it does is for both to compute it the same way from the same source. A separate
"current state" column would be a second answer that can disagree with the log, and the disagreement
always surfaces at the worst moment.

Two projections exist for querying — `ir_runs.status` and the `ir_effect_claims` ledger — and both
are maintained by database triggers with a test that folds the log and asserts they agree. A
projection is allowed; a second source of truth is not.

### Deciding is pure, and time is a parameter

`capsulet-runtime` decides; the worker acts. `decide(definition, state, now)` reads no clock, holds
no connection, and reaches for nothing ambient. Purity is enforced over the resolved dependency
closure rather than promised, exactly as in `capsulet-ir`.

`now` is supplied rather than read for a specific reason: a timeout measured from a clock the
replayer cannot reproduce makes a decision unreplayable, and recovery *is* replay. It also means a
test can drive a run through a week of timers without waiting.

### Effects are claimed before they happen

A protected effect writes a claim, performs, then finalizes. A claim without a finalization after a
crash means *nobody knows*, and what happens next follows from the idempotency the IR declared:
idempotent effects retry under a new attempt, keyed ones retry under the same attempt so the far
side sees the key it already saw, and non-idempotent ones stop the run.

The last case is the one worth defending. Stopping is inconvenient and looks like a worse outcome
than trying again. It is not: the alternative to stopping is guessing, and a guess about whether a
payment went out is exactly the failure this platform exists to prevent.

A key source this runtime cannot supply is refused before the run starts, not at the moment the
effect is due. Inventing a key for a far side that asked for a different one is a subtler version of
the same guess.

### Leases carry a fencing epoch

Every lease bumps the run's epoch and every event names the epoch it was written under. A worker
that pauses long enough to lose its lease names the old epoch, its write matches no row, and it
finds out. Heartbeats extend a lease without changing the epoch — "still alive" is not "took over" —
and a heartbeat that returns false is the earliest warning a worker gets.

### One orchestration path

Only the graph worker advances an IR run. This is enforced at the source: `check-contracts.ps1`
fails when any crate outside `capsulet-graph-worker` calls the log writers, and when the scheduler or
evaluator so much as mentions IR run events or the decision core. A behavioural test would prove
only that today's scheduler happens not to.

### Stopping is a decision, not an interruption

Cancellation is a request the run honours at the next safe point, never inside a claimed effect.
Compensation runs before a run ends, for reversible effects that actually happened, newest first;
an irreversible effect has no compensation and is not pretended to have one. Escalation suspends on
a human gate that opens only for somebody holding the authority it names.

## What was rejected

**A status column as the source of truth.** Simpler, faster, and the reason the previous design
could not say why a run was where it was. It also cannot survive two writers, which is what a
recovering worker and a paused one are.

**Deciding inside the worker.** Every decision the worker takes is one that a recovering worker
might take differently, and the difference is invisible until an effect happens twice.

**Rules rather than triggers for append-only enforcement.** PostgreSQL refuses `INSERT … ON
CONFLICT` on a table carrying an INSERT or UPDATE rule, and a `DO INSTEAD NOTHING` rule makes a
refused mutation look like a successful no-op. Raising says plainly that the write was refused.

**Optimistic concurrency on a version column instead of leases.** It answers "did anybody else
write" but not "am I still the owner", and a worker that is no longer the owner should stop rather
than retry.

**Cancelling immediately.** A run cancelled between claiming an effect and finalizing it ends with
something nobody can ever account for, which is worse than taking a moment longer to stop.

**An internally-tagged event enum.** `RunEvent` is externally tagged because serde reads an
internally-tagged enum by buffering the object first, and that buffer cannot hold a 128-bit integer
— which is exactly what a loop's progress measure is. The internally-tagged form wrote iteration
records it could not read back, so a run with a progress measure became unrecoverable the moment it
restarted. The failure was silent on the way in and only appeared on recovery, which is the worst
possible shape for a bug in a durability layer.

**Killing an operating-system process in the chaos gate.** The shipped binary has no executor that
can run a workflow until M4 brings providers, so there would be no process worth killing. The gate
discards the worker and expires the lease it never released, which takes with it every line of state
a real crash would.

## Consequences

- A control-plane process can be stopped at any point and restarted without losing committed state,
  duplicating a protected effect, or corrupting a certificate. The chaos gate kills the worker at
  every step boundary in turn and checks all three.
- Certificates are assembled from the log, so a killed-and-resumed run and an uninterrupted one
  certify the same.
- The worker does not yet drive loop iterations or execute nodes. Deciding when an iteration begins
  is region-execution semantics, and running a node needs a provider; both arrive with M4. What
  exists is the durability around them, which is the part that has to be right before there is
  anything to be durable about.
- Compatibility job DAGs keep their execution path. Converging them onto the IR is a migration that
  happens after the IR runtime is proven.
