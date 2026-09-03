# ADR 0017: Verified Computation IR v1

Status: Accepted

## Context

[ADR 0014](0014-correctness-first-agent-workflow-product.md) settled that Capsulet is a
correctness-first workflow platform and that one trust-typed IR would eventually represent
deterministic workflows, agent workflows, and automations. It deliberately did not add that IR.

The state it left behind made several claims unfalsifiable. Port types were nominal tags with a
`Json` escape hatch, so two ports agreeing proved nothing. Agent state moved as opaque JSON with no
record of what was lost. Hyperedges named their endpoints but not how values combined, so a join
could not say which input justified which output. Loops lived inside prompts and node
implementations, with no bounds anyone could inspect. The correctness kernel decided exactly one
thing — whether a claim was grounded in its source — and its certificate had no workflow scope, no
policy version, no verifier identity, and no evidence bundle. "Verified" was a word in the
documentation, not a type in the code.

M2 exists to make those claims checkable.

## Decision

### One pure crate owns the representation

`capsulet-ir` holds the IR, its canonical encoding, digests, trust lattice, admission rules, and
assurance decisions. It performs no I/O, spawns nothing, reads no clock, and consults no randomness.
This is enforced by a test over its resolved dependency closure rather than by convention, because a
reviewer cannot see a transitive dependency.

The reason is not tidiness. Everything this crate produces has to be reproducible by someone who
does not have this installation — on another machine, at another time — or the certificate carrying
it means nothing.

### Canonical bytes come before digests

One encoder, one digest algorithm, one spelling (`sha256:<hex>`). Object keys sort by Unicode scalar
value; list order is preserved; there is no floating point anywhere in the model, because a value
that cannot be reproduced bit-for-bit elsewhere cannot enter a digest. Text must be valid UTF-8 in
NFC so two spellings of one string cannot produce two digests.

The deviation from RFC 8785 is deliberate and documented: JCS sorts keys by UTF-16 code unit, and we
sort by scalar value, which is what Rust's `String` ordering already gives and is equally total.

### Values are described structurally, and trust is a type

`ValueSchema` describes shape. Records use width subtyping, unions require exhaustiveness, decimal
scale is part of the type, and every rejection names a path and a rule. The `Json` escape hatch
remains, because real systems carry values nobody has modelled, but structure satisfies opacity and
opacity never satisfies structure, and a value that crosses an opaque hop loses its trust class with
the reason recorded.

`TrustClass` is reachable at `Verified` only from a `VerificationRecord` that justifies it. There is
no cast, setter, or wire field that produces it otherwise; a document claiming more than its record
supports is refused, naming what the record actually justifies. Combining values takes the weakest
relevant trust, and combining values verified under *different* contracts yields `Unverified`,
because neither contract covers the combination.

### Structural admission is mandatory and recorded

Graph validity, typed port compatibility, declared effects and capabilities, bounded loops, required
budgets, provenance, and legal trust edges are checked in every assurance mode. `Observe` means the
domain obligations were not evaluated; it has never meant a malformed or unbounded definition may
run, because no policy makes an unbounded loop safe.

Admission is total — a decision for every definition, tested over generated input with a fixed seed
— and passing produces an `AdmissionRecord` that nothing else can construct. A certificate body
cannot be assembled without one, so a definition that failed admission has no verdict at all, not
even `unverified`.

### The verdict is derived, and the mode travels with it

`AssuranceVerdict` is four-valued at the platform and three-valued at the checker, with a total
mapping in both directions; `unverified` is exactly the case a checker that ran cannot express. A
certificate's verdict is computed from its obligations under its recorded mode, so a certificate
recording a verdict its own contents do not justify cannot be sealed.

Observe concludes `unverified` even when every obligation happened to be discharged, because nothing
was required to be checked. That forced the effective mode into the sealed certificate, which is the
right outcome: `unverified` under observe and `unverified` under enforce are different statements
about a run.

### One decision procedure gates every boundary

`decide_boundary` is pure and total, so the API, the future worker, and the CLI share one rule rather
than three that would eventually disagree — with the permissive disagreement being the one nobody
notices. A missing certificate is `unverified`; verdicts rank `rejected` < `unverified` <
`conditional` < `accepted`, so a rejection cannot satisfy a minimum that absence would fail; and a
protected boundary no policy governs is denied rather than implicitly open.

### Replay is a separate binary, and says what it cannot do

`capsulet-replay` reads a bundle — a certificate plus every byte it cites — and reaches its own
verdict. Its dependency closure excludes databases, HTTP clients, async runtimes, and model
providers, asserted in a test, so "replay needs nothing but the bundle" is a fact about the build.

Replay re-checks the seal and every evidence digest, and re-decides deterministic families from
their pinned inputs. It cannot re-run a scanner or a test runner, so those appear as declared
oracles and the outcome states that the tool was not re-executed rather than letting a reader assume
the kernel confirmed it. Overstating this would be the exact failure the design exists to prevent.

### Storage is append-only in the database

Rules reject UPDATE and DELETE on definitions, versions, evidence, certificates, and obligations. A
certificate whose contents can be edited afterwards proves nothing.

### Adapters describe; they do not execute

`capsulet-ir-adapters` translates today's workflows, agent graphs, and governed-memory records into
the IR, and a test asserts no execution crate depends on it. Every construct that translates with
loss is a row in a generated coverage report, compared against the adapters themselves, so loss must
be declared rather than discovered later by whoever trusted the translation.

## Consequences

- A Capsulet result can carry a certificate someone else can check without trusting Capsulet's
  runtime, its models, or its network.
- Changing the canonical encoding changes every stored digest. It is a schema major bump with a
  compatibility reader, never a fixture edit.
- Definitions and certificates cannot be corrected in place. A mistake is superseded by a new
  version, which is the cost of a record that survives inconvenience.
- Nothing executes from the IR yet. The scheduler and agent runtime are unchanged, and wiring the IR
  into a runtime remains a deliberate M3 decision.
- External verifiers exist in certificates only as declared oracles. The validator SDK, the
  container protocol, and domain packs are M4.
- Interpretability surfaces — inspectors, replay diffing, lineage views — are M5. M2's read APIs
  exist to make certificates exportable and testable, not to be a product surface.

## Alternatives considered

**Extend the existing reasoning certificate instead of adding a platform certificate.** Rejected:
the reasoning certificate is about one claim against one snapshot. Widening it would have made every
claim decision carry workflow scope it does not have, and would have coupled the kernel's rules to
the platform's policy model. Instead the reasoning rules became the first obligation family, with
their behaviour and tests unchanged.

**Serialize with an existing canonical JSON crate.** Rejected for the parser side: a
general-purpose JSON parser silently collapses duplicate keys, which is exactly the ambiguity this
crate refuses. The reader is written here so failures stay typed.

**Store evidence bytes in the database.** Rejected: a metadata store that also holds every scanner
log stops being either. Bytes go to object storage under their digest; the database holds digests
and metadata.

**Let replay re-run external verifiers.** Rejected as impossible to promise. A replayer a year later
has no guarantee the scanner version, its rules, or its environment still exist. Pinning identity
and saying plainly what was not re-run is the honest version.
