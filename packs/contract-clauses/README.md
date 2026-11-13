# Contract clauses

A reference domain pack ([ADR 0021](../../docs/adr/0021-the-verifier-protocol-boundary.md),
[ADR 0022](../../docs/adr/0022-signed-versioned-domain-packs.md)) for answers that quote contract
clauses. The failure it is built to catch is an exact quote that is misread: a clause that says one
thing on its own and another once the provision qualifying it, the clause overriding it, or the
definition of one of its words is read with it.

| Path | Holds |
| --- | --- |
| `contracts/contract-clauses.contract` | the relations, the phrasings that state them, how a contract's provisions qualify and define each other, and who decides a reading |
| `obligations/contract-clauses.json` | the policy defaults for the pack's obligations, as an IR contract |
| `validators/references-resolve.json` | a replayable tool: every section a contract refers to is one of its own provisions |
| `cases/references-resolve.json` | the cases that show the validator works |
| `samples/` | two agreements written for this pack; no real party's contract is in it |

## What it declares

**Relations**, each with the phrasings that state it:

| Kind | Relations |
| --- | --- |
| Obligations | `shall`, `pay_within`, `liable_for` |
| Rights | `may`, `may_assign_to`, `may_raise` |
| Termination | `may_terminate`, `terminates_on` |
| Conditions | `requires` |
| Terms | `are`, `has_term`, `renews_for`, `governed_by` |

A negative clause ("is not liable for", "may not assign") never states a relation. The kernel's own
cues stop it, so an answer resting on one is a reading for a reviewer.

**How a contract's provisions bear on each other**, declared under `statement_policy`:

- **Provisions** start at a heading such as `8.` or `Section 8.3.` at the start of a line. A
  provision runs to the next heading that is not nested inside it.
- **References** are `Section`, `Clause`, `Article` or `paragraph` followed by a dotted number,
  such as `Section 8.3`.
- **Qualifiers:**
  - `subject to`, `except as provided in`, `except as set out in`, `save as provided in` and
    `as limited by` make the provision they name qualify the clause they are in;
  - `notwithstanding` and `in priority to` make the clause they are in prevail over the provision
    they name;
  - `provided that`, `unless`, `if`, `only if`, `except where` and `on condition that` make a clause
    conditional.
- **Definitions** are `"Term" means`, `"Term" has the meaning`, `"Term" includes`, and inline
  `(the "Term")`.

**Who decides a reading:** `review_policy` names `review:legal`. A certificate for an answer read
under this contract carries that authority, and only a review by a holder of `review:legal` decides
one of its readings. A project administrator without it is refused.

## What a cited clause comes back as

The kernel looks up what the cited clause depends on, and the proposer reads it with the same
lookup:

- **A qualified clause** ("Subject to Section 8.3, either party may terminate this Agreement"):
  the clause is read with the provision that qualifies it, and the answer is a reading of it read
  that way. The certificate is `conditional` on that reading, and the residual shows the qualifying
  clause. It stays `conditional` until a holder of `review:legal` decides it holds (`accepted`, on
  basis `human_review`) or does not (`rejected`).
- **A clause using a defined term** is read with the definition, and the certificate carries it.
- **An answer to another question** than the typed one asked is `rejected` (`goal_not_responsive`).

Quoted bare, without the document around it, a qualified clause is `rejected` as
`relation_qualified`. It is never `accepted`.

## The policy for its obligations

`obligations/contract-clauses.json` is what a definition using this pack declares under
`enforce`:

| Obligation | Discharged by |
| --- | --- |
| `clause-reading` | `human_review` only, under `review:legal` |
| `answers-the-question` | `mechanical` against a typed query; `human_review` under `review:legal` for a question asked only in words |
| `references-resolve` | `replayable_tool` only: the validator here, which `decides` it |

Run over an agreement, `capsulet_pack::decide` hands the validator the agreement's text and
discharges or fails `references-resolve` on its word. `clause-reading` and, for a question asked
only in words, `answers-the-question` stay residual for a holder of `review:legal`. The kernel
decides `answers-the-question` against a typed query.

Version 1.1.0 says what the validator decides. 1.0.0 did not, so a run could not discharge
`references-resolve` with it.

A model verifier may endorse or dispute a reading and never discharges one. A boundary whose policy
sets `refuse_disputed` stays shut while a model disputes a reading it gates.

## What it does not catch

- **A qualification written as a sentence of its own**, with no declared cue and no reference, such
  as "The Supplier may raise prices once a year. Any increase needs the Customer's written
  consent." Nothing links the second sentence to the first, so the first is read alone. The
  misreading corpus keeps this case as a recorded limitation (`CAP-CORRECTNESS-019`).
- Whether a reading of a fully cited clause is right. That is the reviewer's decision, and the
  certificate says it rests on one.

## Using it

```sh
capsulet pack test packs/contract-clauses
capsulet pack sign packs/contract-clauses --key publisher.key.json
capsulet pack verify packs/contract-clauses --trust trust.json
```

The pack is committed unsigned. Sign it with your own publisher key, which a project's trust store
holds for `contract-clauses`, and install it with `POST /v1/packs`
([domain packs](../../docs/contracts/domain-packs.md)). Store the contract with
`POST /v1/memory/contracts`, and ask with its id so answers are read under it.
