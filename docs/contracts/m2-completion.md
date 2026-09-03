# M2 Completion Report — Verified Computation IR v1

Status: implementation complete, gate **partially verified** in this environment. Read the
[what was not verified](#what-was-not-verified) section before treating M2 as closed.

M2 gives Capsulet one versioned, trust-typed representation for deterministic workflows, agent
workflows, and automations; immutable correctness objects; mandatory structural admission; explicit
assurance modes; and a certificate that someone else can check without trusting this installation.

Nothing executes from the IR. The durable runtime is M3, and a test asserts that no execution crate
depends on the adapters, so wiring it in stays a deliberate decision.

## The gate demonstration

The scenario the milestone is judged by is an executable test:
`crates/replay/tests/gate_scenario.rs`. It builds one definition containing a proposer, a checker, a
bounded loop that runs out of iterations, and a publication effect behind a protected boundary, then
admits it, certifies it, bundles it, replays it offline with the shipped binary, tampers with one
byte, and gates the same certificate under two modes.

Transcript from `cargo test -p capsulet-replay --test gate_scenario -- --nocapture`:

```text
admitted: sha256:2f2506fe5731bed39f971a12e86567e7a3fc527c11292ed5e80d6d503c9284fc
certified: conditional (1 residual, stopped: budget_exhausted)
--- replay (clean) ---
reproduced: conditional
  note: `cargo-test` was not re-run; its word was taken because the test runner reports its own result

--- replay (tampered) ---
diverged
  recorded:   conditional
  recomputed: rejected
  finding: evidence sha256:528f8a00…588ae0 contains bytes that digest to sha256:350e4a4c…496a84
  finding: the certificate records conditional but its contents support rejected
  note: `cargo-test` was not re-run; its word was taken because the test runner reports its own result

enforce: denied (conditional < accepted)
verify: not enforced (verdict recorded, nothing gated)
```

The replay steps run the built `capsulet-replay` binary as a child process with `env_clear()`: no
API URL, no token, no proxy, no database. Its dependency closure is asserted to exclude databases,
HTTP clients, async runtimes, and model providers, so this is a property of the build rather than of
the test setup.

## What was delivered

| Deliverable | Where | Tests |
| --- | --- | --- |
| Pure IR crate, canonical encoding, digests, schema versions | `crates/ir` | 114 |
| Structural value schemas and the trust lattice | `crates/ir/src/{value,trust}.rs` | included above |
| Nodes, effects, capabilities, provider bindings | `crates/ir/src/{node,effect,capability}.rs` | included above |
| Graph, hyperedges, branches, nested regions | `crates/ir/src/{graph,region,port}.rs` | included above |
| Bounded loops and typed stop reasons | `crates/ir/src/loop_region.rs` | included above |
| Immutable proposal, evidence, obligation, certificate | `crates/ir/src/correctness/` | included above |
| Mandatory structural admission | `crates/ir/src/admission.rs` | included above |
| Assurance modes and boundary decisions | `crates/ir/src/assurance.rs` | included above |
| Kernel obligation families, certificate assembly, replay | `crates/kernel` | 25 |
| Certificate bundles and the offline replay binary | `crates/kernel/src/bundle.rs`, `crates/replay` | 10 |
| Adapters for job DAGs, agent graphs, governed memory | `crates/ir-adapters` | 10 |
| Append-only persistence and read APIs | `migrations/20260905120000_ir_and_assurance.sql`, `crates/postgres`, `crates/api` | 7 (unrun; see below) |

New verification gates: `ir` (IR contracts, crate purity, adapter coverage) and `replay` (offline
replay including the tamper case), both in the fast and full profiles.

## Decisions worth knowing about

**The reasoning kernel was extended, not forked.** Its claim-grounding rules became the first
obligation family. All 17 of its existing tests pass without a single changed assertion.

**Observe forced the mode into the certificate.** Observe mode concludes `unverified` even when every
obligation happened to be discharged, which contradicted the earlier rule that a verdict must equal
what its obligations justify. The resolution was to seal the effective mode into the certificate, so
`unverified` under observe ("nobody was asked to check") and `unverified` under enforce ("a required
check did not happen") are distinguishable. This was a design improvement discovered by a failing
test.

**A tampered blob is a replay finding, not a parse error.** `Bundle::read` checks completeness and
leaves digest mismatches to replay, because tampering is the finding replay exists to make and
burying it in a container error would hide the signal a reader most needs.

**Replay states what it did not do.** External verifiers are declared oracles. Replay checks their
identity, version, and environment digest, takes their word, and says so in the outcome rather than
letting a reader assume the kernel confirmed a scanner result.

## Repairs made along the way

The M1 typed-IAM migration was half-applied at the M2 entry point: `ProjectRole` was typed in
`auth.rs` while several call sites still passed `Arc<str>`, so `cargo test --workspace` did not
compile and the `unit` gate could not run. That was fixed (commit `1501292`), including adding the
`memory:read` and `memory:write` permissions the in-flight authorization matrix test already
asserted. A stored role this build does not recognise now grants nothing rather than falling back to
the weakest known role.

`cargo fmt --check` was also red at HEAD; corrected separately in `601104f`.

## What was not verified

**The PostgreSQL integration tests have not been run.** `crates/postgres/tests/ir_and_assurance.rs`
compiles and is registered in the `postgres` gate, but no Docker daemon was available in the
environment where this work was done, so the append-only rules, project isolation, and idempotent
registration are asserted in code that has not executed. The gate fails rather than skips when
infrastructure is absent, so this is visible rather than assumed — but it is not yet evidence.

**The full profile has not been run end to end.** The fast profile passes (`format`, `unit`, `ir`,
`replay`, `api-contracts`, `claims`, `sdk`). The remaining full-profile gates — `lint` is green when
run directly, but `dashboard`, `postgres`, `migrations`, `security`, `compose`, `helm`, and `kind`
need Docker, npm, or a cluster — were not exercised here.

**The CLI `certificate export` command was not added.** The bundle endpoint exists
(`GET /v1/assurance/certificates/{id}/bundle`) and returns exactly what `capsulet-replay` reads, but
the convenience command that fetches and writes it is not implemented.

**Evidence upload has no write path.** Certificates record evidence metadata and the object key the
bytes belong at, but nothing in M2 writes those bytes: the correctness plane has no producer yet,
because producing them is what the M3 runtime does. The bundle endpoint therefore works for
certificates whose evidence was placed in object storage by something else.

## Residual risks

- **Adapter losses are declared, not eliminated.** Job ports are opaque, soft dependencies keep their
  ordering but not their semantics, and the agent continuation is a placeholder node. Each is a row
  in [the coverage report](ir-adapter-coverage.md). They become real fidelity when M3 executes from
  the IR, not before.
- **No workload uses this yet.** M2 proves the representation is checkable; it does not prove it is
  expressive enough for three unrelated domains. That is M4's gate, and it is where the design is
  most likely to need changes.
- **The accepted-to-conditional ratio is unmeasured.** The product design asks for it to be reported
  honestly. The obligation projection table makes it queryable; nothing produces enough certificates
  to measure it yet.

## M3 entry conditions

Before durable-runtime work starts:

1. Run the `postgres` and `migrations` gates against a real database and record the result here.
2. Run the full profile from a clean checkout on a machine with Docker, npm, Helm, and Kind.
3. Decide how a running worker obtains evidence bytes and writes them under their digest, since M3
   is the first thing that produces evidence rather than describing it.
4. Keep the adapters out of the execution path until the IR itself is what executes; the coverage
   report's losses are the list of things that must become real first.
