# IR Structural Admission Rules

Status: experimental. These are the checks `capsulet_ir::admit` applies to every definition, in
every assurance mode.

## Admission is the floor, not a mode

`Observe`, `Verify`, and `Enforce` differ in what happens to *domain* obligations and protected
boundaries. None of them turns these rules off. Observe means nobody checked the domain properties;
it has never meant that a malformed graph, an undeclared effect, an unbounded loop, or an ungranted
capability may run. Those are not domain questions, and no policy makes an unbounded loop safe.

Two properties hold alongside the rules:

- **Admission is total.** It returns a decision for every definition, never panics, and never runs
  forever. A checker that can be crashed by hostile input is a checker that can be made to skip a
  check. `crates/ir/tests/admission.rs` exercises this over generated definitions with a fixed seed.
- **Admission is recorded.** Passing produces an `AdmissionRecord`, which nothing else can
  construct, and a certificate body cannot be assembled without one. So "this definition was
  structurally admitted" is provable after the fact rather than assumed — and a definition that
  failed admission has no verdict at all, not even `unverified`.

Rules are applied in a fixed order, so the first refusal is deterministic and can be quoted back to
whoever has to fix it.

## The rules

Each refusal carries a stable code and the subsystem that owns the repair, using the same
`RepairOwner` vocabulary the correctness kernel routes failures by. This table is generated from
`AdmissionCode` and checked against it by `crates/ir/tests/admission.rs`.

<!-- generated: admission rules -->

| Code | Owner | Refuses |
| --- | --- | --- |
| `graph_invalid` | runtime | The graph references nodes, ports, or identifiers that do not exist, or declares the same identifier twice. |
| `port_incompatible` | runtime | An edge delivers a value that does not satisfy the schema of the port it feeds. |
| `effect_undeclared` | runtime | A node performs an effect it did not declare, an effect its kind may not perform, or a protected boundary is declared over an effect that does not exist. |
| `capability_ungranted` | policy | A node reaches for a capability the definition never granted, holds one its kind may not hold, or a nested scope widens what it was given. |
| `repetition_unbounded` | runtime | A loop lacks a finite bound, or a cycle exists outside any loop region. |
| `budget_invalid` | runtime | A budget is empty where work must happen, or a scope claims more than the scope that contains it. |
| `provenance_missing` | runtime | A combination produces a value that cannot be traced back to the sources that contributed to it. |
| `trust_edge_illegal` | verifier | An edge delivers less assurance than a port requires, or claims a contract from a node that is not a verifier. |
| `schema_invalid` | runtime | A schema version, digest, or encoding does not parse or does not match what the document claims. |
| `boundary_invalid` | runtime | A protected boundary names a node or effect the definition does not declare. |
| `contract_invalid` | runtime | A contract is declared twice, or its obligations are not distinct. |
