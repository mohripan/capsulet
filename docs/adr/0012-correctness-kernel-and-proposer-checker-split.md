# ADR 0012: Correctness Kernel and the Proposer/Checker Split

## Status

Proposed

## Context

Capsulet's objective is to make a small model sufficient for complex work by making correctness a
property of the system rather than of the model. That requires deciding what the platform is able
to guarantee.

Five candidate architectures were explored and adversarially reviewed: proof-carrying answers,
verifier-guided search, constrained decoding, neurosymbolic entailment, and per-step contracts.
Every one of them placed a language model in the verifier at the same point — deciding whether a
cited passage supports a proposition. That step is not mechanically decidable over natural
language. A design that hides it inside a heuristic either smuggles an unreliable judge into the
trusted base or silently degrades to substring matching, which certifies copying rather than
support.

Separately, the current runtime does not enforce the typing it appears to. `GraphDefinition::new`
compares `PortValueType` discriminants across hyperedges while node output flows between nodes as
an opaque `state_json` string that nothing inspects, and `NodeKind::Validator` is a node an author
can omit. `Evidence` stores a locator and an excerpt as strings with no byte offsets or source
content hash, so a citation cannot be re-derived from its source.

## Decision

Correctness is enforced by a **kernel**: a deterministic decision procedure with no learned
component, no network access, and no I/O beyond reading the store. Nodes return proposals rather
than values, and the runtime commits to run state or memory only on an admitted verdict.

The kernel's verdict is three-valued. **Accepted** means every step was discharged mechanically.
**Conditional** means the result is sound given a set of named interpretation obligations, each
pinned to a specific span. **Rejected** names the failed premise and the subsystem that owns the
repair.

Interpretation is represented explicitly as a kernel rule that discharges nothing and records a
residual obligation. The platform reports what it verified and what it assumed, per answer,
instead of implying that it verified meaning.

Learned components — including knowledge graph embeddings such as QuatE — are confined to the
proposer side: retrieval candidate generation, search ordering, and gap detection feeding the
review queues. An embedding score never appears in a certificate, and learned scores must not be
written into `Claim::confidence`, because contract policy gates admission on that field.

## Consequences

- `AgentNodeOutcome::state_json` is removed in favour of typed values with reference types into
  the memory graph; `PortValueType::Json` is deleted, since it is what makes the hyperedge check
  vacuous.
- `Evidence` gains byte offsets and a source content hash, and sources are stored immutably.
  Without this, provenance is asserted rather than verified and every layer above it is unsound.
- `AgentRuntimeError::NodeExecutor { message: String }` is replaced by a typed error taxonomy, so
  a rejection routes a repair instead of triggering a re-prompt. Several error classes become
  repairable with no model call.
- Validation stops being a graph node and becomes an admission rule on the write path.
- Introducing embeddings adds a training and evaluation subsystem the project does not have, and
  it depends on a populated graph, so it is sequenced late.
- The accepted-to-conditional ratio on real questions becomes the metric that decides whether the
  thesis holds. If most answers are conditional with many unexamined readings, the result is a
  provenance tracker rather than a correctness system.
- Authoring ontologies and contracts remains a frontier-model or domain-expert task. This is
  accepted as a one-time, per-domain, offline cost.

Full reasoning, staging, and verified references to the current code are in
[docs/design/correctness-architecture.md](../design/correctness-architecture.md).
