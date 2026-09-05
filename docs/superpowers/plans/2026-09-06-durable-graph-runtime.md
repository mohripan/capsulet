# Durable Graph Runtime Implementation Plan

> **For agentic workers:** Implement this plan task-by-task with test-first changes and one focused
> commit per task. M2's gates are the entry condition. This milestone makes the IR *run*; it does
> not add verifier protocols (M4) or inspection surfaces (M5).

**Goal:** Complete M3 by executing verified-computation IR definitions in a dedicated durable
worker, such that any control-plane process can be killed at any point and restarted without losing
committed state, duplicating a protected effect, or corrupting the certificate chain.

**Architecture:** A new pure crate `capsulet-runtime` owns the decision layer: given a run's state
and its event history, it decides what may happen next. It performs no I/O and reads no clock, for
the same reason `capsulet-ir` does not — a decision that depends on ambient state cannot be
replayed, and recovery *is* replay. The worker is the I/O shell around it: it leases work, asks the
decision core what to do, performs effects, and appends events. Persistence is an append-only event
log per run; run state is a projection of that log, never an independent source of truth.

**Tech Stack:** Rust 1.96, PostgreSQL with `FOR UPDATE SKIP LOCKED` leasing and fencing epochs
(the pattern `job_runs` already uses), the M2 IR and kernel crates, and the `capsulet-xtask`
verification gates.

---

## Entry conditions

`verify --profile full` passes on a clean checkout. M2 delivered the representation, admission,
assurance decisions, certificates, replay, and the adapters; M3 is the first thing that executes
from any of it.

The conditions M3 exists to correct, from the product design's own audit:

- agent-run creation persists queued work but there is no dedicated agent or graph worker;
- graph execution is synchronous and in-process, so a request that dies takes the run with it;
- there is no durable record of *why* a run is where it is — only its current status;
- effects have no once-only story: nothing claims an effect before performing it, so a retry after a
  crash cannot tell "not yet done" from "done, and the acknowledgement was lost";
- loops exist only inside node implementations, so their budgets cannot be enforced across a
  restart;
- the scheduler and the evaluator each advance work along their own path.

## Chosen approach

- **The event log is the truth.** A run's state is a fold over its events. Recovery is that fold,
  not a special path, so the code that recovers is the code that runs normally.
- **Decisions are pure.** `capsulet-runtime` decides; the worker acts. A decision function that
  cannot reach a clock or a database can be tested exhaustively and replayed exactly.
- **Effects are claimed before they happen.** A protected effect writes a claim, performs, then
  finalizes. A claim without a finalization after a crash is *uncertain*, and what happens next
  depends on the idempotency the IR declared — never on a guess.
- **Leases carry a fencing epoch.** Every write names the epoch it was authorised under, and the
  database rejects writes from a superseded owner. A worker that pauses long enough to lose its
  lease cannot corrupt the run it thinks it owns.
- **Budgets are counted from the log.** Iterations, tokens, cost, and wall time come from persisted
  totals, so a restart cannot reset a loop's spending.
- **One orchestration path.** IR runs advance through the graph worker only. The scheduler
  continues to own compatibility job DAGs until they are migrated; nothing advances an IR run from
  two places.

## Scope boundaries

- No validator SDK, container verifier protocol, or domain packs — M4. Verifier nodes execute the
  in-process kernel families M2 shipped.
- No dashboard runtime views, replay diffing, or lineage UI — M5.
- No change to the certificate or IR schemas. If M3 needs a field they do not have, that is a schema
  major bump with a compatibility reader, decided deliberately.
- Compatibility job DAGs keep their current execution path. Converging them onto the IR is a
  migration, and it happens after the IR runtime is proven, not during.
- No new floating-point quantity, and no clock reads inside the decision core.

---

### Task 1: The pure decision core

**Files:** `crates/runtime/{Cargo.toml,src/lib.rs,src/state.rs,src/event.rs,src/decide.rs}`,
`crates/runtime/tests/decide.rs`, `Cargo.toml`, `crates/xtask/src/verify/catalog.rs`

- [x] Failing tests: a run with no events decides "start"; a completed run decides nothing; an
  unknown event kind is refused rather than ignored; folding the same log twice gives the same state.
- [x] `RunEvent` covering admitted, started, node-ready, node-started, node-finished, effect
  claimed/finalized/uncertain, iteration started/finished, suspended, resumed, cancelled, failed,
  completed — each carrying the fencing epoch it was written under.
- [x] `RunState` as a fold over events, with the run's position, ready set, loop counters, spent
  budgets, and outstanding effect claims.
- [x] `decide(state, definition) -> Vec<Decision>`: which nodes may start, which waits are due,
  which budgets are exhausted, whether the run is finished.
- [x] Purity test over the crate's dependency closure, as `capsulet-ir` has.
- [x] Covered by a gate in the fast and full profiles. Folded into the existing `ir` gate rather
  than given its own: the decision core is IR contracts executed, and a gate per crate is a gate
  nobody reads.

### Task 2: The append-only run event log

**Files:** `migrations/*_ir_runs.sql`, `crates/postgres/src/ir_runs.rs`,
`crates/postgres/tests/ir_runs.rs`

- [x] Failing tests: positions are gapless per run; an insert at a taken position is refused; UPDATE
  and DELETE raise; events from a superseded epoch are refused.
- [x] `ir_runs` (identity, definition digest, status, epoch, lease) and `ir_run_events` (run,
  position, kind, payload, epoch, recorded_at), both append-only.
- [x] Append is a single statement that takes the next position atomically, so two workers cannot
  both write position *n*.
- [x] Loading a run returns its events in order, and the projection equals the stored status.

### Task 3: Leases with fencing epochs

**Files:** `crates/postgres/src/ir_runs.rs`, `crates/postgres/tests/ir_runs.rs`

- [ ] Failing tests: two workers leasing concurrently get different runs; an expired lease is
  reclaimable; a write from the old epoch after reclamation is refused; heartbeats extend a lease
  without changing the epoch.
- [ ] `lease_next_run` using `FOR UPDATE SKIP LOCKED`, bumping the epoch on each new lease.
- [ ] Every event append carries an epoch and is rejected when it is not the current one.

### Task 4: The effect ledger and once-only semantics

**Files:** `crates/runtime/src/effect.rs`, `migrations/*`, `crates/postgres/src/ir_runs.rs`, tests

- [ ] Failing tests: a claimed-but-unfinalized idempotent effect is retried; a keyed one is retried
  with the same key; a non-idempotent one stops the run as `effect_uncertain` rather than retrying;
  a finalized effect is never performed twice.
- [ ] Claim rows keyed by (run, effect, attempt) with the idempotency key the IR declared.
- [ ] Recovery classifies every outstanding claim by the IR's declared idempotency, and the run
  records which branch it took.

### Task 5: Loops that survive a restart

**Files:** `crates/runtime/src/loops.rs`, tests

- [ ] Failing tests: iteration counts survive a fold; a budget exhausted before the crash stays
  exhausted; non-progress is detected across a restart; an invariant failure routes by the IR's
  declared repair route.
- [ ] Iteration records appended per iteration, with the digests M2's `IterationRecord` defines.
- [ ] Budgets computed from the log, never from memory.

### Task 6: Durable waits

**Files:** `crates/runtime/src/wait.rs`, `crates/postgres/src/ir_runs.rs`, tests

- [ ] Failing tests: a timer due in the future does not run; a due timer resumes exactly once; an
  external event resumes a matching wait only; a human gate resumes only on an authorised decision.
- [ ] Suspension is an event; resumption is an event; nothing is held in worker memory.

### Task 7: The graph worker

**Files:** `crates/graph-worker/{Cargo.toml,src/main.rs,src/runtime.rs}`, tests

- [ ] Failing tests: the worker advances a run to completion; it stops when its lease is lost; it
  reattaches to a run whose previous owner vanished.
- [ ] Lease, fold, decide, execute, append — in that order, with a heartbeat while executing.
- [ ] No decision is taken in the worker that the decision core could take.

### Task 8: Cancellation, retries, timeouts, compensation, escalation

**Files:** `crates/runtime/src/failure.rs`, tests

- [ ] Failing tests: cancellation stops before the next effect and not mid-effect; a retry respects
  the declared attempt budget; a timeout is a typed stop reason; a compensation runs only for a
  reversible effect that actually happened; escalation suspends rather than failing.

### Task 9: Certificates from the run history

**Files:** `crates/runtime/src/certify.rs`, tests

- [ ] Failing tests: a completed run's certificate replays offline; a run killed and resumed
  produces the same certificate as one that ran straight through; a certificate names every loop
  stop reason the log recorded.
- [ ] Assembly reads the event log and hands M2's `certify` its inputs.

### Task 10: Convergence

**Files:** `crates/scheduler/src/service.rs`, `crates/evaluator/src/service.rs`, docs

- [ ] Failing tests: nothing but the graph worker advances an IR run; the scheduler leaves IR runs
  alone; an automation that targets an IR definition enqueues a run rather than executing it.

### Task 11: The M3 gate

**Files:** `crates/graph-worker/tests/chaos.rs`, `crates/xtask/src/verify/catalog.rs`

- [ ] A looping workflow with a protected effect runs while the worker process is killed at each
  phase boundary in turn; after every restart the run completes with no duplicated effect, no lost
  committed state, and a certificate that replays.
- [ ] The gate runs this as a required check.

### Task 12: Close M3

**Files:** ADR, `docs/contracts/`, claims registry, completion report

- [ ] ADR for the durable runtime decisions and what was rejected.
- [ ] Claims only for what the gates execute.
- [ ] Completion report with the chaos transcript and M4 entry conditions.

## M3 exit criteria

- M0–M2 gates stay green;
- a killed and restarted control plane loses no committed state, duplicates no protected effect, and
  corrupts no certificate;
- run state is a projection of an append-only log, and recovery uses the same fold as normal
  operation;
- effects are claimed before they happen, and an uncertain non-idempotent effect stops the run
  rather than being retried;
- loop budgets and non-progress survive restarts;
- one path advances an IR run;
- documentation claims only what the gates prove.
