# Verified Computation IR v1 Implementation Plan

> **For agentic workers:** Implement this plan task-by-task with test-first changes and one focused
> commit per task. M1 exit criteria are the entry gate for M2, not optional cleanup. This milestone
> defines, admits, certifies, and replays computation; it does not execute it. Do not begin M3
> durable-runtime work while any M2 exit criterion is red.

**Goal:** Complete M2 by giving Capsulet one versioned, trust-typed intermediate representation for
deterministic workflows, agent workflows, and automations; immutable proposal, evidence, artifact,
obligation, verdict, and certificate models; mandatory structural admission independent of assurance
mode; explicit Observe/Verify/Enforce policies with protected boundaries; and a small pure kernel
that replays a stored certificate offline and reaches the same verdict without a model or network.

**Architecture:** A new pure crate `capsulet-ir` owns the IR, canonical encoding, digests, the trust
lattice, admission rules, and assurance policy decisions. It performs no I/O, no async, no clock or
randomness access, and no floating-point arithmetic on persisted quantities. `capsulet-kernel` gains
a platform-certificate module and keeps its existing claim-reasoning rules as the first obligation
family. A separate minimal binary owns offline replay so its dependency closure is provably free of
databases, HTTP clients, and model providers. A new `capsulet-ir-adapters` crate translates existing
`WorkflowDefinition`, `GraphDefinition`, agent, and governed-memory records into the IR. Existing
execution paths keep running today's models unchanged; the IR is registered, admitted, certified,
and replayed alongside them.

**Tech Stack:** Rust 1.96, serde with a canonical JSON encoder, SHA-256 content addressing,
PostgreSQL and sqlx for append-only definition/evidence/certificate tables, object storage for
evidence bytes, Axum 0.8 with Utoipa 5.5 and `utoipa-axum` 0.2 for read APIs, `capsulet-cli` for
bundle export, and `capsulet-xtask` for the verification gates.

---

## Entry conditions

M2 starts only when `cargo run -p capsulet-xtask --locked -- verify --profile full` is green on a
clean checkout, including the typed IAM and authorization-matrix work currently in flight
(`crates/api/src/auth.rs`, `crates/api/src/endpoint_contract.rs`,
`crates/api/tests/authorization_matrix.rs`). Every route this plan adds needs generated
authorization coverage on the first commit that adds it, so the M1 policy model must exist first.

The conditions M2 exists to correct are structural, not cosmetic:

- `PortValueType` in `crates/core/src/domain/graph.rs` is a fixed nominal tag list with a `Json`
  escape hatch, so type compatibility and provenance claims are close to vacuous;
- graph state moves as opaque JSON (`AgentStateSnapshot.state_json`), and nothing records what was
  lost when a value passed through it;
- `GraphHyperedge` records sources and targets but no combine/distribute semantics, so a join cannot
  say which inputs justify which outputs;
- agent budgets and termination conditions exist (`crates/core/src/domain/agent.rs`) but loops are
  not first-class regions with invariants, progress measures, or typed stop reasons;
- `capsulet-kernel` certifies one claim-reasoning proposal only: `Certificate` has no workflow
  scope, no contract or policy version, no verifier identity, no evidence bundle, and its
  `replay_digest` covers the proposal but not the evidence set or the kernel version;
- `reasoning_certificates` is a single-domain table with no evidence linkage strong enough to replay
  from;
- no trust class exists anywhere in the codebase, so "verified" is currently a narrative, not a type;
- the platform verdict `unverified` is documented in `docs/contracts/lifecycle-and-assurance.md` but
  has no implementation.

## Chosen approach

Define the representation and its decision procedures before any runtime consumes them.

- One pure crate owns the IR. Purity is enforced by a dependency-closure test, not by convention.
- Canonical bytes come before digests. One encoder, one digest algorithm, one textual form
  (`sha256:<64 lowercase hex>`) everywhere a digest appears.
- Structural schemas describe values. Existing nominal port types survive as named aliases over
  structural schemas, so current graphs keep their labels while gaining real structure.
- Trust strengthens only through a verification record the kernel admitted. No cast, setter, or
  serde path constructs a strengthened trust class directly.
- Structural admission always applies. Observe suppresses domain obligations; it never suppresses
  graph validity, declared effects, bounded loops, budgets, or provenance requirements.
- Certificates are self-describing and bundle-replayable: pinned inputs, schema and kernel versions,
  verifier identities, evidence digests, obligations, assumptions, residuals, verdict, stop reasons.
- Adapters are additive translations with a generated, checked-in coverage report. Constructs that
  translate with loss must say so in the report and in the trust class; silent loss is a gate failure.
- Every new public route follows the M0/M1 contract rules: typed route declaration, generated
  OpenAPI, generated authorization cases, and a claims-registry entry labeled experimental.

## Scope boundaries

- Do not implement the M3 durable graph worker, timers, durable waits, human-gate suspension,
  compensation, cancellation semantics, checkpoint recovery, or worker reattachment. M2 defines the
  types those mechanisms will persist and certify.
- Do not implement the M4 validator SDK, container/process verifier protocol, or domain packs.
  External verifiers appear in M2 only as declared oracle records inside certificates, and the only
  executable verifier family shipped is the existing in-process kernel rule set.
- Do not build the M5 interpretability product. Read APIs exist to make certificates exportable and
  testable, not to become a dashboard surface.
- Do not rename persisted execution statuses or collapse them into the six target concepts. The
  lossy-mapping debt recorded in `docs/contracts/lifecycle-mapping.json` stays recorded.
- Do not introduce a new float-valued persisted quantity. Iterations, milliseconds, tokens, and cost
  units are integers; other decimals are fixed-point strings. The kernel's existing `ARITH_EPSILON`
  behavior stays inside the claim-reasoning family and never leaks into IR digests.
- Do not delete or fork the reasoning kernel. It becomes the first obligation family, and its
  current tests must pass unmodified.
- Do not weaken an M0 or M1 gate to make M2 land. If a new contract exposes a false claim, fix the
  claim or the implementation.

---

### Task 1: Create the pure IR crate and the canonical encoding contract

**Files:**

- Create: `crates/ir/Cargo.toml`
- Create: `crates/ir/src/lib.rs`
- Create: `crates/ir/src/canonical.rs`
- Create: `crates/ir/src/digest.rs`
- Create: `crates/ir/src/version.rs`
- Create: `crates/ir/tests/canonical.rs`
- Create: `crates/ir/tests/purity.rs`
- Create fixtures under: `crates/ir/tests/golden/`
- Modify: `Cargo.toml`
- Modify: `crates/xtask/src/verify/catalog.rs`
- Modify: `docs/contracts/verification-matrix.md`

- [ ] Add failing tests proving that two structurally equal documents with different key order and
  whitespace produce one identical digest, and that reordering a list changes it.
- [ ] Add failing tests rejecting NaN, infinity, duplicate object keys, non-UTF-8 bytes, and
  unnormalized Unicode before any digest is computed.
- [ ] Implement the canonical encoder: UTF-8 output, lexicographically sorted object keys, no
  insignificant whitespace, integers encoded as integers, other decimals as fixed-point strings, and
  an explicit refusal to encode `f32`/`f64` anywhere in persisted IR types.
- [ ] Implement a single `Digest` newtype over SHA-256 with `Display`/`FromStr` round-trip and
  parse-time rejection of malformed values. Every digest in this milestone uses that type.
- [ ] Implement `SchemaVersion` and stamp `capsulet.ir/v1` on every persisted root object. The
  compatibility reader accepts known majors and fails closed with a typed reason on anything else.
- [ ] Add the golden corpus and a test that re-encodes each fixture and byte-compares it. Document in
  the crate root that changing those bytes requires a schema version bump.
- [ ] Add the purity test: assert the crate's resolved dependency closure excludes database, HTTP,
  async-runtime, randomness, and clock crates, and report the offending path when it does not.
- [ ] Register an `ir` gate in the fast and full profiles that runs the crate's tests plus the golden
  digest comparison, and record it in the verification matrix.

**Task gate:**

```powershell
cargo run -p capsulet-xtask --locked -- verify --gate ir --gate format --gate lint
git diff --check
```

### Task 2: Define structural value types and the trust-class lattice

**Files:**

- Create: `crates/ir/src/value.rs`
- Create: `crates/ir/src/trust.rs`
- Create: `crates/ir/tests/value_compatibility.rs`
- Create: `crates/ir/tests/trust.rs`
- Create: `docs/contracts/ir-value-types.md`

- [ ] Add failing tests for structural compatibility, including a record accepted by a wider
  requirement, a union rejected without an exhaustive discriminant, and a mismatch that reports which
  field and which rule failed rather than a boolean.
- [ ] Add failing tests proving `Verified` cannot be produced by deserialization, cloning, or any
  public constructor without an admitted verification record.
- [ ] Implement `ValueSchema` covering unit, boolean, bounded integer, fixed-point decimal,
  constrained string, bytes-by-digest, enumeration, bounded list, record with required and optional
  fields, discriminated union, artifact reference, and the opaque `Json` escape hatch.
- [ ] Implement structural compatibility with typed reasons. Two `Json` values are never compatible
  merely because both are opaque; compatibility through `Json` is legal only where the definition
  declares the degradation.
- [ ] Implement `TrustClass` as `Unverified`, `Conditional { contract, certificate }`, and
  `Verified { contract, certificate }`, with a `meet` operation used to derive the trust of any value
  produced from several inputs.
- [ ] Make strengthening constructible only from a `VerificationRecord`. Serde deserializes into a
  raw representation that must pass admission before it becomes a `TrustClass`.
- [ ] Give every `Json` value a recorded provenance-loss marker that admission propagates into the
  certificate, so an opaque hop is visible rather than assumed harmless.
- [ ] Publish the mapping table from each current `PortValueType` variant to a named structural
  schema. Adapters consume this table in Task 12, and it is the only place the mapping is defined.

**Task gate:**

```powershell
cargo run -p capsulet-xtask --locked -- verify --gate ir
git diff --check
```

### Task 3: Declare nodes, effects, capabilities, and provider bindings

**Files:**

- Create: `crates/ir/src/node.rs`
- Create: `crates/ir/src/effect.rs`
- Create: `crates/ir/src/capability.rs`
- Create: `crates/ir/tests/effects.rs`

- [ ] Add failing tests for a node performing an undeclared effect, a node naming a capability the
  definition never granted, and a protected boundary declared without an underlying effect.
- [ ] Implement `NodeKind` for pure computation, proposer (model or tool), verifier, effect, human
  gate, memory read, memory write, sub-workflow, and region entry/exit. Kind determines what a node
  may declare, not merely how a UI labels it.
- [ ] Implement `Effect` with kind (network, filesystem, secret access, publication, memory write,
  external side effect), target descriptor, reversibility, and idempotency behavior: idempotent,
  keyed by an explicit idempotency key, or explicitly non-idempotent.
- [ ] Implement `Capability` grants for providers, container images, verifier identities, and
  network/filesystem/secret/data-residency scopes. A node may reference only capabilities the
  definition granted; a planner emitting an ungranted name is an admission failure, not a new
  capability.
- [ ] Implement `ProtectedBoundary` marking an effect or trust transition that requires a minimum
  verdict, identified stably enough to be named by a policy and by a certificate.
- [ ] Express resources and budgets as integers (milliseconds, tokens, cost micro-units, effect
  counts) so digests stay stable across platforms.

**Task gate:**

```powershell
cargo run -p capsulet-xtask --locked -- verify --gate ir
git diff --check
```

### Task 4: Define graph structure, hyperedges, branches, joins, and nested regions

**Files:**

- Create: `crates/ir/src/graph.rs`
- Create: `crates/ir/src/region.rs`
- Create: `crates/ir/tests/graph_structure.rs`

- [ ] Add failing tests for a dangling port reference, a type-incompatible edge, a join whose output
  cannot name its contributing sources, a value escaping a nested region's declared exit, and a cycle
  outside a loop region.
- [ ] Implement typed input and output ports carrying both a `ValueSchema` and a `TrustClass`.
- [ ] Implement hyperedges with explicit combine and distribute semantics, where every produced value
  records the source set that contributed to it. Provenance is part of the structure, not a log line.
- [ ] Implement sequence, conditional branch with exhaustive typed conditions, and join with a
  declared merge policy and declared trust derivation.
- [ ] Implement nested regions with typed entry and exit values and scoped capabilities. A region may
  narrow, never widen, its parent's capabilities and budgets.
- [ ] Make node, port, and edge ordering canonical and independent of insertion order, so two
  equivalent definitions authored differently produce one digest. Test this explicitly.

**Task gate:**

```powershell
cargo run -p capsulet-xtask --locked -- verify --gate ir
git diff --check
```

### Task 5: Make bounded loops first-class regions with typed stop reasons

**Files:**

- Create: `crates/ir/src/loop_region.rs`
- Create: `crates/ir/tests/loops.rs`
- Modify: `crates/ir/src/region.rs`

- [ ] Add failing tests for a loop without an iteration bound, a loop with an unbounded time, token,
  or cost budget, an invariant with no evaluator, a budget-exhausted loop reported as successful
  completion, and non-progress that goes unrecorded.
- [ ] Implement `LoopRegion` with typed entry state, body graph, continuation condition, and typed
  exit values.
- [ ] Require finite budgets for maximum iterations, wall-clock milliseconds, tokens, cost units, and
  effect counts. Absence is an admission failure, not a default.
- [ ] Support invariants evaluated before and after each iteration and an optional progress measure
  with a declared monotonic direction.
- [ ] Implement typed `StopReason`: continuation condition false, budget exhausted (naming which
  budget), invariant failed, non-progress detected, repair route exhausted, escalation required, or
  cancellation. Exhaustion is never a success value.
- [ ] Implement the per-iteration record type (iteration index, input and output digests, invariant
  outcomes, verdicts) so M3 can persist an execution history the certificate already understands.
- [ ] Require each declared failure type to name an allowed repair or escalation route. "Ask the
  model again" is expressible only where it is a declared, budgeted route for a failure type that
  permits it.

**Task gate:**

```powershell
cargo run -p capsulet-xtask --locked -- verify --gate ir
git diff --check
```

### Task 6: Define the immutable correctness objects

**Files:**

- Create: `crates/ir/src/correctness/mod.rs`
- Create: `crates/ir/src/correctness/proposal.rs`
- Create: `crates/ir/src/correctness/evidence.rs`
- Create: `crates/ir/src/correctness/obligation.rs`
- Create: `crates/ir/src/correctness/certificate.rs`
- Create: `crates/ir/tests/correctness.rs`
- Create fixtures under: `crates/ir/tests/golden/certificates/`

- [ ] Add failing tests proving that changing any field changes the object digest, that a certificate
  referencing absent evidence is invalid, and that an obligation with no discharge, assumption,
  waiver, residual, or failure state cannot be represented.
- [ ] Implement `Proposal` with producer identity, pinned input digests, candidate value, derivation
  reference, and claimed evidence. Nothing in the type permits promotion to accepted output.
- [ ] Implement `EvidenceRef` with content digest, media type, byte length, producer identity, and
  capture time carried as recorded data rather than read from a clock.
- [ ] Implement `Artifact` with its trust class and lineage references to the artifacts and evidence
  it derives from.
- [ ] Implement `Contract` (input requirements, output properties, allowed effects, obligation set,
  version) and `Obligation` (identifier, contract reference, statement, discharge state, owner).
- [ ] Implement the platform `AssuranceVerdict` (`unverified`, `accepted`, `conditional`, `rejected`)
  as a type distinct from the kernel's three-valued `Verdict`, with a total documented mapping in both
  directions and a test that the mapping loses nothing.
- [ ] Implement the platform `Certificate`: schema version, definition version, policy version,
  kernel version, contract references, verifier records (identity, version, environment digest, input
  and output digests, trust policy), evidence digests, discharged obligations, assumptions,
  residuals, stop reasons, verdict, and a replay digest computed over the canonical bytes of all of
  the above.
- [ ] Add golden certificate fixtures covering accepted, conditional, rejected, and unverified.

**Task gate:**

```powershell
cargo run -p capsulet-xtask --locked -- verify --gate ir
git diff --check
```

### Task 7: Implement mandatory structural admission

**Files:**

- Create: `crates/ir/src/admission.rs`
- Create: `crates/ir/tests/admission.rs`
- Create: `docs/contracts/ir-admission-rules.md`

- [ ] Add failing tests proving admission runs and can reject in Observe mode, that a definition
  failing admission produces no verdict at all rather than `unverified` output, and that each
  rejection carries a distinct typed code.
- [ ] Implement the rule set: graph validity, typed port compatibility, declared effects and
  capabilities, bounded retries and loops, required budgets, provenance requirements, schema and
  digest validity, and legality of every trust-typed edge.
- [ ] Give every rejection a typed code and an owning subsystem, reusing the kernel's existing
  `RepairOwner` vocabulary so failure routing stays one concept.
- [ ] Add a property test over generated definitions asserting admission is total: it always returns a
  decision, never panics, and never loops.
- [ ] Make the admission result a first-class record embedded in the certificate, so "structurally
  admitted" is provable after the fact.
- [ ] Generate the admission-rule reference document from the rule table so documentation cannot
  drift from the implemented rules.

**Task gate:**

```powershell
cargo run -p capsulet-xtask --locked -- verify --gate ir --gate claims
git diff --check
```

### Task 8: Implement assurance policies and protected-boundary decisions

**Files:**

- Create: `crates/ir/src/assurance.rs`
- Create: `crates/ir/tests/assurance.rs`
- Modify: `docs/contracts/lifecycle-and-assurance.md`

- [ ] Add failing tests proving Observe never yields `accepted`, Verify yields verdicts but never
  blocks a boundary, Enforce blocks a protected boundary whose certificate is below the minimum
  verdict, and a waiver without an authorized policy identity is rejected.
- [ ] Implement `AssuranceMode` selectable per workflow and per protected subgraph, where the
  strictest enclosing mode wins and the effective mode is recorded in the certificate.
- [ ] Implement `AssurancePolicy` with required contracts, required verifier identities, required
  approvals, minimum verdict per boundary, waiver authority, and trust-class routing rules for memory
  spaces and downstream nodes.
- [ ] Implement `decide_boundary(policy, certificate, boundary) -> BoundaryDecision` as a pure total
  function with typed denial reasons, so the API, the future worker, and the CLI share one decision
  procedure instead of three.
- [ ] Encode explicitly that an absent certificate or an unevaluated required check is `unverified`
  and can never satisfy a minimum verdict above `unverified`.
- [ ] Update the lifecycle and assurance contract to point at the implementing types and note that the
  verdict dimension is now executable.

**Task gate:**

```powershell
cargo run -p capsulet-xtask --locked -- verify --gate ir --gate claims
git diff --check
```

### Task 9: Extend the kernel to platform certificates and offline replay

**Files:**

- Modify: `crates/kernel/Cargo.toml`
- Modify: `crates/kernel/src/lib.rs`
- Create: `crates/kernel/src/family.rs`
- Create: `crates/kernel/src/workflow.rs`
- Create: `crates/kernel/src/replay.rs`
- Create: `crates/kernel/tests/replay.rs`
- Modify: `crates/kernel/src/tests.rs` only if a module path moves

- [ ] Add failing tests proving replay of a stored certificate returns the identical verdict and
  replay digest, a single mutated evidence byte turns the verdict to `rejected`, an unknown verifier
  identity fails closed, and a kernel or schema version mismatch is reported rather than silently
  replayed.
- [ ] Introduce `ObligationFamily` and register the existing claim-reasoning rules as the first
  family. The existing kernel test suite must pass without changing its assertions.
- [ ] Implement platform certificate assembly from an admitted definition, pinned inputs, evidence,
  obligation outcomes, and the effective assurance policy.
- [ ] Treat external verifiers as declared oracles: replay re-checks their recorded identity, version,
  environment digest, and input and output digests against the trust policy, and states in the
  outcome that it did not re-execute them.
- [ ] Implement `replay(certificate, bundle) -> ReplayOutcome` as a pure, total function with no
  clock, network, model, or filesystem access.
- [ ] Implement the certificate compatibility reader: a known major replays, an unknown major fails
  closed with a typed reason and never degrades to a permissive verdict.
- [ ] Keep reasoning certificates replayable: an existing stored `reasoning_certificates` row must
  translate into a platform certificate and replay to the verdict it recorded.

**Task gate:**

```powershell
cargo run -p capsulet-xtask --locked -- verify --gate ir --gate unit
git diff --check
```

### Task 10: Ship certificate bundles, the replay binary, and the replay gate

**Files:**

- Create: `crates/kernel/src/bundle.rs`
- Create: `crates/replay/Cargo.toml`
- Create: `crates/replay/src/main.rs`
- Create: `crates/replay/tests/cli.rs`
- Modify: `crates/cli/src/lib.rs`
- Modify: `crates/cli/src/tests.rs`
- Create: `crates/xtask/src/verify/replay.rs`
- Modify: `crates/xtask/src/verify/catalog.rs`
- Create fixtures under: `crates/replay/tests/fixtures/`
- Modify: `docs/contracts/verification-matrix.md`

- [ ] Add failing tests for a bundle missing referenced evidence, a bundle carrying an unreferenced
  blob, a tampered blob, and a manifest digest that does not match its contents.
- [ ] Implement the bundle format: a canonical manifest plus content-addressed blobs, produced as
  deterministic bytes and stamped with its own schema version.
- [ ] Add `capsulet certificate export <id> --output <bundle>` to the CLI, reading through the API and
  writing a self-contained bundle.
- [ ] Implement replay as a separate minimal binary whose dependency closure excludes database, HTTP,
  async-runtime, and model-provider crates, and assert that closure in a test. Export needs a network;
  replay must prove it does not.
- [ ] Make the replay command exit non-zero on verdict mismatch and print both verdicts plus the first
  differing digest, so a mismatch is diagnosable without a debugger.
- [ ] Add the `replay` gate: build a fixture bundle, run the replay binary in a child process with no
  service running and network-related environment scrubbed, assert the recorded verdict, then mutate
  one evidence byte and assert the verdict flips to `rejected`.
- [ ] Register `replay` in the full profile and record it in the verification matrix.

**Task gate:**

```powershell
cargo run -p capsulet-xtask --locked -- verify --gate ir --gate replay
git diff --check
```

### Task 11: Persist immutable IR definitions, evidence, and certificates

**Files:**

- Create: `migrations/20260905120000_ir_and_assurance.sql`
- Create: `crates/postgres/src/ir_definitions.rs`
- Create: `crates/postgres/src/assurance.rs`
- Modify: `crates/postgres/src/lib.rs`
- Modify: `crates/postgres/src/rows.rs`
- Create: `crates/postgres/tests/ir_and_assurance.rs`
- Modify: `crates/application/src/ports.rs`
- Create: `crates/application/src/assurance.rs`
- Modify: `crates/api/src/lib.rs`
- Modify: `crates/api/src/endpoint_contract.rs`
- Modify: `crates/api/src/auth.rs`
- Regenerate: `crates/api/openapi.json`
- Modify: `crates/api/tests/authorization_matrix.rs`
- Modify: `docs/contracts/database-migrations.md`
- Modify: `docs/contracts/product-claims.json`

- [ ] Add failing tests proving an update or delete of a stored definition version, evidence row, or
  certificate is refused by the database itself, a wrong-project read is denied, an insert whose
  content does not match its declared digest is refused, and re-registering identical content is
  idempotent and returns the same version.
- [ ] Create append-only tables `ir_definitions`, `ir_definition_versions`, `assurance_evidence`,
  `assurance_obligations`, and `assurance_certificates`, each carrying tenant and project ownership
  from the first migration and each enforcing immutability in the database, not only in Rust.
- [ ] Store canonical IR bytes and digests in PostgreSQL; store large evidence bytes in object storage
  keyed by digest, with the database holding digests and metadata only.
- [ ] Implement repositories whose reads and writes require tenant and project predicates, following
  the M1 ownership rules, with creation and ownership established in one transaction.
- [ ] Add routes `POST /v1/ir/definitions` (validate, admit, register), `GET /v1/ir/definitions`,
  `GET /v1/ir/definitions/{id}`, `GET /v1/ir/definitions/{id}/versions/{version}`,
  `GET /v1/assurance/certificates`, `GET /v1/assurance/certificates/{id}`, and
  `GET /v1/assurance/certificates/{id}/bundle`, all declared experimental with typed permissions.
- [ ] Return the admission result, not a bare `400`, when registration fails: the caller must be able
  to read which rule rejected the definition and which subsystem owns the repair.
- [ ] Regenerate OpenAPI, generate authorization cases for every new route, and add claims entries
  whose evidence commands actually execute the new tests.

**Task gate:**

```powershell
cargo run -p capsulet-xtask --locked -- verify --gate postgres --gate migrations --gate api-contracts --gate claims
git diff --check
```

### Task 12: Adapt job DAGs, agent graphs, and governed memory into the IR

**Files:**

- Create: `crates/ir-adapters/Cargo.toml`
- Create: `crates/ir-adapters/src/lib.rs`
- Create: `crates/ir-adapters/src/workflow.rs`
- Create: `crates/ir-adapters/src/graph.rs`
- Create: `crates/ir-adapters/src/memory.rs`
- Create: `crates/ir-adapters/tests/differential.rs`
- Create fixtures under: `crates/ir-adapters/tests/fixtures/`
- Create: `docs/contracts/ir-adapter-coverage.md`
- Modify: `Cargo.toml`
- Modify: `crates/xtask/src/verify/catalog.rs`

- [ ] Add failing differential tests: every fixture workflow and graph translates and is admitted; any
  definition today's `WorkflowGraph` or `GraphDefinition` validation rejects is also rejected by
  admission with a mapped reason; and translating the same input twice in separate processes yields
  the same digest.
- [ ] Translate `WorkflowDefinition`, steps, dependency policies, retries, and timeouts into IR nodes,
  edges, effect declarations, and budgets, defaulting to Observe mode.
- [ ] Translate `GraphDefinition` and agent definitions: each `PortValueType` maps through the Task 2
  table, opaque JSON state becomes a `Json` value whose trust class records the provenance loss, and
  agent budgets and termination conditions become loop-region budgets and typed stop reasons.
- [ ] Translate governed memory: sources, evidence spans, claims, and memory contracts become IR
  evidence and obligations, and memory writes become protected boundaries.
- [ ] Wrap existing reasoning certificates through the kernel's obligation family so a stored
  reasoning verdict is expressible as a platform certificate without re-deciding it.
- [ ] Generate the adapter coverage report listing every construct as fully represented, represented
  with recorded loss, or unsupported. Check it in and gate on it, so newly added loss must be declared
  rather than discovered later.
- [ ] Assert no behavioral change to existing execution: scheduler, agent runtime, and worker tests
  stay untouched and green, and the adapters are not wired into any execution path.

**Task gate:**

```powershell
cargo run -p capsulet-xtask --locked -- verify --gate ir --gate unit --gate postgres
git diff --check
```

### Task 13: Close M2

**Files:**

- Create: `docs/adr/0017-verified-computation-ir-v1.md`
- Create: `docs/contracts/ir-and-certificates.md`
- Create: `docs/contracts/m2-completion.md`
- Modify: `docs/contracts/stability-and-versioning.md`
- Modify: `docs/contracts/lifecycle-and-assurance.md`
- Modify: `docs/contracts/lifecycle-mapping.json`
- Modify: `docs/contracts/verification-matrix.md`
- Modify: `docs/contracts/product-claims.json`
- Regenerate: `docs/contracts/product-claims.md`
- Modify: `ARCHITECTURE.md`
- Modify: `docs/architecture.md`

- [ ] Record ADR 0017: IR v1 scope, canonical encoding, digest algorithm, trust lattice, admission
  model, certificate format, adapter strategy, and the deliberate exclusions M3 and M4 own.
- [ ] Document the IR, certificate, and bundle schema versions and their compatibility windows in the
  stability contract, including what a major bump requires of already-stored data.
- [ ] Update the lifecycle and assurance contract to state which parts of the verdict dimension are
  now implemented and which lossy execution mappings remain M3 debt.
- [ ] Add claims-registry entries only for behavior the gates execute — admission, assurance
  decisions, offline replay, adapter coverage, storage immutability — and keep durable runtime,
  verifier ecosystem, and domain packs labeled planned.
- [ ] Update the architecture documents to describe the IR crate boundary and the replay binary
  without implying the runtime already executes the IR.
- [ ] Run the full-profile rehearsal from a clean checkout, including the new `ir` and `replay` gates.
- [ ] Write `docs/contracts/m2-completion.md` with the replay demonstration transcript, the adapter
  coverage summary, residual risks, and the M3 entry conditions.
- [ ] Commit only after the full gate, generated-artifact checks, `git diff --check`, and a reviewed
  `git status --short` show no unintended files.

**Task gate:**

```powershell
cargo run -p capsulet-xtask --locked -- verify --profile full
git status --short
```

## M2 gate demonstration

The milestone gate is one scenario, executed by the `replay` gate and reproduced in the completion
report:

1. A fixture definition contains a proposer node, a deterministic checker, a bounded loop whose
   budget is exhausted, and a publication effect at a protected boundary under Enforce mode.
2. Admission runs, obligations are decided, and the platform certificate is produced with a
   `conditional` verdict, one residual obligation, and a budget-exhausted stop reason.
3. The certificate and its evidence export as a bundle.
4. The replay binary, in a process with no database, no API, no model provider, and no network,
   reproduces the same verdict and the same replay digest from the bundle alone.
5. Mutating a single evidence byte makes replay return `rejected` and name the failing digest.
6. The protected boundary is denied under Enforce with a typed reason, while the same policy under
   Verify records the verdict without denying anything.

## M2 release gate

The supported command remains:

```powershell
cargo run -p capsulet-xtask --locked -- verify --profile full
```

The full profile must include everything M1 required, plus, without silent omission:

1. IR crate contract tests, golden byte and digest stability, and the dependency-purity assertions;
2. structural admission rule tests, including the totality property test;
3. assurance-mode and protected-boundary decision tests for Observe, Verify, and Enforce;
4. kernel obligation-family tests with the existing reasoning suite unmodified;
5. the offline replay gate, including the tamper case and the version-mismatch case;
6. IR and assurance persistence tests proving append-only storage and project isolation;
7. OpenAPI, authorization, and claims coverage for every new route; and
8. the adapter differential tests and the checked-in coverage-report comparison.

## M2 exit criteria

M2 is complete only when all of the following are true:

- M0 and M1 gates remain green under the full profile;
- one versioned IR expresses deterministic workflows, agent workflows, and automations, and its
  canonical encoding produces stable digests across platforms and process runs;
- structural admission is mandatory in every assurance mode, total, and typed by repair owner;
- trust classes exist as types, never strengthen without an admitted verification record, and derive
  the weakest relevant trust for combined values;
- proposals, evidence, artifacts, obligations, and certificates are immutable, content-addressed, and
  rejected by the database on mutation;
- Observe, Verify, and Enforce are implemented as one pure decision procedure shared by every caller,
  and an absent certificate is `unverified` at every boundary;
- the kernel replays a stored certificate offline from pinned inputs and evidence, produces the same
  verdict and replay digest, and detects tampering and version mismatch;
- existing job DAGs, agent graphs, and governed-memory records translate into the IR with a generated,
  gated coverage report and no silent loss;
- existing execution behavior is unchanged and no runtime path depends on the IR yet; and
- documentation and claims describe M2 honestly, with durable execution, verifier ecosystem,
  interpretability, and self-hosting still labeled planned.

Passing M2 means a Capsulet result can carry a certificate someone else can check without trusting
Capsulet's runtime, its models, or its network. It does not yet mean the runtime executes that IR
durably; M3 owns the graph worker, durable waits, recovery, and effect-once semantics.
