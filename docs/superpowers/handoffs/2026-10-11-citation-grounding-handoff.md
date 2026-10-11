# Handover: citation grounding, Tasks 6–9

Written 2026-10-11 for the agent taking over
[the citation grounding plan](../plans/2026-10-04-citation-grounding.md). Read the plan first, then
this. The plan says *what* to build; this says how the repository expects it built, what has
already gone wrong once, and which choices are not yours to make.

Background, in reading order: the plan's *Goal* and *Evidence* sections;
[docs/design/correctness-architecture.md](../../design/correctness-architecture.md);
[ADR 0012](../../adr/0012-correctness-kernel-and-proposer-checker-split.md);
[the robustness plan](../plans/2026-09-06-correctness-kernel-robustness.md), for the conventions this
work follows.

## Where things stand

**Done — Tasks 0–5**, one commit each on `main` (`a9d5969` … `0e9e299`, plus this handover's commit):

| Task | Commit | What it changed |
| --- | --- | --- |
| Plan | `a9d5969` | The plan. |
| 0 | `a3a2cbc` | Tripwires in `crates/kernel/tests/known_gaps.rs`; claims and docs restated to what was true. |
| 1 | `faf7e4f` | `CLAIM_REASONING_RULES` replaces the crate version; replay reports `FamilyRulesDiffer`. |
| 2 | `29add1d` | A goal is reached only by `Holds(goal)`, compared field by field; `canonical()` escapes `\|` and `\`. |
| 3 | `59c3cb3` | The authority floor travels with the snapshot; a proposal can raise it, never lower it. |
| 4 | `0e27333` | `text.rs`: whole-word matching (UAX #29), full case folding, ignorables, typography. |
| 5a | `99231d8` | `phrasing.rs`: `Says` only in a declared phrasing, else `Mentions`; `Trust` refuses `Mentions`; the proposer routes by the kernel's check; the dead reading path is fixed. |
| 5b | `0e9e299` | `stated_by:` and `statement_policy: cues:` in the contract DSL; `contract_id` on `/v1/reasoning/ask`. |

**Open — Tasks 6–9.** Tasks 6–8 were revised on 2026-10-11 (see the notes under each in the plan).
In short, typed values and approved names come from the contract and the snapshot, so the proposal's
shape does not change, and Task 6 is no longer a prerequisite for 7 or 8.

**Tripwires left in `known_gaps.rs`:**

| Test | Closed by | Claim |
| --- | --- | --- |
| `a_number_written_another_way_is_not_found` | Task 7 | `CAP-CORRECTNESS-004` |
| `a_date_written_another_way_is_not_found` | Task 7 | `CAP-CORRECTNESS-004` |
| `another_name_for_the_entity_is_not_found` | Task 8 | `CAP-CORRECTNESS-004` |
| `a_doubt_the_cue_list_does_not_know_still_states_the_relation` | never — a recorded limitation | `CAP-CORRECTNESS-012` |

The first three assert on the term step (`term_not_in_span`), not on the verdict, so each goes red
the moment its term starts being found. See *Traps* for why.

**State at handover:** `verify --profile fast` passes all nine gates; kernel, proposer, core and API
tests are green; the product claim registry is valid (55 claims). Nothing from this plan has been
pushed.

## Map of the code

| Where | What it owns |
| --- | --- |
| `crates/kernel/src/lib.rs` | `check`, `derive_cite`, `derive_trust`, `reaches_goal`, the depth and span bounds. |
| `crates/kernel/src/text.rs` | The text relation: `fold`, `contains_term`, `contains_folded`, word boundaries. |
| `crates/kernel/src/phrasing.rs` | `Phrasebook`, `DEFAULT_CUES`, `statement_in`, sentence splitting. |
| `crates/core/src/domain/phrasing.rs` | The single definition of what a phrasing may look like. |
| `crates/core/src/domain/memory_contract.rs` | Contract DSL: `stated_by`, `statement_policy`. |
| `crates/kernel/src/{snapshot.rs,snapshot_document.rs}` | What a decision reads, and its replayable wire form. |
| `crates/kernel/src/family.rs` | `CLAIM_REASONING_RULES` — currently `"5"`. |
| `crates/kernel/src/replay.rs` | Re-deciding a pinned claim-reasoning record. |
| `crates/proposer/src/ollama.rs` | `build_proposal` routing, `declared_predicates`, the prompt and schema. |
| `crates/api/src/reasoning.rs` | `AUTHORITY_FLOOR`, `contract_phrasebook`, the ask flow. |
| `crates/kernel/tests/{grounding,known_gaps,replay}.rs`, `crates/kernel/src/tests.rs` | Kernel tests. |
| `crates/core/tests/memory_contract_dsl.rs` | DSL tests; also compiles every example in `docs/memory-contracts/`. |

## Rules that are not negotiable

1. **The kernel stays a decision procedure.** No learned component, no network, no clock, no
   randomness, and no similarity score anywhere a decision reads. Equivalence comes from reviewed
   data (contract phrasings, confirmed resolutions) or from a closed, versioned grammar.
2. **Provenance stays byte-exact.** Excerpt re-derivation, content hashes, evidence digests and seals
   are integrity checks. Do not soften them.
3. **Ambiguity is a residual, never a choice.** When a grammar reads a form two ways, record both
   readings as a residual and let the verdict be `conditional`.
4. **Every change to what `check` decides bumps `CLAIM_REASONING_RULES` in the same commit.** That
   includes cue lists, grammars, and new snapshot data a decision reads. If a dependency bump moves
   the Unicode tables, `the_unicode_tables_a_citation_is_judged_by_are_pinned` fails; bump both.
5. **Snapshot document fields are additive**, with `#[serde(default, skip_serializing_if = …)]`, so a
   document written before the change still parses, rebuilds, and encodes byte for byte as it did.
6. **The tripwire workflow.** A known gap is a test in `known_gaps.rs` asserting today's wrong
   behaviour, plus a `limitation` claim citing it. When a task fixes it, the test goes red; move it,
   inverted, to the file where the behaviour belongs, and turn the claim into a `capability` with
   verification assertions in the same commit.
7. **Docs say what is true at every commit.** Keep these in step with behaviour: the kernel bullet and
   the numbered limitations list (the count is in the prose) in `ARCHITECTURE.md`; the correctness
   paragraph in `docs/architecture.md`; `docs/design/correctness-architecture.md`; the verdict
   paragraph in `README.md`; the reasoning section of `docs/api.md`.
8. **Plan hygiene.** Tick a task's boxes in the commit that lands it. Where the plan was wrong, record
   the correction under the task ("What the task found") rather than silently doing something else.

## The claim registry, mechanically

- Edit `docs/contracts/product-claims.json`. It round-trips exactly through Python's
  `json.dumps(d, indent=2, ensure_ascii=False) + "\n"` with LF line endings, so scripted edits are
  safe.
- Every `test` evidence entry needs its `(path, selector)` listed under the matching command in
  `test_manifest` — for kernel tests that command is exactly `cargo test -p capsulet-kernel --locked`.
- An `implemented` `capability` needs `verification` assertions whose selectors are among its test
  evidence. A `limitation` may cite `source` evidence; prefer a tripwire test.
- A claim listing `ARCHITECTURE.md` or `docs/architecture.md` as a public surface needs its ID in the
  `capsulet-claims` comment on line 1 of that file.
- Regenerate: `powershell -NoProfile -File scripts/render-product-claims.ps1 -OutputPath docs/contracts/product-claims.md`.
- Validate: `powershell -NoProfile -File scripts/check-product-claims.ps1`.

## Verifying

Per task:

```text
cargo fmt --all
cargo clippy -p capsulet-kernel -p capsulet-proposer -p capsulet-core -p capsulet-api --all-targets --all-features --locked -- -D warnings
cargo test -p capsulet-kernel -p capsulet-proposer -p capsulet-core --locked
cargo test -p capsulet-api --lib --locked        # when the API changed
powershell -NoProfile -File scripts/check-product-claims.ps1
```

Clippy runs with `pedantic` as warnings and the gate denies warnings. If an API request or response
type changes, run `cargo run -p capsulet-api --bin export-openapi` and commit `crates/api/openapi.json`.

Before handing back: `cargo run -p capsulet-xtask --locked -- verify --profile fast` — nine gates
at handover, including `verifier` and `authorization`.

The full profile needs Docker. On this machine Docker Desktop sometimes crash-loops on orphaned
socket files: stop every `Docker Desktop` and `com.docker.*` process, rename (do not delete)
`%LOCALAPPDATA%\Docker\run` and `%LOCALAPPDATA%\docker-secrets-engine`, create empty ones, and start
it again — both in the same pass.

Environment notes (Windows): the shell may be Git Bash or PowerShell. Python heredocs containing
backslashes or backticks broke twice in Git Bash — write edit scripts to a file instead. Files are
LF; `core.autocrlf` warnings on commit are noise.

## Traps already found

- **A tripwire on the verdict may never go red.** With no phrasing declared, a case stays `rejected`
  for a different reason after the gap it tracks is fixed. Assert on the step the gap is about
  (`term_not_in_span`, the `cite` discharge, a specific error code).
- **Phrasing matching is instantiate-and-search** (`Phrasing::instantiate` then a folded,
  word-aligned search). That cannot match a typed slot whose text differs from the term — "$1.2
  million" for `$1,200,000`. Task 7 needs a slot-aware matcher: find the literal pieces word-aligned
  within one sentence, then test the regions between them as the slots.
- **`Cite` has never written a residual.** Only `Interpret` has. Task 7's ambiguity residuals are new
  behaviour; fill `from`, `to`, `rationale` and `evidence_ids` as consistently as `Interpret` does.
- **Segment sentences on the original text, then fold each one.** On folded text, Unicode's rule that
  a period followed by a lowercase letter does not end a sentence merges sentences. A single line
  break is read as a space first, because extracted text is hard-wrapped.
- **The apostrophe tailoring stays.** UAX #29 keeps `acme's`, `l'accord` and `can't` whole; an
  apostrophe is a boundary here so possessives and elision do not hide names.
- **Proposer tests must run what they build through `check`.** A shape-only test is how every reading
  the proposer built came to be rejected without anyone noticing.
- **`Entity.aliases` and `CanonicalEntity.aliases` are not reviewed.** They come straight from API
  requests. Only an `EntityResolution` with status `Confirmed` is a review (Task 8).
- **Exactness.** `Quantity` refuses rather than rounds; a scale word that would leave a fractional
  unit at the parsed scale must be refused, not approximated.
- **`CAP-CORRECTNESS-012` is permanent.** The Task 9 invariant must name it as a recorded exception,
  not drop the case.

## Decisions that belong to the owner

Ask before choosing; the plan records a recommendation for each.

1. **Task 6 — the digest.** Always serialising `schema_version` changes every proposal's replay
   digest. It identifies a proposal on a kernel certificate and no decision reads it. *Recommended:*
   always serialise, as every other persisted root does.
2. **Task 7 — `$` and other shared symbols.** *Recommended:* ambiguous, hence a residual, unless the
   contract declares a default (for example `statement_policy: default_currency: USD`).
3. **Task 7 — numeric dates.** *Recommended:* a contract-level `date_order: day_first | month_first`;
   without it, `05/01/2026` is a residual naming both readings.
4. **Task 8 — which names count.** *Recommended:* only names joined by confirmed resolutions; the
   alias fields stay out unless they gain a review status.
5. **Tracking.** The repository now tracks plans as GitHub issues (see #104). Whether to open issues
   for Tasks 6–9, and whether commits carry `(refs #NN)`, is the owner's call.
6. **`/v1/reasoning/ask`'s authority floor** stays `low` until the owner says otherwise.

## Working agreements

- One focused commit per task on `main`. Short conventional subject (`fix:`, `feat:`, `docs:`,
  `test:`), no body unless something needs explaining, **no `Co-Authored-By` trailer**. Append
  `(refs #NN)` only if an issue exists for the task.
- **Do not push** unless the owner asks.
- Stop and report after each task: what changed, what the task found, the gates that ran, anything
  that needs a decision. Do not start the next task while a decision above is open for it.

## Done means

- Tasks 6–9 are ticked, each with its findings recorded in the plan.
- `known_gaps.rs` holds only recorded limitations, or is gone.
- `CAP-CORRECTNESS-004` and `-008` are closed or restated with evidence; `-012` is still honest.
- `verify --profile fast` passes at every commit.
