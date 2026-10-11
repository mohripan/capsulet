# Citation Grounding Plan

> **For agentic workers:** Implement this plan task by task, test first, with one focused commit per
> task. Task 0 comes first and is not optional: it makes the documentation and the claim registry say
> what the kernel does today, so that a half-executed plan still leaves the project telling the truth.
>
> **Status (2026-10-11):** Tasks 0–7 are done. Tasks 8–9 await the next go-ahead; read
> [the handover](../handoffs/2026-10-11-citation-grounding-handoff.md) before starting. It records the
> conventions this plan depends on, the traps found so far, and the decisions that belong to the owner.

**Goal:** Make `accepted` mean what the documentation says it means. Today a citation is accepted
when the proposition's subject and object both occur somewhere in the cited span. Nothing checks the
relation between them, so a span that negates, reverses, hedges, or states a different relation
between the same two terms grounds the proposition, and the certificate says `accepted`. The
comparison underneath is substring matching on lowercased text, which is too loose in one direction —
a term inside a longer word or number — and too strict in the other — the same value spelled
another way.

**The one mistake:** a citation grounds a proposition's *nouns* and concludes its *statement*. `Cite`
checks the subject and the object against the span, then concludes `Says(source, P)` with the whole
of `P`, predicate included. The predicate is written by the proposer and derived from nothing. That is
the robustness plan's rule — *for every field a decision reads, either it is derived from something
the writer cannot forge, or the decision must not read it* — broken at the centre of the kernel, and
two smaller instances sit next to it: the goal a derivation is checked against, and the authority
floor `Trust` enforces.

The rule this plan applies: **every equivalence the kernel relies on is data somebody reviewed, never
a heuristic.** A relation is stated when a phrasing its contract declares is found. An entity is named
when a name a reviewer approved is found. Two values are equal when a fixed, versioned grammar reads
the same value from both. Everything else reaches `Holds` only through `Interpret`, which is
`conditional`. The kernel stays model-free, and what it does not establish, it says it did not.

---

## Entry conditions

The robustness plan is done (see its *Where this landed*). `verify --profile fast` passes at 7/7 on
`9da34e8`.

## Evidence

Every row was measured by running code, not by reading it: a scratch crate outside the workspace
called `check` at `9da34e8`. Every proposal has the shape `capsulet-proposer`'s `build_proposal` gives
an answer the model marks as a direct quote — `Trust { Cite(goal), min_authority: "low" }`.

**Does the source state it?**

| # | Source text | Proposed fact | A reader says | Kernel | Closed by |
|---|---|---|---|---|---|
| 1 | Fabrikam did not acquire Contoso. | Fabrikam · acquired · Contoso | not stated | `accepted` | Task 5 |
| 2 | Contoso acquired Fabrikam in 2024. | Fabrikam · acquired · Contoso | not stated | `accepted` | Task 5 |
| 3 | Contoso sued Fabrikam over a patent. | Contoso · was_acquired_by · Fabrikam | not stated | `accepted` | Task 5 |
| 4 | Fabrikam may acquire Contoso next year. | Fabrikam · acquired · Contoso | not stated | `accepted` | Task 5 |
| 5 | The CEO announced the annual results. | CEO · is_named · Ann | not stated | `accepted` | Task 4 |
| 6 | The notice period is 300 days. | notice period · in_days · 30 | not stated | `accepted` | Task 4 |
| 7 | Revenue rose to $1.2 million in 2025. | Revenue · rose_to · $1,200,000 | stated | `rejected` | Task 7 |
| 8 | The contract was signed on March 1, 2026. | contract · signed_on · 2026-03-01 | stated | `rejected` | Task 7 |
| 9 | IBM acquired Red Hat in 2019. | International Business Machines · acquired · Red Hat | stated | `rejected` | Task 8 |
| 10 | Acme’s renewal was approved. | Acme's renewal · status · approved | stated | `rejected` | Task 4 |
| 11 | Acme is located at Hauptstraße 5. | Acme · located_at · HAUPTSTRASSE 5 | stated | `rejected` | Task 4 |

**What is a derivation checked against?**

| # | Probe | Kernel | Closed by |
|---|---|---|---|
| 12 | A goal `(Acme\|x, renewed, Contoso)` against a derivation of `(Acme, x\|renewed, Contoso)` — two different propositions with one `subject\|predicate\|object` rendering. | `accepted` | Task 2 |
| 13 | A bare `Cite`, with no `Trust`, of a low-authority source. The goal is reached by attribution alone. | `accepted` | Task 2 |
| 14 | `Trust` over a low-authority source, at the floor the shipped proposer always asks for. | `accepted` | Task 3 |

Rows 1–6 and 12–14 are the dangerous direction: `accepted` is the verdict the architecture describes
as "a real guarantee". Rows 7–11 are the safe direction, and still worth closing: a kernel that
rejects true statements trains its users to route around it.

**Two statements the robustness plan made are not borne out.** Its Task 8 records that lowercasing
"was already Unicode-aware … `ß` is handled". `str::to_lowercase` does produce the right letters, which
is what was measured. Matching needs both sides to fold to the *same* letters, and lowercasing does
not: `ß` stays `ß` while `SS` becomes `ss`, and Greek sigma lowercases to `σ` or `ς` depending on
where it sits. Row 11 is the measured consequence. Case folding is the operation built for matching.

## Overclaims to correct

Task 0 restates each of these to what holds today:

- `CAP-CORRECTNESS-001` — "accepts a pinned citation when it re-derives and contains the cited
  proposition". It contains the proposition's subject and object, never its predicate.
- `docs/architecture.md` — "Nothing is established by being asserted". A citation's predicate is.
- `docs/design/correctness-architecture.md` — the rule table gives `Cite` as checking that "the span
  exists and covers the quoted literal", and calls `accepted` "a real guarantee" without saying what
  it is a guarantee of.
- `crates/kernel/src/ir.rs` (`Rule::Cite`) and `crates/kernel/src/lib.rs` (`derive_cite`) — both
  still say only the object is checked, and both describe the conclusion as more than the check.

## Chosen approach

- **A statement is earned by a declared phrasing.** A relation is stated in a span only when a
  phrasing its contract declares — `"{from} acquired {to}"` — is found within one sentence, each slot
  holding the term in its own role, with no negation or hedge cue in that sentence. Where both terms
  are present and no phrasing matches, `Cite` concludes `Mentions(source, P)`: true, checkable, and
  not something `Trust` will lift. From a mention, the only way to `Holds` is `Interpret`.
- **The kernel decides which path an answer takes, not the model.** Today the model's own
  `needs_interpretation` flag chooses between `Trust` and `Interpret`. Once phrasings exist, the
  proposer harness runs the same deterministic check the kernel runs, and the flag stops deciding
  anything.
- **Equivalence is reviewed data, pinned with the decision.** Phrasings and cues live in the memory
  contract, entity names come from approved entity resolution, and value grammars are fixed in the
  kernel and versioned with it. Each is carried in the snapshot a decision is made against, so replay
  re-decides from the same data.
- **Text is compared as text, at word boundaries.** Canonical caseless matching (NFD, full case
  folding, NFC); default-ignorable code points removed; a short, explicit table for typographic quotes,
  apostrophes and dashes; whitespace collapsed; and a match counts only between two UAX #29 word
  boundaries. Not NFKC: it rewrites `x²` as `x2` and `½` as `1⁄2`, and would make different numbers
  equal.
- **Ambiguity is a residual, never a choice.** A value the grammar reads two ways is recorded with
  both readings, pinned to the span, rather than resolved by the kernel.
- **The rules a decision was made under are named.** Every task below changes what `check` decides,
  so every task bumps the family's rule version, and replay says when it re-decides a record under
  rules other than the ones that decided it.

## Scope boundaries

- No learned component in the kernel. Entailment and NLI models stay outside it. Discharging
  `Interpret` residuals with a declared oracle is stage 5 of the correctness architecture, not this
  plan.
- Provenance stays byte-exact. Re-deriving an excerpt from source bytes, content hashes, evidence
  digests and seals are integrity checks; a fuzzy integrity check is not one. None of them change.
- `Interpret` stays unsound by design. This plan moves cases out of it only where reviewed data makes
  the step mechanical.
- English phrasings and cues first. The mechanism is language-neutral — phrasings and cues are
  contract data — and lists for other languages are per-domain contract work.

---

### Task 0: Say what is true now

**Files:** `crates/kernel/tests/known_gaps.rs`, `crates/kernel/src/{lib.rs,ir.rs}`,
`docs/contracts/product-claims.json`, `docs/contracts/product-claims.md`, `ARCHITECTURE.md`,
`docs/architecture.md`, `docs/design/correctness-architecture.md`

- [x] Land rows 1–14 as tripwires in `known_gaps.rs`, each asserting today's verdict with what a
  reader would say beside it. Each goes red exactly when the task that fixes it lands, which forces
  the claim registry to change in the same commit.
- [x] Restate `CAP-CORRECTNESS-001` to what is checked.
- [x] Record limitations: `CAP-CORRECTNESS-009`, the predicate is not checked; `CAP-CORRECTNESS-010`,
  the goal is compared as a rendering, so attribution stands in for assertion; `CAP-CORRECTNESS-011`,
  the proposer chooses its own authority floor. Restate `CAP-CORRECTNESS-004` to cover both
  directions of the text relation, with the tripwires as its evidence.
- [x] Correct the doc comments on `Rule::Cite` and `derive_cite`.
- [x] `verify --profile fast` passes; the registry renders clean.

### Task 1: Name the rules a decision was made under

**Files:** `crates/kernel/src/{family.rs,replay.rs}`, `crates/kernel/tests/replay.rs`

- [x] Failing tests: the claim-reasoning family's version is its rule version, not the crate version;
  a full replay of a real proposal and snapshot re-decides and reproduces; a record decided under
  other rules is re-decided and says so.
- [x] `CLAIM_REASONING_RULES`, bumped by every later task in this plan.
- [x] `ReplayNote::FamilyRulesDiffer`. A note rather than a finding: the re-decision still runs, and
  `FamilyDisagrees` stays disqualifying. A certificate that today's rules would not accept should not
  reproduce. The note tells a reader that the disagreement may be the rules rather than the bundle.

`ClaimReasoning::version()` returns `CARGO_PKG_VERSION`, which has been `0.1.0` through every rule
change so far: floating point to fixed point, NFC, the span bound. Nothing a replay reads could tell
the rule sets apart. Separately, no test re-decides a real proposal and snapshot end to end — only the
absent and unreadable cases are covered — so the path every later task changes is untested.

### Task 2: A goal is a fact, and is compared as one

**Files:** `crates/kernel/src/{lib.rs,ir.rs}`, `crates/kernel/src/tests.rs`,
`crates/kernel/tests/grounding.rs`

- [x] Failing tests: a bare `Cite` does not satisfy a goal and `Trust(Cite)` still does; a derivation
  whose rendering equals the goal's but whose fields differ is refused; `Attest`, `Arith`, `Trust` and
  `Interpret` still reach their goals.
- [x] `check` compares the derived judgment with `Holds(goal)` field by field, trimmed as today.
  `canonical()` stays as a display form that no decision reads.
- [x] The grounding tests stop going through the goal. They pin what a citation discharges, so they
  assert on the `cite` step rather than on a verdict whose meaning this plan is changing.

Every rule that concludes a proposition concludes `Holds` — `Trust` after an authority check, `Attest`
from an active claim, `Arith` after recomputing, `Interpret` with a residual — except `Cite`, which
concludes `Says`. Comparing propositions alone let `Says` stand in for `Holds`, so `Trust` was
optional: a proposer that left it out reached `accepted` from any source (row 13).

**One thing the task found.** Comparing fields fixed the decision, but the rendering was still
ambiguous, and `GoalNotDerived` shows a reader two renderings side by side: for row 12 it would have
printed the same string twice and called them different. `canonical()` now escapes `|` and `\`
inside a term. Text with neither renders exactly as before, so no existing certificate reads
differently; the escape character needed escaping too, or `("x\\", "y", "z|w")` and
`("x|y", "z\\", "w")` would still collide.

### Task 3: The authority floor is policy

**Files:** `crates/kernel/src/{lib.rs,snapshot.rs,snapshot_document.rs}`,
`crates/api/src/reasoning.rs`, tests

- [x] Failing tests: a proposal asking for `low` against a snapshot whose floor is `medium` is held to
  `medium`; a proposal may ask for more than the floor and is held to that; a snapshot document
  without a floor rebuilds with `low`, so a bundle written before this task re-decides as it did.
- [x] The floor travels with the snapshot, because it is part of what a decision was made against.
  `Trust` enforces the stricter of the floor and the proposal's own `min_authority`. A proposal can
  raise the bar, never lower it, and its shape does not change.
- [x] `/v1/reasoning/ask` sets the floor explicitly, at `low` — today's effective behaviour — so
  changing it becomes a decision someone makes rather than a field the model fills in.

**Its tripwire did not go red, and was not meant to.** Row 14 was a low-authority source clearing
`Trust` at the floor the proposal asked for. With the policy floor defaulting to `low`, that case
still decides the same way, so the tripwire was retired by hand and its claim restated as a
capability resting on the new tests. What changed is who decides: the floor is now a field the
proposer cannot lower, and `/v1/reasoning/ask` still sets it to `low`. Whether that endpoint should
hold sources to more is a product decision this task deliberately did not make.

### Task 4: A term is a whole word, in any case and any typography

**Files:** `crates/kernel/src/{lib.rs,text.rs}`, `crates/kernel/Cargo.toml`, `Cargo.toml`,
`crates/kernel/tests/grounding.rs`

- [x] Failing tests: `Ann` is not found in `announced`, nor `30` in `300`; `苹果`, `แอปเปิ้ล` and
  `コンピュータ` are still found inside their sentences; `HAUPTSTRASSE` finds `Hauptstraße`, `finance`
  finds `ﬁnance`, and `Acme's` finds `Acme’s`; a soft hyphen or zero-width space inside a word does
  not hide it.
- [x] The text relation moves to `text.rs`: NFD, full case folding, NFC; default-ignorable code
  points dropped; quotes, apostrophes, dashes and minus signs folded through an explicit table;
  whitespace collapsed; a match counts only when it starts and ends on a UAX #29 word boundary of the
  folded text.
- [x] `unicode-segmentation` for boundaries — rules only, no dictionary and no model. `icu_casemap`
  for folding, matching the ICU crates the workspace already locks.
- [x] Restate `CAP-CORRECTNESS-004` to what remains; extend `CAP-CORRECTNESS-005`.

**Why this is safe for scripts written without spaces.** The robustness plan deferred word boundaries
because a flanking-letter rule would reject correct citations in Chinese, Japanese and Thai. Measured
before writing this task: UAX #29's default rules give each Han ideograph, each Hiragana character
and each Thai grapheme its own boundary, and keep a Katakana run together. A boundary rule therefore
refuses `Ann` in `announced` and `30` in `300` while still finding `苹果` in `我喜欢吃苹果` and
`แอปเปิ้ล` in `ฉันชอบกินแอปเปิ้ล`. Those scripts keep today's behaviour rather than losing it.

**Two corrections to this task as it was written.**

*`CAP-CORRECTNESS-004` could not be closed.* Task 0 widened it to both directions of the text
relation, and rows 7–9 — a number or date written another way, another name for an entity — are
not about words at all. They are value and entity equivalence, Tasks 7 and 8. The claim now says
only that.

*Unicode's word boundaries needed one tailoring.* Measured before choosing: UAX #29 keeps
`acme's`, `l'accord` and `can't` whole, as single words. Taken as they are, a possessive would have
hidden the name it is attached to and French elision half its nouns — a broad false rejection, the
kind that sends users around the kernel. An apostrophe is therefore a boundary on both sides. The
cost is narrow and named: `can` is found in "can't" and `Brien` in "O'Brien".

The segmentation and normalisation tables are at Unicode 17.0.0, pinned by a test that has to change
together with `CLAIM_REASONING_RULES`. The case-folding and default-ignorable data come from ICU4X
and are pinned by `Cargo.lock`; only `icu_casemap`, its data crate and `unicode-segmentation` were
new to the lock, because the rest of the ICU closure was already there through `idna`.

### Task 5: A relation is stated only in a phrasing its contract declares

**Files:** `crates/core/src/domain/memory_contract.rs`, `docs/memory-contracts/*.contract`,
`crates/kernel/src/{ir.rs,lib.rs,error.rs,snapshot.rs,snapshot_document.rs,phrasing.rs}`,
`crates/proposer/src/ollama.rs`, `crates/api/src/reasoning.rs`, tests

- [x] Failing tests: rows 1–4 stop reaching `accepted`; `"{to} was acquired by {from}"` grounds a
  passive sentence; a phrasing that spans a sentence boundary does not ground; a cue in the matched
  sentence does not ground; a predicate with no declared phrasings is a mention, never a statement.
- [x] Contract syntax, on relations:

  ```text
  relation acquired:
    from: Organization
    to: Organization
    stated_by:
      - "{from} acquired {to}"
      - "{to} was acquired by {from}"
      - "{from}'s acquisition of {to}"
  ```

  The parser refuses a phrasing without exactly one `{from}` and one `{to}`, or without a literal
  word. `"{from} {to}"` would state every relation at once.
- [x] `Judgment::Mentions`. `Cite` concludes `Says` when a declared phrasing of the proposition's
  predicate matches inside one UAX #29 sentence of the span, slots filled by the subject and object
  under Task 4's text relation, and no cue in that sentence. Otherwise it concludes `Mentions` when
  both terms are present, as today, and is refused when they are not.
- [x] A cue list ships with the kernel, versioned with its rules: negation (`not`, `no`, `never`, the
  `n't` contractions, `neither`, `nor`, `without`, `denied`) and hedging (`may`, `might`, `could`,
  `would`, `if`, `whether`, `plans to`, `expected to`, `reportedly`, `allegedly`). A contract may add
  to it, never remove from it.
- [x] `Trust` refuses a `Mentions` premise with `RelationNotStated`, routed to the proposer. The
  repair is to propose a reading, which needs no new retrieval.
- [x] The discharged step names the phrasing and the text it matched, not just that terms were found.
- [x] Phrasings and cues are pinned in the snapshot document, additively. A document written before
  this task has none, so nothing in it is a statement, and replay says it re-decided under other
  rules (Task 1).
- [x] Proposer: `build_proposal` chooses `Trust` or `Interpret` by running the kernel's phrasing
  check instead of reading `needs_interpretation`.
- [x] Proposer: the prompt offers the contract's relations as a closed set of predicates, the way the
  evidence alphabet closes the set of citable ids.
- [x] `/v1/reasoning/ask` takes the contract to reason under. Without one nothing is declared, every
  answer is a reading, and the verdict is `conditional` — which is the truth about an answer that no
  contract stands behind.
- [x] Close `CAP-CORRECTNESS-009`; restate `CAP-CORRECTNESS-001`.

**What this does not do.** It is not comprehension. "Rumours that Fabrikam acquired Contoso were
false" still contains a declared phrasing, and no cue list will ever be complete. What changes is the
claim: `accepted` stops meaning "both words occur in the span" and starts meaning "the span contains a
phrasing the contract declared for this relation, with the right terms in the right roles". That is
narrow and checkable, and it is what the certificate says it established. Contiguous phrasings
already exclude most interposed negation — "Fabrikam has not acquired Contoso" does not match
`"{from} acquired {to}"` — and the rest is recorded as a limitation rather than implied away.

**Landing in two steps.** The kernel and the proposer harness landed first, with no contract wired
to `/v1/reasoning/ask`: until one is, nothing an answer there cites is a statement, and every answer
is a reading. The contract syntax, the predicate set in the prompt, and the endpoint's contract are
the second step.

**What the first step found.**

*The proposer's reading path had been dead since grounding checked both endpoints.* `build_proposal`
cited the quote under the subject `"quote"`, and a citation has had to contain its subject since the
fix that grounded both endpoints — so every answer the proposer routed through `Interpret` was
rejected for a missing term, and `/v1/reasoning/ask` could only ever answer `accepted` or
`rejected`. Its tests checked the shape of the derivation and never what the kernel decided about it.
A reading now cites the answer's own terms when the excerpt has them and the quote when it does not,
and every proposer test runs what it built through `check`.

*`may` is not a cue.* The list above included it. It is also a month, so "acquired Contoso in May"
would have been refused, and contiguous phrasings already exclude the hedge it would catch —
"Fabrikam may acquire Contoso" does not match `"{from} acquired {to}"`.

*Two refinements the task did not anticipate.* A single line break is read as a space before
sentences are found, because extracted text is hard-wrapped and Unicode ends a sentence at every
line break; a blank line still ends one. And a sentence that ends in a question mark asserts nothing,
however exactly it matches.

*A limitation recorded rather than hidden.* A sentence can doubt a phrasing in a word no cue list
carries — "Analysts doubted that Fabrikam acquired Contoso" still states the acquisition. That is
`CAP-CORRECTNESS-012`, with a tripwire.

**What the second step settled.** The phrasing syntax lives in `capsulet-core`, beside the contract
grammar, and the kernel uses that one definition: a contract and a pinned snapshot document cannot
disagree about whether a template is a phrasing, because there is one parser. A contract adds cues
under `statement_policy: cues:`. `/v1/reasoning/ask` resolves its contract before it stores anything,
and a contract from another project reports as missing, the way the memory routes report one. The
example contracts declare phrasings, so they show the shape rather than only parse.

### Task 6: A proposal carries a schema version

**Files:** `crates/kernel/src/{ir.rs,replay.rs}`, tests

- [x] Failing tests: a proposal from an unknown major is reported unreadable rather than malformed;
  a proposal with no version reads as v1.
- [x] `Proposal.schema_version`, read with `read_compatible` like every other persisted root.
- [x] Close `CAP-CORRECTNESS-008`.

**What the task found (2026-10-11).** An unknown version on an otherwise valid proposal was
silently ignored by Serde, so replay could reproduce `Accepted`, not merely report a future shape
as malformed. The regression first demonstrated that result. The compatibility reader now checks
the declared version before parsing the v1 shape; incompatible inputs make replay `Unreadable`,
whereas malformed v1 inputs still diverge with `FamilyInputsMalformed`. A missing version defaults
to v1 and old pinned bytes replay unchanged. The approved always-written version changes newly
computed proposal digests, not existing certificate seals; the family rules version is now `6`.
No term or phrasing rule changed, so none of the Tasks 7–8 tripwires was moved. This versions the
kernel's claim proposal, not the separate `capsulet-ir` candidate-value `Proposal` type or the
API's raw model proposal. Tracking is [Task 6 #119](https://github.com/mohripan/capsulet/issues/119)
and [the remaining-task overview #123](https://github.com/mohripan/capsulet/issues/123), in
[the citation grounding milestone](https://github.com/mohripan/capsulet/milestone/6).

**Revised 2026-10-11: independent of Tasks 7 and 8.** This task was first written as their
prerequisite, on the assumption that a term would become an entity reference or a typed value inside
the proposal. Reading the code for the handover showed a smaller design: the contract already says
what type a relation's endpoints have, and the snapshot can carry approved names, so the proposition
can stay three strings and the *contract* says how to read them. Task 6 still matters — a bundle
from a build whose proposal shape differed is reported as malformed rather than unreadable — but
nothing else waits on it. Adding the field changes the replay digest of every proposal; that digest
identifies a proposal on a certificate and no decision reads it, so the change is safe, but it is the
owner's call (see the handover).

### Task 7: Values are compared as values

**Files:** `crates/core/src/domain/memory_contract.rs`, `crates/kernel/src/{value.rs,phrasing.rs,
lib.rs,snapshot_document.rs}`, `crates/kernel/tests/{grounding.rs,known_gaps.rs}`

- [x] Failing tests: for a relation whose `to:` is `money`, "Revenue rose to $1.2 million" grounds an
  object of `$1,200,000`; for `date`, "signed on March 1, 2026" grounds `2026-03-01`; `30` is still
  not found in "300 days"; `05/01/2026` from a source with no declared locale grounds neither reading
  by itself and leaves a residual naming both.
- [x] A relation's `to:` may be a value type — `number`, `money`, `date` — as well as an entity type.
  `compile()` accepts the value types. The type travels in the phrasebook, and in its snapshot
  document form additively, so the proposal's shape does not change: the object stays a string, and
  the contract says how to read it.
- [x] Typed slots. For a typed `{to}`, both the term check and the phrasing match compare by value.
  Instantiate-and-search cannot work once the slot's text may differ from the term, so phrasing
  matching becomes slot-aware: the phrasing's literal pieces are found word-aligned within one
  sentence, and the region that fills a typed slot must parse, whole, to a value equal to the term's.
  Regions are bounded by the sentence and the 4 KiB span bound, so enumerating candidate regions
  between word boundaries stays bounded. Entity slots keep today's text relation.
- [x] One closed grammar per type, versioned with `CLAIM_REASONING_RULES`. Numbers: digit grouping, a
  decimal point, an optional sign, and the scale words thousand, million, billion and trillion,
  parsed into the kernel's exact `Quantity` and refused rather than rounded when they are not exact.
  Money: a number plus a currency from an ISO 4217 code or an explicit symbol table. Dates: ISO 8601
  and English month-name forms. No locale-dependent heuristics anywhere.
- [x] A form the grammar reads more than one way is not decided by the kernel. Numeric day/month
  dates without a locale, and a currency symbol shared by several currencies without a contract
  default, become a residual the kernel writes itself, naming each reading and the evidence. That is
  new — `Cite` has never written a residual — so `Rule::Cite`'s documentation says so.
- [x] Inverted, the two Task 7 tripwires move to `grounding.rs`; `CAP-CORRECTNESS-004` is restated to
  what remains; the rules version is bumped.

**What the task found (2026-10-11).** The approved currency/date policies live on the contract,
not the source or the machine locale. Shared symbols remain residuals unless a default is one of
that symbol's explicitly listed currencies. Numeric dates use slash-separated day/month/year;
without an order, all valid calendar readings are recorded, including both readings of
`05/01/2026`. The closed date grammar also includes calendar `YYYY-MM-DD` and full English
month names, not every possible ISO 8601 or human date notation. Money codes are pinned from
SIX List One dated 2026-09-17 (excluding XTS/XXX); the supported symbol table is `$`, `€`, `£`,
`¥`, and `₹`. Unsupported forms, malformed grouping, invalid dates, excess decimal scale and
overflow cannot discharge a typed citation. Built-in endpoint spellings are reserved for values
at `to:`; `from:` and other endpoint names still require declared entities.

The old tripwires had no type metadata, so they would correctly keep rejecting after this task:
undeclared/untyped relations still match text, not inferred values. They were given typed contract
context (and the approved USD default for the money case), went red at the term check, and moved
inverted to grounding regressions. Slot matching must also exclude prefixes of longer lexemes:
a regression caught `$30 CAD` being shortened to `$30` and incorrectly accepted as USD. It now
fails closed. Entity matching and the finite cue limitation have not been widened.

`Cite` records its value residual even without a phrasing; otherwise the proposer could lose it
when taking its interpretation path. The proposer now uses typed term presence on that path,
and actual kernel checks cover both paths. The API loads compiled, scoped contracts before writes;
a legacy stored contract with unknown endpoints is refused. Its authority floor remains `low`
and no request/response shape changed. Additive snapshot fields pin types and both policies;
unknown pinned metadata is refused and legacy documents still encode byte for byte.

Replay testing found an older fixture marking every obligation discharged regardless of its
family verdict. Consequently its platform `accepted` did not demonstrate a kernel-accepted
citation: `Contoso` did not fill the declared `{from} renewed {to}` slot in “Acme renewed the
Contoso contract.” The fixture now records discharged/residual/failed states consistently, and
the versionless-proposal test explicitly checks a genuinely accepted citation. The platform code
is unchanged: its overall verdict folds obligation states, not verifier-record verdicts alone.

The local 37-test grounding suite initially took 8.05 seconds on a 4 KiB hostile-value search.
Reading the target once, searching already-folded candidates, avoiding full boundary rescans, and
rejecting non-three-letter currency lookups early reduced that same debug regression run to
0.14 seconds; this is a local observation, not a production benchmark. The rules version is now
`7`. `CAP-CORRECTNESS-004` now names only entity-name limitations, and new capability
`CAP-CORRECTNESS-013` records exact typed grounding and ambiguity. Tracking is
[Task 7 #120](https://github.com/mohripan/capsulet/issues/120) in the citation grounding milestone.

### Task 8: An entity is found by any name a reviewer approved

**Files:** `crates/kernel/src/{phrasing.rs,lib.rs,snapshot.rs,snapshot_document.rs}`,
`crates/api/src/reasoning.rs`, tests

- [ ] Failing tests: `International Business Machines` grounds against "IBM acquired Red Hat" when a
  confirmed entity resolution joins the two names under one canonical entity; a name joined only by a
  candidate or rejected resolution does not; the discharged step names the resolution it relied on.
- [ ] Reviewed names, and only those. A resolution with status `Confirmed` is a review; the
  `aliases` fields on `Entity` and `CanonicalEntity` are written straight from API requests, with no
  review, and must not widen what a citation matches. Two entities joined by confirmed resolutions to
  one canonical entity form a name group of their `name`s.
- [ ] The snapshot carries name groups — names plus the resolution ids that joined them — pinned in
  the snapshot document additively. A term is found when it, or any name in a group containing it,
  occurs as whole words. Entity slots in phrasings match the same way. The proposal's shape does not
  change.
- [ ] `/v1/reasoning/ask` loads the confirmed groups for the caller's project, bounded, and pins them.
- [ ] No similarity score anywhere in the path. An embedding may propose a candidate resolution for
  review; only the review makes it a name.
- [ ] Inverted, the Task 8 tripwire moves to `grounding.rs`; `CAP-CORRECTNESS-004` is closed or
  restated; the rules version is bumped.

### Task 9: Measure it

**Files:** `crates/kernel/tests/statement_corpus.rs`, `docs/design/correctness-architecture.md`

- [ ] The tripwires become a labelled corpus whose invariant is asserted, not counted: no case a
  reader marks *not stated* reaches `accepted`. That is the guarantee. The one known exception — a
  sentence that doubts a phrasing in words no cue list carries (`CAP-CORRECTNESS-012`) — is listed in
  the corpus as a recorded limitation the invariant names, never dropped to make it pass.
- [ ] The share of *stated* cases that reach `accepted` is reported, and recorded here as each task
  lands. The architecture document names that ratio as the number that decides whether the approach
  works.
- [ ] Not a gate: the same numbers on real questions, from the ask loop with a real model over a fixed
  corpus. It needs a model, so it is an experiment someone runs and records, not something CI can
  hold.

---

## Verification

The plan is complete when:

- `known_gaps.rs` is gone and the corpus invariant holds;
- `CAP-CORRECTNESS-004`, `-009`, `-010` and `-011` are closed or restated, with evidence;
- `verify --profile full` passes;
- no rule in the kernel concludes more than it checked.

## Risks

- **Task 5 moves most answers to `conditional`** until contracts declare phrasings. That is the honest
  number — today's `accepted` rate is inflated by rows 1–6 — but it will look like a regression.
  Dry-run it over the certificates `/v1/reasoning/ask` has stored before it lands, so the size of the
  change is known rather than discovered.
- **Phrasings are only as good as the contract.** The parser refuses a phrasing with no literal word,
  and phrasings are reviewable data, but nothing can refuse a well-formed phrasing that is simply too
  loose. That is the trust already placed in whoever writes a contract.
- **Cue lists fail open.** A negation the list does not know still matches. The list is versioned and
  extensible, and the gap is recorded as a limitation rather than closed by pretending.
- **Tasks 7 and 8 no longer change the proposal's shape** (revised 2026-10-11): typing and names come
  from the contract and the snapshot. Each still bumps the rules version and adds snapshot fields.
- **Every task changes what replay re-decides.** Task 1 lands first so each change is named, and the
  archive of claim-reasoning records is empty today — nothing in production writes one yet — which
  makes this the cheapest moment the change will ever have.
