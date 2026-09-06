# Correctness Kernel Robustness Plan

> **For agentic workers:** Implement this plan task-by-task with test-first changes and one focused
> commit per task. Task 0 comes first and is not optional: it makes the documentation honest before
> any code changes, so that a half-executed plan still leaves the project telling the truth.

**Goal:** Make the correctness kernel and the assurance gate enforce the guarantees they already
claim. Several were stated in doc comments and in the product-claims registry without being
implemented, and three were disproven by tests written against this repository. Those three are
closed; Tasks 4–13 remain.

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

Three findings were confirmed by running code, not by reading it, and all three are now closed.
Each began as a tripwire in `known_gaps.rs` asserting the **current, wrong** behaviour, so that a gap
nothing failed on still had a shape CI held. Each went red exactly when its task landed, which forced
the claim registry to be updated in the same commit. `known_gaps.rs` is gone; the behaviour each one
described is now asserted the right way round in `crates/kernel/tests/depth.rs`,
`crates/ir/tests/trust.rs` and `crates/ir/tests/assurance.rs`.

| # | Finding | Status |
|---|---------|--------|
| 1 | A boundary requiring contract `no-secrets-leaked` opens for a certificate that discharged only an unrelated `house-style` obligation. | **Proven — fixed in Task 3** |
| 2 | `TrustClass::Verified` is reachable from a hand-written record naming a certificate digest that resolves to nothing. | **Proven — fixed in Task 2** |
| 3 | `check` does not terminate with a verdict on a deeply nested derivation; the process exits with `STATUS_STACK_OVERFLOW`. | **Proven — fixed in Task 1** |

## Overclaims corrected

Three documented guarantees were stronger than the code. All three now hold; the history is kept
because how each one survived is more instructive than the fix:

- `crates/kernel/src/lib.rs:5` — "every check is total: `check` always terminates with a verdict",
  and `:45` — "Always terminates." Finding 3 disproved both. Restored in Task 1, now resting on a
  stated bound rather than on nothing.
- `CAP-IR-002`, maturity `implemented`, listed on the public surfaces `ARCHITECTURE.md` and
  `docs/architecture.md`: *"A value's trust class cannot be strengthened by assertion."* Finding 2
  disproved the headline. Its qualifying clause — "a document claiming a verdict its verification
  record does not justify is refused" — was true, and was all the evidence tests checked. The claim
  passed the registry's staleness gate because the clause is a tautology: the record is part of the
  same document, so the check compared a document against itself. Restored in Task 2, with evidence
  that resolves a certificate.
- `crates/ir/src/trust.rs:3` — "Trust never strengthens by assertion. Not by a cast, not by a
  setter, not by a field in a JSON document someone posted." The third of those was exactly what
  Finding 2 did. Also restored in Task 2.

The tautology is the pattern worth remembering: every gate this repository has was green while the
headline was false, because the claim's own qualifying clause asked nothing of the world. Task 13
catches *unread* fields, not vacuous ones.

## Chosen approach

- **Coverage is computed, never declared.** `Contract` already carried its `obligations`, and
  `Definition::contract(id)` already resolved one; the gate could use none of it because
  `decide_boundary` received only the definition's *digest*. Given the definition, coverage is a
  calculation over data that already existed. Done in Task 3, in `crates/ir/src/coverage.rs`.
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

- [x] Failing tests: a derivation far past the bound returns a certificate rejecting it rather than
  killing the process; a derivation at exactly the bound still decides; the bound is recorded on the
  certificate; the kernel reaches the same answer without the parser's help.
- [x] `CheckError::DerivationTooDeep { limit }`, and `derive` carries a depth it refuses to exceed.
- [x] `MAX_DERIVATION_DEPTH = 64`, stated by the kernel rather than inherited from the host stack.
- [x] Record the limit on the certificate, so a reader knows what bound a decision was made under
  rather than assuming today's constant.

**Two corrections to this task as it was written.**

*Deserialization was not a second open door.* The bullet said serde recurses before the kernel is
entered, so a fix in `derive` alone leaves the process killable. Measured: `serde_json` enforces a
recursion limit of 128, and each `Trust` costs two levels of JSON, so it refuses at roughly 64
nested rules — shallower than anything that threatens the stack. The kernel is bounded on the JSON
path whatever it does. That is a property of the format the caller chose, not a decision the kernel
made, so the kernel now states its own bound and `the_kernel_does_not_rely_on_the_transport_to_bound_depth`
pins the distinction. No parse-boundary work was needed.

*The walk was not the only recursion.* Bounding `derive` was not sufficient: `check` also calls
`replay_digest`, which serializes the whole proposal, and a serializer has no depth limit of its
own. The first fix left `check` still dying — before it returned — on the original case. The bound
is therefore measured iteratively at the top of `check`, before anything reads the derivation, and
an over-deep proposal is refused without being encoded.

**Two residuals, for follow-on tasks.**

`Rule` is a recursive type, so `drop`, `clone` and `serialize` still recurse in code the kernel does
not own — measured to break between 10,000 and 50,000 rules. `check` refuses such a value without
reading it and no supported transport can deliver one, so nothing is exposed today. The durable fix
is a validated constructor that bounds depth where a `Rule` is built rather than where it is used,
which would also let the pre-screen go away.

The recorded bound does not survive storage. `derivation_depth_limit` is `Option<u32>` because
`crates/postgres/src/certificates.rs` rebuilds a certificate from columns and has none for it, so a
reloaded certificate honestly reports `None` rather than being backfilled with the running build's
constant — which is the same mistake Task 10 exists to fix, in miniature. Persisting it is a column
and a migration; it belongs with Task 10's schema work rather than on its own.

### Task 2: Bind a trust record to a certificate that exists

**Files:** `crates/ir/src/trust.rs`, `crates/ir/tests/trust.rs`, `docs/contracts/product-claims.json`

- [x] Failing tests: a record naming a digest that resolves to no certificate is refused; a record
  naming a contract the certificate does not cover is refused; the verdict and residual count come
  from the certificate.
- [x] `VerificationRecord::admit(raw, &impl CertificateSource, provenance)`. The raw document supplies
  only *which* certificate and *which* contract; `RawVerificationRecord` lost its other three fields
  because a document has no standing to state them.
- [x] Remove the context-free `Deserialize for TrustClass`, along with `RawTrustClass` and
  `RecordVerdict`, which existed only to serve it.
- [x] Restore the `CAP-IR-002` headline, with evidence that resolves a certificate.

**What the removal found.** The plan expected the compile errors to be the audit, and they were, but
not where predicted. `Artifact` was one site and is unused. The other was `OutputPort::produces`,
which is inside `Graph`, inside `Definition` — a document that arrives over the wire. The field is
private, and `new()` defaults it to `Unverified`, but a derived `Deserialize` sets private fields, so
a posted definition could declare that a node produces verified output. `graph.rs` then meets those
classes to decide whether a downstream trust requirement is satisfied. The same forgery, one layer
down, and reachable from the API rather than only from a hand-written record.

Both sites now carry `#[serde(skip_deserializing)]`: the class is still written out, so a reader can
see what a port claims, and reading always lands on `Unverified`. That is the module's own rule about
which direction is safe, applied to serde — weakening on the way in is sound, strengthening is not.
`TrustClass` gained a `Default` of `Unverified` so the skipped field has somewhere honest to land.

**Residual, recorded as `CAP-IR-006`.** `provenance_complete` is the one field that could not be
derived, because nothing on a certificate records provenance loss — it is a property of the value's
path, not of the run. It is now a typed `Provenance` argument to `admit` rather than a document
field, so it cannot arrive over the wire, but it is still the caller's word. Deriving it needs
lineage the certificate does not carry.

### Task 3: Compute contract coverage from the definition

**Files:** `crates/ir/src/assurance.rs`, `crates/ir/src/correctness/certificate.rs`,
`crates/ir/tests/assurance.rs`

- [x] Failing tests: the Finding 1 probe, inverted — a boundary is denied when the required
  contract's obligations are unaccounted for, *naming* the missing ones; a contract the definition
  never declared is denied; an obligation attributed to another contract does not count; obligations
  beyond the contract do not block a crossing; `policy.required_contracts` is enforced.
- [x] `decide_boundary` takes the admitted `Definition` rather than a bare `Digest`. The digest
  equality check stays, now computed from the definition itself.
- [x] Coverage of contract X = every `X.obligations[*].id` appears in `body.obligations` with
  `contract == X`. This is the first code anywhere to read `Obligation.contract`.
- [x] `DenialReason::ContractNotCovered` now carries `missing`, the statements actually unaccounted
  for, instead of `covered`, which was a list of the certificate's own declarations.
- [x] `body.contracts` is read by no decision. Kept and documented as descriptive.
- [x] `policy.required_contracts` enforced through the same path.

**Coverage is presence, not outcome.** Every obligation a contract declares must appear on the
certificate, attributed to that contract. Whether each was discharged, waived or left open is what
the verdict already says under the run's mode; deciding it a second time here would be a quieter
copy of that rule, and the two would drift. A boundary wanting more than presence asks for it
through its `minimum`.

**Three corrections to this task as it was written.**

*The blast radius was smaller than predicted.* The plan said `decide_boundary` is called from the
API, the worker and the CLI. It is called from tests only — nothing executes from the IR through this
gate yet. The signature change cost nothing outside the test suites.

*`ContractNotCovered` had two meanings.* `check_trust_route` used the same variant to say something
different: that a value's trust was established under another contract entirely. Reshaping the
variant for coverage would have quietly changed what that denial meant, so the trust-route case got
its own `ContractMismatch`, and a definition that never declared the contract gets
`ContractNotInDefinition` rather than a "missing everything" answer that reads like a coverage gap.

*The trust path needed it too.* Task 2 left `covers()` in `trust.rs` reading the declared list, with
a comment promising this task would replace it. It could not be replaced in place: computing coverage
needs the definition, which `VerificationRecord::admit` did not have. It now takes one, and checks
the certificate is about that definition before reading its contracts — otherwise coverage would be
computed against the wrong obligations. Leaving the weak check there would have undermined Task 2's
own guarantee, since that record is what `check_trust_route` then reads.

**What the existing tests showed.** The assurance suite already used the contract's declared
obligation, so it kept passing. The two certificates *I* added in Task 2 did not — they attributed an
obligation named `patch-compiles` to contract `patch-compiles`, when the declared statement is
`compiles`. Nothing had ever compared the two, so the mismatch was invisible until coverage was
computed.

### Task 4: Verdicts per contract

**Files:** `crates/ir/src/correctness/certificate.rs`, `crates/ir/src/assurance.rs`, tests

- [x] Failing tests: a certificate whose only residual is on contract Y still opens a boundary
  requiring X at `minimum: Accepted`; the overall verdict is the weakest of the per-contract
  verdicts; a residual on X denies an X boundary; a failure anywhere denies; a contract the
  certificate is silent about is `Unverified`.
- [x] `AssuranceVerdict::for_contract(mode, contract, obligations)`, used by the gate whenever the
  boundary names a contract.
- [x] The motivating failure is a false *denial*, which is the safe direction but pushes operators to
  lower a boundary's minimum to get unrelated work through — trading a precise gate for a blunt one.

**Residuals are scoped; failures are not.** An undecided obligation of another contract is an absence
of information about this one. An obligation that was *checked and did not hold* is information: the
run produced something known to be wrong, and releasing an effect from it on the strength of an
unrelated contract would be the same passing-check-of-the-wrong-property this layer exists to refuse.
So `for_contract` scopes residuals to their contract and lets a failure anywhere reject.

**Silence is not acceptance.** A contract with no obligations on the certificate is `Unverified`,
never `Accepted` — the vacuous reading is the same shape as the hole Task 3 closed, and
`from_obligations` already had the right instinct for the empty case.

**The global verdict is unchanged.** It stays the run's summary and remains the weakest of the
per-contract verdicts; nothing about sealing or `CertificateBody::check` moves. A gate that disagreed
with the certificate it was reading would be worse than either rule alone.

### Task 5: Verifier identity means identity

**Files:** `crates/ir/src/assurance.rs`, tests

- [x] Failing tests: a required verifier at a different version is denied; one whose own verdict was
  `Rejected` does not satisfy the requirement; an environment mismatch is denied; a policy can demand
  more than the default.
- [x] `required_verifiers: Vec<VerifierRequirement>` carrying name, optional version, optional
  environment, and a minimum verdict. The old match was `record.identity.name == identity`, so a
  scanner that ran, failed, and said so satisfied a policy that required the scanner.
- [x] Selection then judgement: name, pinned version and pinned environment narrow the records down,
  and *every* record still standing must meet the minimum. A verifier that ran twice and rejected once
  has rejected; picking the run that agrees with you is the whole failure mode.

**The default minimum is `Conditional`, not `Accepted`.** The replay gate scenario caught this: its
`cargo-test` concluded `conditional`, and an `Accepted` default denied the crossing on the verifier
requirement rather than on the run's verdict — the same denial, reported less usefully. A verifier
returning `conditional` has done its job and reported a qualified result; that qualification already
flows into the certificate's verdict, where the boundary's own `minimum` judges it. The gap this task
closes is rejection being counted as satisfaction, and the default now closes exactly that and no
more. A policy wanting a particular verifier to have accepted outright sets `minimum` itself.

### Task 6: No floating point in the kernel

**Files:** `crates/kernel/src/{ir.rs,lib.rs}`, `crates/kernel/src/tests.rs`

- [x] Failing tests: summation is order-independent; `0.1 + 0.2` is exactly `0.3`; neighbouring values
  are distinguished at a magnitude where the old epsilon fell below the representable gap; a result
  that would not be exact is refused rather than rounded; trailing zeros round-trip without changing
  the value.
- [x] `Quantity` — an `i128` of units and a `u8` scale — replaces `f64` in `Rule::Arith`,
  `CheckError::ArithMismatch` and `CertificateError::corrected_value`.
- [x] `ARITH_EPSILON` deleted. Equality is exact, which is the only kind a kernel can defend.
- [x] `CheckError::ArithNotExact`, because "operands but no exact answer" is not a mismatch: reporting
  it as one would name a computed value the kernel does not have.

**The wire form is a decimal string, not `{units, scale}`.** `capsulet_ir::Decimal` already
establishes the convention and says why — "encoded as a JSON string, never as a JSON number: a number
would invite a float somewhere in the pipeline". Two decimal representations in one repository would
be a question every reader has to answer twice.

**No division, so no rounding policy.** `Sum`, `Difference`, `Product`, `Min` and `Max` are all
exactly representable. `MAX_SCALE = 18` keeps a product of two maximally-scaled operands inside an
`i128`; past that there is no exact answer and the kernel says so rather than rounding.

**No schema bump; the honest signal is coarser than it should be.** The kernel's `Proposal` is a
persisted document — bundles carry it as a pinned deterministic-family input — and it has no schema
version of its own, contrary to what `version.rs` states as the rule for every persisted root object.
So a bundle produced before this change no longer re-decides, and replay reports it as *inputs
missing* rather than as a shape it cannot read. That fails closed, which is the right direction, but
the reason is wrong; Task 12 is where that message gets sharpened. Giving `Proposal` its own schema
version is the real fix and is a task of its own. Recorded as `CAP-CORRECTNESS-007`.

### Task 7: Canonical bytes for the replay digest

**Files:** `crates/kernel/src/lib.rs`

- [x] `replay_digest` is `capsulet_ir::digest_of`, and its `Result` is handled: a proposal that has no
  canonical encoding is refused with `CheckError::ProposalNotEncodable` rather than digesting the
  empty string.
- [x] The reachable collision is gone with the floats — `serde_json` refuses `NaN`, which was the one
  input that reached `unwrap_or_default()`.

**Switching encoders changed behaviour, and the change is correct.** The canonical encoder *refuses*
text that is not Unicode NFC rather than normalising it, so that two spellings of one string cannot
produce two digests. A proposal carrying decomposed text is therefore now refused outright. That is
not in tension with Task 8: a *document* is whatever was ingested and its composition must not decide
a citation, while a *proposal* enters a digest and is asked to be normalised rather than having it
done for it — which would make the digest depend on who did the normalising. Both directions have
tests.
- [ ] `replay_digest` calls `serde_json::to_string(proposal).unwrap_or_default()`, so it neither uses
  the canonical encoding the rest of the system is built on, nor survives an encoding failure — every
  failed proposal collapses to the digest of `""`, and so to each other.

**This was blocked on Task 6, and the plan did not say so.** The canonical encoder refuses floating
point outright — `CAP-IR-001`, and `serialize_f64` returns an error — so while `Rule::Arith` carried
`f64`, switching to it would have refused every arithmetic proposal rather than digesting it. The two
are one change, and landed as one.

### Task 8: Grounding that means containment

**Files:** `crates/kernel/src/lib.rs`

- [x] Failing tests: NFC-equivalent text matches, in both directions; an excerpt longer than the bound
  is not a citation; an excerpt exactly at the bound still is; case and whitespace still do not
  matter.
- [x] `normalize` now folds Unicode composition as well as case and whitespace. NFC rather than NFD,
  to agree with the IR's canonical encoding — two layers normalising differently would disagree about
  whether the same document says the same thing.
- [x] `MAX_CITED_EXCERPT_BYTES = 4096`, with `CheckError::CitedSpanTooLong`. Checked after provenance,
  so a span that does not re-derive is reported as the fabrication it is rather than as a size
  complaint.

**Two of these bullets were wrong.**

*Case folding was already Unicode-aware.* `str::to_lowercase` is a full Unicode mapping, not an ASCII
one — measured: Turkish dotted I folds to `i` plus a combining dot, Greek final sigma folds
correctly, `ß` is handled. There was nothing to fix, and "fixing" it would have been churn against a
correct implementation.

*Refusing a match that spans a sentence boundary would reject true citations.* A verbatim quotation of
two consecutive sentences is a perfectly good citation, and the rule as written would refuse it. The
real concern underneath was locality — subject and object are matched independently, so they could sit
pages apart in a long excerpt and still both be "contained". The honest control for that is the size
of the span, not where sentences fall inside it, which is what the bound does.

**Not shipped: word-boundary matching.** An object of `cat` is still grounded by a document containing
`concatenate`. The obvious fix — require the match not to be flanked by alphanumerics — silently
breaks every script that does not put spaces between words, so it would trade a narrow false
acceptance for a broad false rejection across Chinese, Japanese and Thai. Doing it properly needs
Unicode word segmentation (`unicode-segmentation`), which is a dependency and a design decision of its
own rather than a line in this task. Recorded as `CAP-CORRECTNESS-004`.

### Task 9: `Interpret` says what it assumed

**Files:** `crates/kernel/src/{lib.rs,ir.rs}`

- [x] Failing tests: a reading without a rationale is refused; the residual names premise, conclusion
  and evidence; a chain of readings is bounded; a reading never reaches `accepted` at any authority.
- [x] A rationale is required. `CheckError::InterpretationWithoutARationale`.
- [x] `Interpret` stays unsound by design — it is the step no kernel can take.

**Most of this task was already done, and its one novel bullet was wrong.**

*The residual was already reviewable.* It carries the premise's own judgment, the conclusion, the
rationale and the cited evidence ids — enough to go straight to the passage and see the leap. A test
now pins that rather than leaving it to survive by luck.

*The chain was already bounded*, by Task 1. `MAX_DERIVATION_DEPTH` caps readings along with
everything else, so an unbounded pile of questions nobody would work through is not reachable.

*Requiring the conclusion to share a subject with the premise would refuse true readings.* "The
agreement states 30 days' notice" read as "Acme must notify within 30 days" changes the subject and
is exactly the kind of reading this rule exists to record. Syntactic overlap is not what makes an
interpretation honest.

**What was actually missing was the rationale.** It was never validated, so an empty one produced a
residual that told a reviewer nothing — a question with the question left out. That is not a smaller
problem than an unsound rule; it is the same problem, filed.

**And a doc contradiction.** `Rule::Trust` said it was "the only rule that can" turn attributed
content into an assertion, while `Interpret` reaches `Holds` too. The real distinction is discharge,
not conclusion: `Trust` clears an authority floor and can be discharged, `Interpret` never is, and a
certificate carrying its residual is `conditional`. Authority says a source is worth believing, not
that a particular reading of it is right — which is why clearing the floor is not something
interpretation can borrow. Corrected in `ir.rs`, with a test pinning that no authority discharges a
reading.

### Task 10: Record the rule, not just the result

**Files:** `crates/ir/src/correctness/certificate.rs`, `crates/ir/src/version.rs`

- [x] `crates/ir/tests/verdict_rule.rs` pins all four cases of the rule and round-trips a sealed
  certificate through `Deserialize`, so a change to `from_obligations` fails loudly with an
  explanation instead of silently closing the archive.
- [x] `CertificateBody::check` now documents that it re-derives with *this build's* rule and what that
  costs, at the place someone editing the rule would be standing.
- [x] Duplicate detection keyed on contract *and* statement — see below.
- [ ] Recording the rule on the body: **not done, and the reason is a real tension.** See below.
- [ ] `policy_version` is recorded and never consulted. Still true; carried to a later task.

**Recording the rule on the body would cause the harm it prevents.** The body is sealed: its digest
covers every field, and `Deserialize` re-verifies that digest. Adding `verdict_rule` therefore changes
the digest of every certificate, breaking every existing seal, which forces a
`CERTIFICATE_SCHEMA_VERSION` major bump. And `read_compatible` refuses any major but its own — "unknown
majors fail closed" is a stated principle of `version.rs` — so bumping makes v1 certificates
unreadable *immediately and certainly*, to protect against them becoming unreadable *later and
hypothetically*. A compatibility reader that reads older majors would change that principle, which is
a decision about the version module rather than about verdicts.

There is a good argument for doing it anyway: the archive is close to empty, since nothing has
executed IR in production, so this is the cheapest moment it will ever be. That is an argument about
*stored data this session cannot see*, so it is not mine to settle. The tripwire delivers the
protection that does not depend on the answer.

**A false rejection Task 3 created.** `check` keyed duplicate obligations on the statement id alone,
while Task 3 made `(contract, statement)` the identity. Two contracts each declaring something called
`compiles` are two obligations that share a name, not one recorded twice — and a certificate covering
both could not be sealed at all. Fixed, with tests for both directions.

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
