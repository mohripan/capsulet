# Correctness architecture

## Status

Partially implemented historical foundation. The deterministic kernel, proposer boundary, and a
narrow certificate API slice exist. The product constitution supersedes this document for the
unified IR, platform assurance, protected-boundary admission, and release roadmap. Decisions
extracted from this design are recorded in
[ADR 0012](../adr/0012-correctness-kernel-and-proposer-checker-split.md).

## Problem

Capsulet's goal is not to replace frontier models. It is to make a small model good enough to
carry out complex work, by making correctness a property of the system rather than a property of
the model.

Complex work fails on small models for a specific reason: they are poor at long chains and
roughly adequate at single hops. Any architecture that asks a small model to plan, retrieve,
reason, compose, and check in one pass inherits every weakness at once. The way out is to only
ever ask for a single hop and move composition somewhere it can be made correct by construction.

That requires being precise about what "correct" can mean, because the honest answer is narrower
than it first appears.

## The verifiability boundary

Five architectures were explored for this: proof-carrying answers, verifier-guided search,
constrained decoding, neurosymbolic entailment, and per-step contracts. Each was adversarially
reviewed. All five arrived at the same wall, in the same place.

The wall is **entailment**. Whenever a design has to decide whether a cited passage actually
*supports* a proposition, it either becomes a language model — which destroys the soundness
argument, because an unreliable component is now judging an unreliable component — or it quietly
degrades to substring matching, which certifies that text was copied rather than that a claim is
supported.

This is not an engineering gap. It is the boundary of mechanical verification over natural
language. The design therefore does not try to move it. It makes it explicit.

Mechanically decidable, with no model in the loop:

- whether an entity, claim, evidence, or source id exists;
- whether a claim is active rather than candidate, expired, superseded, or contradicted;
- whether a quoted string is byte-identical to the span it cites;
- whether arithmetic recomputes to the same value;
- whether two temporal intervals actually overlap;
- whether a predicate is declared in the governing contract, with that arity;
- whether a contradicting active claim exists, and whether a resolution rule was cited;
- whether a deterministic job produced a given output digest.

Not decidable without a model:

- whether a passage *means* what a claim says it means;
- which reading of an ambiguous sentence is intended;
- whether retrieval surfaced everything relevant;
- whether an ontology is the right carve-up of a domain.

Capsulet does not verify the second list today either. The difference this design makes is that
it stops implying that it does, and reports per answer exactly which steps were discharged and
which were assumed.

## Architecture: propose, check, certify

A node never returns a value. It returns a **proposal**: a candidate result together with the
derivation claimed to justify it. A small pure kernel decides the proposal and emits a
**certificate**. Nothing reaches run state, memory, or a caller without one.

```text
proposer            checker                    record
--------            -------                    ------
retrieval     ->    kernel (deterministic)  -> certificate
small model         no model, no network       verdict
                    no I/O beyond the store    discharged steps
                                               residual obligations
                                               replay digest
```

### The kernel

The kernel is a decision procedure. It always terminates, with an accept or a specific localized
reason. It has no network access, no model, and no I/O beyond reading the store. Its rule set is
closed and small:

| Rule | Concludes | The kernel checks |
| --- | --- | --- |
| `Cite(evidence, span)` | `Says(source, P)` | the span exists and covers the quoted literal |
| `Attest(claim)` | `Holds(...)` | the claim is active and within its validity interval |
| `Arith(op, premises)` | a numeric fact | recomputes the value itself |
| `Temporal(P, interval)` | a time-bounded fact | performs the interval algebra itself |
| `RelChain(relation, hops)` | a derived relation | walks the graph, bounded by contract policy |
| `Trust(Says(s,P), policy)` | `P` | the source priority and confidence floor are satisfied |
| `Defeat(P, conflict, rule)` | `P` despite a conflict | a declared contradiction rule authorises it |
| `Compute(job, inputs, digest)` | an oracle result | the digest matches a replayable run |
| `Interpret(Says(s,P), P')` | `P'` | **nothing. This is the residual.** |

`Interpret` is the design. The step no kernel can take is neither hidden inside a heuristic nor
faked; it is named, and it lands on the certificate as an obligation that a human, a stronger
model, or an independent second reading can discharge later.

### Verdicts

Because of `Interpret`, a verdict is not a boolean:

- **Accepted** — every step discharged mechanically, no interpretation required. A real guarantee.
- **Conditional** — sound given N named readings, each pinned to a specific span. The honest
  answer for most real questions, and it points at exactly what to review.
- **Rejected** — a premise failed. The error names which one, and which subsystem owns the repair.

### Certificates and error routing

A rejection is not a retry signal, it is a routing decision. A dangling reference means retrieval
failed: re-run the retriever, spend no model tokens. An arithmetic mismatch is auto-repairable
with no model call at all, because the kernel's computed value is authoritative. A span mismatch
returns the actual span text, which is a strictly smaller and fully specified subgoal rather than
"try again".

This error taxonomy is worth building even if the rest of this document is abandoned. It replaces
`AgentRuntimeError::NodeExecutor { message: String }`, which discards the one piece of information
a repair loop needs.

## Where learned components belong

The kernel must contain no learned component. That constraint does not exclude learned components
from the system — it fixes where they sit.

### Knowledge graph embeddings as a proposer

[Quaternion Knowledge Graph Embeddings](https://arxiv.org/abs/1904.10281) (Zhang et al., NeurIPS
2019) represents entities as quaternion vectors and relations as rotations under the Hamilton
product, scoring the plausibility of a triple. Its output is a scalar score and a ranking. It
carries no proof and offers no guarantee, and it captures symmetry, antisymmetry, and inversion
but not composition.

That makes it unusable as a verifier and genuinely useful as a proposer. Three fits, strongest
first:

1. **Search guidance.** Search over candidate derivations needs an ordering heuristic. An
   unsound heuristic guiding a sound checker is the same arrangement used by SAT solvers and
   superposition provers: the heuristic affects only speed, never correctness.
2. **Building the evidence alphabet.** Retrieval's product is a closed, run-pinned set of legal
   ids. Embedding similarity is a reasonable way to generate that candidate set; the kernel then
   constrains generation to it and verifies every citation against spans.
3. **Gap and conflict detection.** High-scoring absent triples are worth proposing for review;
   low-scoring present ones are worth flagging. These enter the existing review queues as
   candidates and are never promoted by a score.

The division of labour is convenient: the embedding is good at single-hop plausibility and does
not model composition, which is exactly the work `RelChain` already does by walking the graph.

The governing rule:

> The embedding decides what to try. The kernel decides what is true. An embedding score never
> appears in a certificate.

### The confidence-laundering trap

`Claim` carries a `confidence` field and contract policy gates admission on a confidence floor.
Writing embedding scores into that field would launder an unsound score into an admission
decision, and it would be invisible on inspection later. Learned scores must occupy a separate,
differently named field that the kernel is structurally unable to read.

### Caveats on the embedding choice

- Knowledge-graph-embedding benchmark results are known to be sensitive to tuning, and new models
  need comparing against well-tuned baselines rather than published numbers. Treat the 2019
  results as claims against 2019 baselines.
- Embeddings need a populated graph. A new tenant has none, and populating it is what ingestion
  is for. This is a late-stage capability, not a foundation.
- Training, negative sampling, evaluation, and retraining as the graph mutates are a new
  subsystem the project does not currently have.
- The choice between quaternion, complex, and real-valued embeddings is an empirical modelling
  question. The architecture must not depend on the algebra.

## What the implemented kernel does and does not guarantee

Stage 0 and the kernel are implemented. Two limitations surfaced in live testing
against `qwen2.5:1.5b` and are recorded here because both are properties of the
boundary rather than bugs to be fixed.

**Grounding is checked on both endpoints, not on meaning.** `Cite` requires that
the subject and the object of a proposition both appear literally in the cited
span, and that the span re-derives byte-for-byte from stored source content. An
earlier version checked only the object; asked "which company acquired Contoso?"
the model attached the subject `Contoso` to a sentence about anniversary dates,
purely because a matching string appeared there, and the kernel accepted it.
Grounding both endpoints rejects that. It still does not establish that the span
*entails* the proposition — that remains `Interpret`, and remains a residual.

**Relevance is not checked at all.** Asked the same unanswerable question after
the fix, the model returned `Dana Whitfield | is the named account owner |
Dana Whitfield` — a correctly grounded, accurately cited fact that does not
answer the question, and the kernel accepted it. This is correct behaviour: the
certificate asserts that a claim is grounded in its source, not that it responds
to the caller's question. Whether an answer is responsive is a second undecidable
axis, and any mechanism that judged it would be a model in the verifier seat.
Treat it the way interpretation is treated — as an explicit obligation on the
caller or a review step — rather than something the kernel can absorb.

## Why this targets small models

Three arguments, in descending order of confidence.

**Composition moves out of the model.** The model is only ever asked to extract one proposition
from one span. Transitivity, temporal intersection, arithmetic, conflict resolution, and
multi-step framing move into the kernel, where they are correct by construction. The model's job
shrinks until it fits the model available.

**Refutation is total; generation is not.** The kernel always terminates with a verdict, which
converts "produce a correct answer" into "produce a candidate that survives a cheap total check".
That is the asymmetry that made machine-checked proof tractable with weak automation. It holds
only where checking is genuinely cheaper than generating — that is, over the decidable list above.

**Errors route repairs instead of re-prompting.** Most apparent model failures are retrieval
failures, arithmetic failures, or stale-reference failures. A typed error sends each to the
subsystem that owns it, and several classes need no model call at all.

## Impact on the current code

Each of the following was verified against the working tree rather than assumed.

**The runtime type-checks the pipes and ignores the water.** `GraphDefinition::new` validates that
hyperedge endpoints agree, but the check in `crates/core/src/domain/graph.rs` compares
`PortValueType` discriminants, and that enum is eleven nominal tags ending in `Json`. Meanwhile
`crates/application/src/agent_runtime.rs` assigns `run.state_json = outcome.state_json`: the
content itself flows between nodes as an opaque string nothing inspects. The typing is
ceremonial, and this assignment is where the architecture has to change.

**The memory model already encodes proof obligations.** `Claim`'s constructor refuses to build
without evidence. `SummaryTrace` is a conclusion together with its premises. `MemoryContract` is a
signature: sorts, field types, arities, and a trust ordering. What is missing is the judgment
connecting them and the kernel that decides it.

**Citations are not re-checkable.** `Evidence` carries `locator` and `excerpt` as strings, with no
byte offsets and no source content hash, so nothing can confirm that an excerpt is a faithful
transcription of its source. Provenance is currently asserted rather than verified, which makes
this the first thing to fix.

**Validation is opt-in, so it is not a guarantee.** `NodeKind::Validator` is a node an author can
forget to wire. Correctness has to be an admission rule on the write path.

## Staging

Ordered so that each stage is independently valuable and each can kill the next.

| Stage | Deliverable | Why it stands alone |
| --- | --- | --- |
| 0 | Verifiable provenance: byte offsets and source content hashes on `Evidence`, immutable source storage, and a checker that re-derives every excerpt from its source. | Makes citations mechanically checkable for the first time. Everything above is unsound without it. |
| 1 | Typed values and admission: remove `state_json`, introduce structural value types with reference types into the memory graph, and make executors return proposals the runtime only commits on an admitted verdict. | Turns a ceremonial type check into a real one and makes correctness non-optional. |
| 2 | The evidence alphabet: retrieval produces a closed, run-pinned set of legal ids and surface forms with a recorded digest. | This, not decoder masking, is the anti-fabrication guarantee. It works against any model API. |
| 3 | The kernel: the rule set above, the three-valued verdict, certificates, and the typed error taxonomy that drives repair routing. | First point at which "verified" can be said and mean something precise. |
| 4 | Search and learned proposers: reuse the existing budget and termination machinery as a search controller, and introduce embeddings for candidate generation and search ordering. | Only worth building once refutation is cheap and total. This is where small-model quality climbs. |
| 5 | Discharging residuals: route obligations to human review, a stronger model, or independent second readings through the existing review queues. | Turns a conditional verdict from a dead end into a workflow. |

## What would falsify this

The ratio of accepted to conditional verdicts on real questions decides whether the thesis holds.
If most answers return conditional with a long tail of unexamined readings, the result is an
elaborate provenance tracker rather than a correctness system.

That ratio is measurable after stages 1 and 2, long before the kernel exists. The experiment
should be designed before the kernel is started: take a real corpus, a real question set, a small
model constrained to a pinned evidence alphabet, and measure how often single-hop extractions
survive span checking.

Two places still require a large model or a domain expert, and they differ in kind:

- **Authoring ontologies and contracts** is a one-time, per-domain, offline cost. Paying a
  frontier model or an expert once per domain to bootstrap a signature that a small model then
  works against indefinitely is a coherent trade, in the same shape as paying a compiler author
  once so that every later program is checked cheaply.
- **Interpretation is per answer and does not amortise.** This is why the residual has to be
  explicit and why the accepted-to-conditional ratio is the metric that matters.

## Cleanups implied regardless of direction

- `Claim::conflicts_with` is dead outside a single test;
- `detect_claim_conflicts` performs a bounded table scan inside an HTTP handler;
- `ContradictionRuleSpec` is unexecutable by construction, so contradiction policy is currently
  declarative only;
- `PortValueType::Json` is the escape hatch that makes the hyperedge check vacuous;
- `Claim` timestamps are strings, and any temporal reasoning needs parsed intervals.

## References

- Zhang, Tay, Yao, Liu. [Quaternion Knowledge Graph Embeddings](https://arxiv.org/abs/1904.10281).
  NeurIPS 2019.
- [Noeon Research](https://noeon.ai/) — an independent team pursuing a related direction:
  knowledge representation defined through actions using category theory, with graph structures
  for deliberative reasoning. Their framing of knowledge as actions is close to the
  precondition/postcondition treatment of plan steps above. Their reliability claims are stated
  aspirations rather than published results; the convergence is evidence that the direction is
  real, not that the problem is solved.
