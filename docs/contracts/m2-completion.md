# M2 Completion Report — Verified Computation IR v1

Status: implementation complete and **verified**. `cargo run -p capsulet-xtask --locked -- verify
--profile full` passes all fifteen gates on the development machine. See [the verification
run](#verification-run-2026-09-03) for what it took to get there and what it found.

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
| Append-only persistence and read APIs | `migrations/20260905120000_ir_and_assurance.sql`, `crates/postgres`, `crates/api` | 7, run against PostgreSQL |

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

## Verification run, 2026-09-03

`verify --profile full` passes end to end:

```text
[verify] passed 15 required gates: format, lint, unit, ir, replay, api-contracts, claims,
         sdk, dashboard, postgres, migrations, security, compose, helm, kind
```

Getting there took four fixes, three of which were defects the gates found rather than anything the
milestone planned. That is the case for gates that fail rather than skip, made concretely.

### The immutability defect the `postgres` gate found

The first run of the persistence gate failed every write:

```text
INSERT with ON CONFLICT clause cannot be used with table that has INSERT or UPDATE rules
```

The migration enforced append-only storage with `CREATE RULE ... DO INSTEAD NOTHING`, and the
repositories used `INSERT ... ON CONFLICT DO NOTHING` for idempotent, content-addressed
registration. PostgreSQL refuses to combine the two, so *every* definition and certificate write
would have failed at runtime. No unit test could have caught it: the incompatibility exists only in
the database.

The fix improved the design rather than working around it. Immutability is now a row-level
`BEFORE UPDATE OR DELETE` trigger raising `restrict_violation`, which is better than the rule it
replaced on its own terms. A `DO INSTEAD NOTHING` rule makes a mutation *silently* succeed as a
no-op, so someone rewriting a verdict would believe it had worked; raising says plainly that the
write was refused. The tests now assert the refusal is loud rather than merely ineffective.

### The prerequisite probe reported installed tools as missing

`verify doctor` claimed `helm` and `kubectl` were absent on a machine where both were installed. The
probe ran `<tool> --version` for everything, and both tools reject that flag outright — they take a
`version` subcommand. The consequence was worse than a cosmetic wrong answer: an operator would be
told to install what they already had, and the full profile would stop at a gate that could have
run. Probes now use the flag each tool actually accepts, and a test asserts the doctor never reports
a tool that is on `PATH` as missing.

The `security` gate had the same shape of bug one layer down: it invoked `cargo audit --locked`, and
`cargo audit` has no such flag. Both only surfaced once the tools were present, which is the point —
a gate that cannot run proves nothing about the gate.

### The dashboard lint was broken by an override, not by ESLint

The first diagnosis was incomplete. Pinning ESLint back to 9 removed the
`react/display-name` crash but exposed the real cause: `package.json` carried

```json
"overrides": { "brace-expansion": "^5.0.8" }
```

which forced *every* consumer onto v5, including `minimatch@3`, which requires `^1.1.7` and uses the
older export shape. ESLint then died with `expand is not a function` regardless of version. The
override was a CVE remediation, so removing it outright would have traded a broken lint for a
vulnerability. Scoping it per major keeps both properties:

```json
"overrides": { "brace-expansion@1": "^1.1.18", "brace-expansion@2": "^2.1.4" }
```

`minimatch@3` now resolves a patched 1.1.18, `minimatch@10` keeps v5, `npm audit` still reports zero
vulnerabilities, and the lockfile was regenerated because the previous one recorded a tree npm
itself considered invalid. The dashboard also carried one genuine lint warning — a deliberate full
page reload after sign-in, so every server component and the middleware see the new session cookie —
which is now documented at the call site rather than left to look like an oversight.

### One vulnerability was fixed and one is an owned exception

`cargo audit` found two. `RUSTSEC-2026-0258` in `h2` is reachable through `hyper` and `axum`, so it
was fixed by moving 0.4.14 to 0.4.19.

`RUSTSEC-2023-0071` in `rsa` has no fixed release. It reaches `Cargo.lock` only through
`sqlx-mysql`, which the workspace's postgres-only feature set never compiles: `cargo tree -i rsa`
reports nothing and a workspace-wide `cargo tree -e normal` contains no `rsa` node. `cargo audit`
reads the lockfile rather than the build graph, so it cannot see that.

That is recorded as an exception in [security-exceptions.md](security-exceptions.md) with an owner
and a review date, and a test fails the build if an advisory is silenced without being documented,
or if a review date passes. **It is a security exception created during this work and it belongs to
whoever owns the platform** — the reasoning is verifiable but the decision to accept it is not the
implementer's to make silently.

## What was not verified

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

1. Confirm the full profile on a second machine and in CI, since it has only been run here.
2. Decide how a running worker obtains evidence bytes and writes them under their digest, since M3
   is the first thing that produces evidence rather than describing it.
3. Keep the adapters out of the execution path until the IR itself is what executes; the coverage
   report's losses are the list of things that must become real first.
