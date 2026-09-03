# IR and Certificate Contract

Status: experimental. This is the reference for the M2 verified-computation IR, its certificates,
and their bundles: what each document promises, how versions change, and what a reader is entitled
to conclude from one.

Related contracts: [IR value types and trust classes](ir-value-types.md), [structural admission
rules](ir-admission-rules.md), [adapter coverage](ir-adapter-coverage.md), [lifecycle and
assurance](lifecycle-and-assurance.md).

## The three documents

| Document | Schema | What it is |
| --- | --- | --- |
| Definition | `capsulet.ir/v1` | A versioned workflow: graph, capabilities, budgets, boundaries, contracts, assurance mode. |
| Certificate | `capsulet.certificate/v1` | What was checked about one run of one definition, sealed under a digest. |
| Bundle | `capsulet.bundle/v1` | A certificate plus every byte of evidence it cites, in one deterministic file. |

Every persisted root carries its schema version as data. A reader that does not recognise the major
refuses the document rather than interpreting it: a document from a later schema is not an empty
document, and treating it as one would turn an unreadable record into a permissive one.

## Canonical bytes

Digests are taken over canonical bytes, never over "some serialization". The encoding is:

- UTF-8, with object keys sorted by Unicode scalar value and no insignificant whitespace;
- list order preserved, because order is meaning in a list;
- integers encoded as integers, other decimals as fixed-point strings;
- no floating point anywhere, so a value that cannot be reproduced bit-for-bit on another machine
  cannot enter a digest;
- text required to be valid UTF-8 in Unicode NFC, so two spellings cannot produce two digests.

This deviates from RFC 8785 in one respect, deliberately: JCS sorts keys by UTF-16 code unit and
this encoding sorts by scalar value, which is what `String` ordering gives and is equally total.

Duplicate object keys, unnormalized text, and floating point are refused at the reader, before a
digest could be computed over an ambiguous document.

`crates/ir/tests/golden/` pins the encoding to checked-in bytes. **Changing those bytes changes every
digest ever stored.** It is a breaking change: bump the schema major and provide a compatibility
reader. Never edit a golden file to match new output.

## What a certificate says

A certificate carries the definition it is about (by digest), the definition and policy versions, the
kernel build that decided it, the mode it ran under, the contracts in play, the verifier records, the
evidence digests, every obligation and what became of it, why any loop stopped, and the verdict.

Three rules govern how it may be read:

1. **The verdict is derived, not asserted.** It is computed from the obligations under the recorded
   mode. A certificate whose recorded verdict does not follow from its own contents cannot be
   sealed.
2. **The mode is part of the statement.** `unverified` under observe means nobody was asked to
   check. `unverified` under enforce means a required check did not happen. Reading one as the other
   is a mistake the format prevents by carrying the mode.
3. **The seal is checkable by anyone.** The digest covers the whole body and travels with it, so an
   edit is detectable by a holder of the document, not only by this installation. Deserialization
   re-checks it, which means a tampered certificate never becomes a certificate value.

An obligation is discharged, assumed, waived, left residual, or failed. There is no absent case.
Assumption and waiver are kept separate because one says nobody checked and the other says a named
authority decided to release anyway.

## What replay proves, and what it does not

`capsulet-replay <bundle.json>` exits 0 when the certificate still justifies its verdict, 1 when it
does not, and 2 when the bundle cannot be read.

Replay **does**:

- re-check the seal;
- re-check that every cited piece of evidence is present and digests to what the certificate says;
- re-decide deterministic obligation families from their pinned inputs;
- recompute the verdict from the obligations under the recorded mode.

Replay **does not** re-run external tools. A scanner, compiler, or test runner is a declared oracle:
its identity, version, and environment digest are pinned and checked, its word is taken, and the
outcome says so explicitly. A replayer running a year later has no guarantee that tool still exists,
and claiming otherwise would be the overstatement this design exists to avoid.

Failures fail closed. Missing or mismatched evidence fails the obligations resting on it, and the
verdict recomputes to `rejected`. An unknown checker claiming determinism is not trusted. An
unreadable schema major supports no verdict at all.

## Versioning and compatibility

| Change | Requires |
| --- | --- |
| Adding an optional field to a document | Minor change; readers ignore what they do not know. |
| Adding a variant to a closed enum (verdict, discharge state, stop reason) | Schema major bump: an old reader would mis-handle the new case. |
| Changing the canonical encoding, key ordering, or digest algorithm | Schema major bump plus a compatibility reader; every stored digest becomes unreproducible otherwise. |
| Adding an admission rule | Recorded in the admission record's `rules_applied`, so an old certificate still says which rules its build applied. |
| Adding an obligation family | Additive. Old certificates keep replaying; the family appears in new ones. |

Stored certificates are append-only and cannot be corrected in place. A mistake is superseded by a
new certificate, which is the cost of a record that survives the wish that it had said something
else.

## Storage

| Table | Holds | Immutability |
| --- | --- | --- |
| `ir_definitions`, `ir_definition_versions` | Definition identity and the exact canonical bytes of each version. | UPDATE and DELETE refused by database rule. |
| `assurance_certificates` | Certificate bytes plus the queryable columns (verdict, mode, digests). | UPDATE and DELETE refused by database rule. |
| `assurance_obligations` | A projection of the obligations, so outstanding work is a query. | UPDATE and DELETE refused by database rule. |
| `assurance_evidence` | Evidence metadata and its object-storage key. | UPDATE and DELETE refused by database rule. |

Evidence bytes live in object storage under `assurance/evidence/<digest>`, so the key cannot point
at bytes that are not what it names, and the metadata store does not become a blob store.

Tenant and project are part of every key rather than a filter applied afterwards: the same
definition id in two projects is two definitions, and a certificate from another project does not
come back at all.
