# ADR 0021: The Verifier Protocol's Boundary

Status: Accepted

Tracked in the [M4 milestone](https://github.com/mohripan/capsulet/milestone/7); this decision is #152.

## Context

M4 opens the correctness plane to checkers Capsulet does not contain: compilers, test runners,
policy engines, parsers, people, and other models. [M1's completion report](../contracts/m1-completion.md#entry-conditions-for-the-next-milestone)
asks for this decision before any M4 change lands: what a validator receives and returns, how it is
isolated, and how its result enters a certificate.

Two failures decide what the boundary has to do. Both are already recorded, and neither is a bug in
the kernel. They are the two things it cannot decide.

- **Reading.** An answer can quote a contract byte for byte and still get its legal meaning wrong.
  The quoted sentence says a party may terminate on notice, and two clauses later "subject to
  Section 12" makes that right conditional. Or a defined term such as "Affiliate" means something
  narrower than its everyday sense. The kernel checks that the quote is exact, that both terms
  occur, and that the sentence uses a declared phrasing. Whether the passage *means* the
  proposition is `Interpret`, which the kernel records as a residual and never discharges
  ([correctness architecture](../design/correctness-architecture.md#the-verifiability-boundary)).
  The finite cue lists that stand in for meaning are a named limitation, `CAP-CORRECTNESS-012`.
- **Responsiveness.** Asked which company acquired Contoso, about a text that does not say, a model
  answered `Dana Whitfield | is the named account owner | Dana Whitfield`. That fact is grounded and
  accurately cited, and the kernel accepted it. The certificate states that a claim is grounded.
  Nothing in it states that the claim answers what was asked, and nothing records that this went
  unchecked ([what the kernel guarantees](../design/correctness-architecture.md#what-the-implemented-kernel-does-and-does-not-guarantee)).

Opening the plane to outside verifiers makes the first failure tempting to "fix" the wrong way: put
a model in the verifier seat, let its agreement discharge the reading, and report `accepted`. The
certificate model would allow it today. `DischargeState::Discharged` names who discharged an
obligation, not what kind of checker it was, so a model's opinion would read exactly like a
recomputed sum. That is the laundering the architecture warns about, moved from a confidence field
into a certificate.

## Decision

### 1. The kernel stays closed

The kernel keeps its fixed rule set, no model, no network and no I/O. External verifiers never add
rules to it. They appear in certificates as verifier records beside the kernel's families, as
`family.rs` already promises. A replayer re-decides only what is deterministic and says plainly what
it took on trust.

### 2. Both undecidable steps become obligations a certificate names

Neither is solved, and neither is left implicit.

**Responsiveness has a mechanical part and a reading part.** A question can be pinned to a typed
**query** over the contract's relations: the relation asked about, the terms it binds, and the slot
it wants filled. "Which company acquired Contoso?" is `acquired(?, Contoso)`. Whether a goal answers
a query is decidable, and the kernel decides it:

- the goal's predicate must be the asked relation;
- each bound term must match, as written or through a confirmed name group, and the step names the
  resolutions it relied on;
- the wanted slot must be filled.

A goal that answers a different query is rejected as non-responsive, and the proposer owns the
repair. The question is pinned in the snapshot, beside the authority floor, because the party a
check constrains does not get to choose what it is checked against.

Turning a sentence into a query is a reading, and it goes where readings go:

- **A query the caller supplies** is no reading at all.
- **A query the proposer supplies** for a question asked only in words is a residual. It names the
  question, the query and the proposer's rationale, so the reviewer judges one small thing:
  whether `acquired(?, Contoso)` is what the sentence asks. The answer's fit to that query is then
  checked mechanically.
- **An answer to a pinned question that does not say how it read the question** is refused, in the
  same way an interpretation without a rationale is refused (`CAP-CORRECTNESS-006`).

A snapshot with no pinned question decides exactly as before. Such a certificate is about a claim,
not an answer, and says so.

**Reading has a mechanical part too: what the cited span leaves out.** Most legal misreadings of an
exact quote are quotes out of context. The quoted clause is qualified elsewhere ("subject to",
"notwithstanding", "except as provided in"), refers to another provision, or uses a defined term.
Whether the reading is right stays undecidable. Whether the derivation cited the provisions that
could change it is decidable. A contract can declare:

- qualifier cues;
- a reference grammar;
- how the document marks out its provisions and defines its terms.

The kernel applies those templates to the stored text itself, so no external parser is needed and
the extraction is re-decided offline like every other mechanical step. An earlier draft of this
decision had a separate parser produce that map and pin it as evidence (#157).

The kernel then leaves a residual naming each qualifying provision or definition the span depends on
and the derivation does not cite. The mechanism is generic. Every cue, pattern and term comes from
the contract, so a legal pack supplies legal ones and the kernel learns nothing about law.

### 3. Every discharge says what kind of checker made it

A discharged obligation records its **basis**:

| Basis | Who | A replayer can |
| --- | --- | --- |
| `mechanical` | a deterministic kernel family | re-decide it offline from the pinned inputs |
| `replayable_tool` | an external tool with no network and no model, in an environment pinned by digest: a compiler, a test runner, a policy engine | re-run it in that environment; the kernel does not re-decide it |
| `human_review` | a named person holding the authority the obligation names | check who decided, under which authority, from which evidence; not re-decide it |

A model is not on that list. **A model-backed verifier never discharges an obligation.** Its result
is recorded as an *endorsement* or a *dispute* on an obligation that stays outstanding. Each records
the model's identity and version, its independence from the proposer, and the calibration record it
was admitted under. That keeps a model useful without letting it certify:

- it orders a review queue, so disputed readings are looked at first;
- it measures, because endorsement rates against human outcomes are the calibration;
- a dispute can block release under a policy that says so.

It cannot turn `conditional` into `accepted`. Independence is required, not assumed. An endorsing
model from the proposer's own family is recorded as such and policy can refuse it, because one model
agreeing with itself is not a second reading.

Policy decides which bases a required obligation accepts. A legal pack can require `human_review`
for every reading under `enforce`, and a code pack can accept `replayable_tool` for "the tests
pass". An obligation discharged on a basis its policy does not accept stays outstanding. The verdict
rule changes only additively: a sealed certificate written before bases existed reads as it did,
with every discharge in it read as `mechanical` or as a declared oracle, as recorded.

### 4. What a validator receives and returns

A validator is a program in an isolated environment that speaks a versioned protocol
(`capsulet.verifier/v1`) over standard input and output.

- **It receives** one request:
  - the protocol version;
  - the obligations it is asked to decide, with their statements and contracts;
  - its declared parameters;
  - its inputs as content-addressed, read-only files.

  It receives nothing else: no credentials, no database, and no network unless a capability grants
  it.
- **It returns** one response, with one result per obligation: `holds`, `does_not_hold` or
  `cannot_decide`, each with a reason a reviewer can act on. Anything it produces is written to an
  outputs directory. A `cannot_decide` stays residual. It is never read as either answer.
- **The host does the bookkeeping, not the validator.** It digests every input and output, pins the
  environment by image digest, enforces time, memory and capability limits, and builds the
  `VerifierRecord`. The validator does not choose its basis. That comes from its registration in a
  signed pack and is checked against its capabilities, so a validator granted network or model
  access cannot be registered as `replayable_tool`.
- **Isolation** is the runner's existing boundary ([ADR 0009](0009-runner-boundary-and-stub-runner.md)).
  A container is pinned by digest, with a read-only root and inputs, a writable outputs directory,
  no network by default, and resource limits. A plain local process is supported only by the
  development harness, and its records say so.

### 5. Domain packs carry everything domain-specific

A **domain pack** is a signed, versioned bundle of:

- contracts, relations, phrasings and cues;
- qualifier and reference declarations;
- validator registrations with their bases and capabilities;
- the policy defaults for its obligations.

The kernel and runtime contain no domain-specific code, and M4's gate is three unrelated packs (legal
clauses, code changes, infrastructure policy) using the same IR and certificate protocol without an
exception for any of them.

## Consequences

- The question "does this answer what I asked" gets an honest answer for the first time: checked
  against a typed query, conditional on a reviewed reading of the question, or refused for not
  saying how it read the question. The case where the kernel accepted a grounded fact that answered
  nothing is rejected once a query is pinned, and conditional when only the words are.
- `accepted` keeps its meaning. Every step was decided mechanically, re-run in a pinned tool, or
  decided by a person with authority, and the certificate says which, step by step. More answers
  will be `conditional` than before, and more of those will say exactly what to review.
- The `ask` endpoint changes behaviour. An answer to a question asked only in words is `conditional`
  until its reading of the question is reviewed. Callers who want `accepted` send a typed query.
- Model verifiers are useful without being trusted, and their usefulness becomes measurable, which
  is what deciding to trust them later would need.
- Context completeness narrows the reading gap without closing it. A reading of a fully cited clause
  can still be wrong, and the residual still says so.

## Not decided here

- How calibration records are produced and how often they expire. That belongs to the evaluation
  work in the MLOps milestone (#110).
- The pack signature scheme and key distribution, decided in #158 against the existing supply-chain
  signing ([supply chain](../security/supply-chain.md)).
- Multi-turn questions, and questions whose answer is a set or a comparison. The first query form is
  one relation and one wanted slot, or none for a yes-or-no question.

## What would make this wrong

- If reviewers cannot judge question readings much faster than they judge whole answers, splitting
  responsiveness buys nothing. The labelled corpus (#162) measures it.
- If most real questions do not fit one relation and one slot, the typed query is the wrong first
  form. The same corpus counts how many do.
- If a pack cannot express its workload without a kernel or runtime change, the protocol is not
  general. M4's gate (#163) is built to fail in exactly that case.
