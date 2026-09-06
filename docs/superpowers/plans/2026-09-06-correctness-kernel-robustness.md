# Correctness Kernel Robustness Plan

> **For agentic workers:** Implement this plan task-by-task with test-first changes and one focused
> commit per task. Task 0 comes first and is not optional: it makes the documentation honest before
> any code changes, so that a half-executed plan still leaves the project telling the truth.

**Goal:** Make the correctness kernel and the assurance gate enforce the guarantees they already
claim. Several of those guarantees are stated in doc comments and in the product-claims registry but
are not implemented, and three of them are disproven by tests now in this repository.

**The one mistake, in twelve places:** every finding below is the same error wearing a different
hat — *a declaration is being read as a proof*. A certificate declares `contracts: [X]` and the gate
treats that as coverage of X. A trust document declares `verdict: accepted` and the admission
function treats that as a verdict. A policy declares `required_verifiers: [scanner]` and a name
match is treated as a check having happened. In each case the field a decision reads is written by
the party the decision is meant to constrain.

The rule this plan applies everywhere: **for every field a decision reads, either it is derived from
something the writer cannot forge, or the decision must not read it.**

---

## Entry conditions

`verify --profile full` passes at 16/16 gates. M3 is delivered. This plan touches `capsulet-ir`,
`capsulet-kernel`, and `capsulet-replay`; it does not add verifier protocols (M4) or inspection
surfaces (M5).

## Evidence

Three findings were confirmed by running code, not by reading it. The probes are in the tree under
`crates/ir/tests/known_gaps.rs` and `crates/kernel/tests/known_gaps.rs`. Each asserts the **current,
wrong** behaviour and says so; each is a tripwire that goes red exactly when its task lands, which
forces the implementer to update the claim registry in the same commit.

| # | Finding | Status |
|---|---------|--------|
| 1 | A boundary requiring contract `no-secrets-leaked` opens for a certificate that discharged only an unrelated `house-style` obligation. | **Proven** |
| 2 | `TrustClass::Verified` is reachable from a hand-written record naming a certificate digest that resolves to nothing. | **Proven** |
| 3 | `check` does not terminate with a verdict on a deeply nested derivation; the process exits with `STATUS_STACK_OVERFLOW`. | **Proven** |

## Overclaims to correct

Documented guarantees that are stronger than the code, in three places:

- `crates/kernel/src/lib.rs:5` — "every check is total: `check` always terminates with a verdict",
  and `:45` — "Always terminates." Finding 3 disproves both.
- `CAP-IR-002`, maturity `implemented`, listed on the public surfaces `ARCHITECTURE.md` and
  `docs/architecture.md`: *"A value's trust class cannot be strengthened by assertion."* Finding 2
  disproves the headline. Its qualifying clause — "a document claiming a verdict its verification
  record does not justify is refused" — is true, and is all the evidence tests check. The claim
  passed the registry's staleness gate because the clause is a tautology: the record is part of the
  same document, so the check compares a document against itself.
- `crates/ir/src/trust.rs:3` — "Trust never strengthens by assertion. Not by a cast, not by a
  setter, not by a field in a JSON document someone posted." The third of those is exactly what
  Finding 2 does.

## Chosen approach

- **Coverage is computed, never declared.** `Contract` already carries `obligations:
  Vec<ObligationStatement>`, and `Definition::contract(id)` already resolves one. The gate cannot use
  any of it because `decide_boundary` receives only the definition's *digest*. Give it the definition
  and coverage becomes a calculation over data that already exists.
- **A record whose truth lives elsewhere cannot have a context-free `Deserialize`.** `Certificate`
  self-verifies because it carries its own seal. `VerificationRecord` cannot, because its truth lives
  in a certificate somewhere else — so it takes a resolver argument, the way `replay` already takes
  an `EvidenceSource`.
- **Totality is a bound, not a hope.** A recursive checker over proposer-supplied input is bounded
  explicitly, the bound is recorded on the certificate, and exceeding it is a rejection.
- **Record the rule, not just the result.** `AdmissionRecord` already does this: it stores
  `rules_applied` "so a later reader knows which rules this build applied rather than assuming it
  applied today's set". The verdict derivation needs the same treatment.
- **Purity is preserved.** No decision function gains a clock, a network call, or a database. Time
  and revocation arrive as arguments, for the same reason the runtime's decision core has no clock:
  a decision that depends on ambient state cannot be replayed.

## Scope boundaries

- No verifier protocol, container sandbox, or domain packs — M4.
- No dashboard or lineage UI — M5.
- Schema changes here are deliberate major bumps with compatibility readers, not silent additions.
  Tasks 6 and 10 both bump; they should land together to spend one migration rather than two.
- `Rule::Interpret` stays the step the kernel cannot justify. Task 9 makes its residual reviewable
  and bounded; it does not try to make interpretation sound.

---

### Task 0: Say what is true now

**Files:** `crates/kernel/src/lib.rs`, `crates/ir/src/trust.rs`, `docs/contracts/product-claims.json`,
`crates/ir/tests/known_gaps.rs`, `crates/kernel/tests/known_gaps.rs`

- [x] Restate `CAP-IR-002` to the guarantee that actually holds: a trust class cannot exceed what its
  record's *recorded* verdict justifies. Drop the "cannot be strengthened by assertion" headline, and
  add it back in Task 2 with evidence that resolves a certificate.
- [x] Downgrade the kernel's totality doc comments to what holds: `check` returns a verdict for any
  derivation within the depth bound. Task 1 restores the unqualified claim.
- [x] Amend the `trust.rs` module doc so it stops promising the property Finding 2 disproves.
- [x] Land the three probes as documented tripwires. The kernel depth probe is `#[ignore]`d with its
  reason, because a stack overflow takes the whole test binary down with it.
- [x] `verify --profile full` still passes; the claims registry renders clean.

### Task 1: Make `check` total again

**Files:** `crates/kernel/src/{lib.rs,ir.rs,error.rs}`, `crates/kernel/tests/depth.rs`

- [ ] Failing tests: a derivation nested 100,000 deep returns a certificate rejecting it rather than
  killing the process; a derivation at exactly the bound still decides; the bound appears in the
  certificate's errors; a wire document nested past the bound is refused during deserialization.
- [ ] `CheckError::DepthExceeded { limit }`, and `derive` carries a depth it refuses to exceed.
- [ ] Deserialization is the second door and needs its own bound. `Rule` is `Deserialize` and serde
  recurses before the kernel is ever called, so a fix inside `derive` alone leaves the process just
  as killable.
- [ ] Prefer an explicit work stack over recursion, so the bound expresses policy rather than
  whatever stack the host thread happened to have.
- [ ] Record the limit on the certificate, so a reader knows what bound a decision was made under
  rather than assuming today's constant.

### Task 2: Bind a trust record to a certificate that exists

**Files:** `crates/ir/src/trust.rs`, `crates/ir/tests/trust.rs`, `docs/contracts/product-claims.json`

- [ ] Failing tests: a record naming a digest that resolves to no certificate is refused; a record
  whose verdict disagrees with the resolved certificate is refused; a record naming a contract the
  certificate does not cover is refused; `residual_count` and `provenance_complete` come from the
  certificate, and a document that disagrees is refused.
- [ ] `VerificationRecord::admit(raw, &impl CertificateSource)`. The raw document supplies only
  *which* certificate and *which* contract; every field a decision reads is derived from the resolved
  certificate.
- [ ] Remove the context-free `Deserialize for TrustClass`. A type whose invariant lives elsewhere
  cannot honestly have one, and leaving it in place is what makes the forgery a one-liner.
- [ ] Restore the `CAP-IR-002` headline, with evidence that resolves a certificate.

### Task 3: Compute contract coverage from the definition

**Files:** `crates/ir/src/assurance.rs`, `crates/ir/src/correctness/certificate.rs`,
`crates/ir/tests/assurance.rs`

- [ ] Failing tests: the Finding 1 probe, inverted — a boundary requiring `no-secrets-leaked` is
  denied for a certificate that discharged only `house-style`; a certificate covering every statement
  of the required contract is allowed; a certificate missing one statement is denied *naming the
  missing statement*; an obligation attributed to another contract does not count;
  `policy.required_contracts` is enforced.
- [ ] `decide_boundary` takes the admitted `Definition` rather than a bare `Digest`. The digest
  equality check stays — it is what ties the certificate to this definition.
- [ ] Coverage of contract X = every `X.obligations[*].id` appears in `body.obligations` with
  `contract == X`, in a state the mode accepts. This is the first code anywhere to read
  `Obligation.contract`, which until now is written to Postgres and never consulted.
- [ ] Replace `DenialReason::ContractNotCovered { required, covered }` — whose `covered` is a list of
  the certificate's own declarations — with one carrying the statements actually missing.
- [ ] `body.contracts` stops being an input to any decision. Either drop the field or verify at seal
  time that it equals computed coverage.
- [ ] `policy.required_contracts` is enforced through the same path. It is currently dead: declared,
  set by two tests, read by nothing.

### Task 4: Verdicts per contract

**Files:** `crates/ir/src/correctness/certificate.rs`, `crates/ir/src/assurance.rs`, tests

- [ ] Failing tests: a certificate whose only residual is on contract Y still opens a boundary
  requiring X at `minimum: Accepted`; the overall verdict remains the meet of the per-contract
  verdicts; a residual on X denies an X boundary.
- [ ] `AssuranceVerdict::for_contract(mode, contract, obligations)`, with the global verdict derived
  from it rather than the other way round.
- [ ] The motivating failure is a false *denial*, which is the safe direction but pushes operators to
  lower a boundary's minimum to get unrelated work through — trading a precise gate for a blunt one.

### Task 5: Verifier identity means identity

**Files:** `crates/ir/src/assurance.rs`, tests

- [ ] Failing tests: a required verifier present under a different version is denied; one whose own
  verdict was `Rejected` does not satisfy the requirement; an environment mismatch is denied; a policy
  can demand a minimum version.
- [ ] `required_verifiers: Vec<VerifierRequirement>` carrying name, version requirement, environment,
  and minimum verdict. Today the match is `record.identity.name == identity`, so a scanner that ran,
  failed, and said so satisfies a policy that requires the scanner.

### Task 6: No floating point in the kernel

**Files:** `crates/kernel/src/{ir.rs,lib.rs}`, `crates/kernel/src/tests.rs`

- [ ] Failing tests: summation is order-independent; two values differing by less than the old epsilon
  are distinguished; at a magnitude where `1e-9` falls below the representable gap, a value no longer
  compares equal to its neighbour; an unrepresentable decimal is refused rather than rounded.
- [ ] Replace `operands: Vec<f64>` and `claimed: f64` with an exact scaled integer
  (`{ units: i128, scale: u8 }`), consistent with the IR's stated no-float rule and the runtime's
  `i128` progress measure.
- [ ] Delete `ARITH_EPSILON`. Equality becomes exact, which is the only kind a kernel can defend.
- [ ] Schema major bump with a compatibility reader. Land with Task 10 to spend one migration.

### Task 7: Canonical bytes for the replay digest

**Files:** `crates/kernel/src/lib.rs`

- [ ] Failing tests: two proposals differing only in map ordering digest identically; a proposal that
  fails to encode is refused rather than digesting the empty string; the digest agrees with the IR's
  canonical encoder.
- [ ] `replay_digest` calls `serde_json::to_string(proposal).unwrap_or_default()`, so it neither uses
  the canonical encoding the rest of the system is built on, nor survives an encoding failure — every
  failed proposal collapses to the digest of `""`, and so to each other.

### Task 8: Grounding that means containment

**Files:** `crates/kernel/src/lib.rs`

- [ ] Failing tests: NFC-equivalent text matches (`é` as one codepoint against `e` plus a combining
  accent); a match spanning a sentence boundary is refused; an object matching only as a fragment
  inside a longer word is refused; a span longer than its declared bound is refused; case folding is
  Unicode-aware rather than ASCII.
- [ ] `normalize` collapses whitespace and lowercases, nothing more.
  `unicode-normalization` is already a dependency of `capsulet-ir` and used there; the kernel should
  not be doing weaker text handling than the layer beneath it.
- [ ] Containment with no locality constraint means a long document grounds nearly any short object.
  A bound on the span, and a requirement that a match not straddle a sentence boundary, is the minimum
  that makes `Cite` mean what its doc comment says.

### Task 9: `Interpret` says what it assumed

**Files:** `crates/kernel/src/{lib.rs,ir.rs}`

- [ ] Failing tests: an `Interpret` whose conclusion shares no subject with its premise is refused;
  the residual names both premise and conclusion; a chain of `Interpret` steps is bounded.
- [ ] `derive_interpret` returns `Judgment::Holds { proposition }` for whatever proposition it was
  handed, so one valid `Cite` plus one `Interpret` reaches *any* goal at `Conditional`. That is weaker
  than the design intends: the residual is meant to mark a reviewable leap, not license an arbitrary
  one.
- [ ] Keep `Interpret` unsound by design — it is the step no kernel can take. Make its residual carry
  enough to review, and stop it from being unbounded.

### Task 10: Record the rule, not just the result

**Files:** `crates/ir/src/correctness/certificate.rs`, `crates/ir/src/version.rs`

- [ ] Failing tests: a certificate sealed under verdict rule v1 still deserializes after v2 lands and
  reports which rule decided it; a certificate whose verdict disagrees with *its own recorded* rule is
  refused; a policy can require a minimum verdict-rule version.
- [ ] `CertificateBody::check` re-derives the verdict with this build's rule, so any deliberate change
  to `from_obligations` makes every historical certificate fail to deserialize — the archive becomes
  unreadable as a side effect of a policy improvement.
- [ ] `AdmissionRecord::rules_applied` already solved this problem in this codebase. Do the same:
  record the verdict rule on the body and validate against the recorded one.
- [ ] `policy_version` is recorded and never consulted. Either the gate reads it or the field goes.

### Task 11: Certificates that can go stale or be withdrawn

**Files:** `crates/ir/src/assurance.rs`, tests

- [ ] Failing tests: a certificate past its boundary's freshness horizon does not open it; a revoked
  certificate is denied naming the revocation; revocation is append-only and replays; freshness is
  decided from a passed-in time.
- [ ] `BoundaryPolicy.max_age` plus a revocation set resolved the way evidence is. A certificate is
  currently valid forever, and evidence later found to be poisoned cannot be untrusted — the only
  remedy today is to delete the certificate, which is exactly the unauditable action the append-only
  design exists to avoid.
- [ ] `decide_boundary` stays pure: time is an argument, never a clock read.

### Task 12: Replay that does not overstate

**Files:** `crates/kernel/src/replay.rs`, tests

- [ ] Failing tests: a divergent outcome cannot be read as a positive verdict through `.verdict()`;
  malformed pinned inputs are reported differently from absent ones; a certificate with no evidence
  and no verifiers reports that it had nothing to check rather than reproducing cleanly.
- [ ] `redecide` collapses "bytes missing", "bytes malformed", and "snapshot would not rebuild" into
  one `Redecision::Missing`, so tampering that breaks parsing is indistinguishable from a bundle that
  was merely incomplete.
- [ ] `ReplayOutcome::Diverged { recomputed }` can hold `Accepted`, and `verdict()` hands it out. A
  caller that reads the verdict without checking `reproduced()` gets a positive answer from a failed
  replay.
- [ ] Replay checks only what a certificate chose to claim: a body with empty `evidence` and empty
  `verifiers` passes every check trivially. `Reproduced` should carry what was actually re-decided, so
  a thorough replay is distinguishable from a vacuous one.

### Task 13: A gate that notices dead policy

**Files:** `crates/xtask/src/verify/catalog.rs`, `crates/ir/tests/`, `fuzz/`

- [ ] Failing tests: a field added to `AssurancePolicy` and read by no decision fails the gate; the
  gate passes once it is read.
- [ ] Property tests: `check` decides every generated `Rule` without panicking; `decide_boundary`
  never returns `Allowed` for a certificate that does not cover the required contract; canonical
  encoding round-trips.
- [ ] Fuzz targets for `Rule` deserialization and `check`, since both take proposer-shaped input under
  an explicit bound once Task 1 lands.
- [ ] A `correctness` gate in the fast and full profiles.
- [ ] This is the task that would have caught `required_contracts`: a field declared on the policy, set
  by two tests, and read by nothing. The tests made it look covered, which is worse than it not
  existing.

---

## Verification

Every task lands with its tests. The plan is complete when:

- the three tripwires in `known_gaps.rs` have been inverted to assert the correct behaviour;
- `crates/kernel/tests/depth.rs` runs un-ignored;
- the kernel's totality claim and `CAP-IR-002` are restored with evidence that earns them;
- `verify --profile full` passes with the new `correctness` gate;
- no decision function reads a field the party it constrains can write.

## Risks

- **Tasks 6 and 10 are schema major bumps.** Land them together, with compatibility readers, or the
  archive splits across two migrations for no benefit.
- **Task 3 changes `decide_boundary`'s signature**, which is called from the API, the worker, and the
  CLI. That is the point — those callers all have the definition already — but it is the widest blast
  radius in the plan and should land on its own.
- **Task 2 removes a `Deserialize` impl.** Anything deserializing a `TrustClass` today is relying on
  the hole; the compile errors are the audit.
- **Task 8 may reject citations that pass today.** That is the intent, but it will surface as
  previously-accepted runs going `Conditional`. Worth a dry run over stored certificates before it
  lands.
