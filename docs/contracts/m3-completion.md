# M3 Completion Report — Durable Graph Runtime

Status: implementation complete and **verified**. `cargo run -p capsulet-xtask --locked -- verify
--profile full` passes every gate on the development machine, including the new `chaos` gate. See
[the verification run](#verification-run-2026-09-06) for what it took and what it found.

M3 makes the M2 representation *run*, and makes "kill the control plane at any point and restart it"
an ordinary thing to do rather than a scenario with its own recovery code.

What M3 does not add: node providers, effect transports, or the verifier protocol. Those are M4. The
worker this milestone ships refuses every node with `verifier_unavailable` rather than pretending to
have run it, and the tests drive it through a scripted executor. What is finished is the durability
*around* execution, which has to be right before there is anything worth being durable about.

## The gate demonstration

The scenario the milestone is judged by is an executable test:
`crates/graph-worker/tests/chaos.rs`. It builds a definition with a bounded loop region and a
protected, keyed publication effect, seeds two finished loop iterations, and then runs the workflow
twelve times over — killing the worker after 1, 2, 3 … 12 decisions in turn. Each kill discards the
worker entirely and expires the lease it never released; the next worker recovers from the log and
the database alone, sharing nothing with the one before it. The executor is *not* discarded, because
it stands in for the world outside the run and the world does not restart when a worker does.

The effect's first attempt returns `Uncertain` — the request went out and the answer never came
back — so every scenario also crosses the one gap the whole design exists to make answerable.

After every restart the test asserts:

- the run reaches `completed`;
- the log folds, and its positions are gapless from zero;
- the loop's two iterations and its progress reading are still there, so no restart handed the
  budget back;
- every effect attempt presented the same idempotency key, and exactly one `effect_finalized` is in
  the log;
- the effect ledger and the fold agree that nothing is outstanding;
- a certificate assembled from that log replays offline.

```text
$ cargo run -p capsulet-xtask --locked -- verify --gate chaos
[verify] chaos            running
[verify] chaos            passed
[verify] passed 1 required gates: chaos
```

What this is not: an operating-system kill. The shipped binary has no executor that can run a
workflow until M4, so there would be no process worth killing. What it does exercise is every line
of state a real crash would take with it, because the recovering worker shares none of it.

## What was delivered

| Deliverable | Where | Tests |
| --- | --- | --- |
| Pure decision core: events, fold, `decide` | `crates/runtime/src/{event,state,decide}.rs` | 61 across the crate |
| Once-only effect protocol and key derivation | `crates/runtime/src/effect.rs` | included above |
| Loop budgets, invariants, progress direction, repair routes | `crates/runtime/src/loops.rs` | included above |
| Durable waits and signal matching | `crates/runtime/src/wait.rs` | included above |
| Cancellation, timeouts, compensation, escalation | `crates/runtime/src/failure.rs` | included above |
| Certificate inputs read from the log | `crates/runtime/src/certify.rs` | included above |
| Append-only run log, leases with fencing epochs | `migrations/20260906120000_ir_runs.sql`, `crates/postgres/src/ir_runs.rs` | 72 in the postgres gate |
| Effect ledger | `migrations/20260906130000_ir_effect_claims.sql`, `crates/postgres/src/ir_effects.rs` | included above |
| Durable waits and the signal inbox | `migrations/20260906140000_ir_run_waits.sql`, `crates/postgres/src/ir_signals.rs` | included above |
| The graph worker | `crates/graph-worker` | 13 |
| The chaos gate | `crates/graph-worker/tests/chaos.rs` | 1 scenario × 12 kill points |

## What the gates prove

Every claim registered for M3 names a test a gate runs:

| Claim | Statement | Gate |
| --- | --- | --- |
| `CAP-RUNTIME-001` | State is a fold; the stored status is a projection of the log | `ir`, `postgres` |
| `CAP-RUNTIME-002` | A superseded worker cannot append | `postgres`, `chaos` |
| `CAP-RUNTIME-003` | Effects are claimed first; uncertainty stops rather than guesses | `postgres` |
| `CAP-RUNTIME-004` | Killing the worker at every step boundary loses nothing | `chaos` |
| `CAP-RUNTIME-005` | Loop budgets survive restarts; non-progress is detected | `ir` |
| `CAP-RUNTIME-006` | Cancellation stops safely; compensation precedes the ending | `ir`, `postgres` |

## Verification run 2026-09-06

Three things the gates found that review had not.

### An internally-tagged event enum could not read back what it wrote

`RunEvent` was internally tagged (`{"event": "iteration_finished", …}`). Serde reads an
internally-tagged enum by buffering the whole object first, and that buffer cannot hold a 128-bit
integer — which is exactly the type of a loop's progress measure. Writing succeeded; reading failed
with `i128 is not supported`.

The shape of the bug is the reason it is worth recording: it was silent on the way in and only
appeared on recovery. A run with a progress measure could be written all day and became
unrecoverable the moment it restarted. It was found by the chaos gate, which is the only test that
writes an iteration record and then reads it back through the database.

Fixed by making `RunEvent` externally tagged, with a migration replacing the two trigger functions
that reach into the payload (`payload -> kind` is now the variant body).

### An effect node performed its effect before the node started

The first worker implementation claimed and performed the effect, and only afterwards recorded the
node as having started. The log read `effect_claimed, effect_finalized, node_started, node_finished`
— backwards, and confusing to anybody reconstructing what happened. Performing an effect *is* the
effect node running, so the worker now brackets the claim with the node's start and finish, and a
recovered attempt does not open the node twice.

### Two test suites shared a database and collided on tenant names

The graph-worker suite and the postgres suite each numbered their fixtures from one, ran against the
same database, and produced two `tenant_1`s. A test leased another suite's run and failed in a way
that looked exactly like a worker bug. Fixture names now carry the process id.

## Entry conditions for M4

- `verify --profile full` passes on a clean checkout, including `chaos`.
- The decision core is pure and enforced as such; the worker takes no decision the core could take.
- An IR run advances through exactly one path, enforced at the source rather than agreed.
- Effects have a once-only story that a crash cannot break, and the one case nobody can decide stops
  the run instead of guessing.
- Certificates are assembled from the log, so a resumed run and an uninterrupted one certify the
  same.

What M4 has to supply before any of this executes real work: node providers, effect transports, the
container verifier protocol, and the validator SDK. The `Executor` trait in
`crates/graph-worker/src/execute.rs` is the seam they plug into, and its typed outcomes —
`Performed`, `Failed`, `Uncertain` — are what the declared idempotency is matched against.

Two things M3 left for later, deliberately:

- **The worker does not drive loop iterations.** Deciding when an iteration begins is
  region-execution semantics that needs a running body, so the chaos gate seeds the loop history
  rather than producing it. What is proven is that the history survives a restart, which is the
  durability property; what is not yet proven is that a live loop produces it.
- **Region entry and exit beyond loop budgets.** A loop that stops for any reason other than its
  condition becoming false fails the run. A graph that continues past a completed region needs the
  region semantics above.
