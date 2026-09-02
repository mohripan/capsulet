# IR Value Types and Trust Classes

Status: experimental. This document describes the M2 verified-computation IR value model implemented
in `crates/ir`. It does not describe how the current runtime executes graphs; execution still uses
the models in `crates/core`, and the adapters between them are recorded separately.

## Structural, not nominal

A value in the IR is described by a structure, not by a tag from a closed list. The distinction
matters because a nominal tag makes every compatibility question unanswerable in the same way: two
ports agreeing on the name `Json` proves nothing about whether one can feed the other, and a
mismatch can only ever be reported as "types differ".

`ValueSchema` therefore carries shape: bounded integers, fixed-point decimals at an exact scale,
length-bounded text and lists, records with required and optional fields, discriminated unions,
enumerations, digest-addressed bytes, and artifact references.

Compatibility is decided by `ValueSchema::check_satisfies`, and every rejection names a path and a
rule so the author knows which field to fix.

| Construct | Rule |
| --- | --- |
| Record | Width subtyping. A producer may carry extra fields. A producer field that is optional does not satisfy a required requirement. |
| Union | Exhaustiveness. Every variant a producer can emit must be handled by the requirement, and the discriminant must match. |
| Integer, text, list | Bounds must fit inside the required bounds. |
| Decimal | The scale is part of the type. Scale 2 does not satisfy scale 3. |
| Artifact reference | An unconstrained requirement accepts any contract; a constrained one accepts only its own. |
| Opaque (`Json`) | Structure satisfies opacity. Opacity never satisfies structure. |

## Numbers

There is no floating point anywhere in the model. Floats have no canonical encoding, so a digest
taken over one is not reproducible by the person checking the certificate, which defeats the point.
Iterations, milliseconds, tokens, cost units, and effect counts are integers. Everything else that
needs a fraction is a fixed-point decimal carried as text, where trailing zeros are significant
because a scale is a statement about precision.

Values that are genuinely floating point, such as embedding vectors, live in object storage and the
graph moves their digest.

## Opacity is recorded, not hidden

`ValueSchema::Json` exists because real systems carry values nobody has modelled yet. It is legal to
produce one and legal to consume one where the consumer declares that it accepts opacity. It is
never legal to pass one off as structure.

Every opaque schema records why it is opaque, `carries_opacity` reports opacity nested anywhere
inside a value, and a value that passes through an opaque hop loses its trust class with the reason
recorded (`TrustClass::after_opaque_hop`). A certificate can therefore say where structure was lost
instead of leaving a reader to infer it.

## Port tag mapping

The current runtime's `PortValueType` tags map onto structural schemas through
`capsulet_ir::value::aliases::for_port_value_type`. That function is the single definition of this
mapping; the adapters translate through it, and this table is checked against it by
`crates/ir/tests/value_compatibility.rs`.

An unknown tag returns `None` rather than defaulting to opacity, so a tag added to the runtime fails
loudly here instead of quietly losing its structure.

| Port tag | Structural schema | Structure retained |
| --- | --- | --- |
| `user_query` | length-bounded text | yes |
| `conversation_context` | list of text | yes |
| `normalized_query` | length-bounded text | yes |
| `embedding_vector` | digest-addressed bytes | yes |
| `retrieved_documents` | list of `{source_id, content_digest, span_start?, span_end?}` | yes |
| `ranked_documents` | list of `{source_id, content_digest, span_start?, span_end?}` | yes |
| `prompt` | length-bounded text | yes |
| `model_response` | length-bounded text | yes |
| `validation_result` | `{passed, detail?}` | yes |
| `final_answer` | length-bounded text | yes |
| `json` | opaque | no — recorded as provenance loss |

## Trust classes

Trust is a type, not a label. `TrustClass` has three cases: `Unverified`, `Conditional`, and
`Verified`, and the strengthened cases carry the `VerificationRecord` that justifies them.

| Record | Strongest class justified |
| --- | --- |
| `accepted`, no residuals, complete provenance | `Verified` |
| `accepted` with residuals or incomplete provenance | `Conditional` |
| `conditional` | `Conditional` |
| `rejected` or `unverified` | `Unverified` |

Three rules hold everywhere:

1. **Strengthening requires a record.** There is no cast, setter, or wire field that produces
   `Verified` on its own. `RawTrustClass` is the wire shape and it is plain data; it becomes a
   `TrustClass` only by passing the same admission every other path uses. A document that claims
   `verified` while carrying a `conditional` record is refused, naming what the record justifies.
2. **Weakening is always allowed.** Claiming less than a record justifies is conservative, not
   unsound, so it is accepted as written.
3. **Combination takes the weakest relevant trust.** Two values verified under the same contract
   yield the weaker. Two values verified under *different* contracts yield `Unverified`, because
   neither contract covers the combination; a derivation that deserves better needs a verifier of
   its own.
