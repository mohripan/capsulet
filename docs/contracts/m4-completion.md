# M4 Completion Report: Verifier and Domain Ecosystem

Status: **the gate passes; the milestone is not finished.** The M4 gate, three unrelated workloads
on one IR and certificate protocol, passes locally and in CI (see
[the verification run](#verification-run-2026-11-13)). Two M4 issues are still open:

- routing readings to reviewers and independent verifiers (#156);
- the labelled corpus of real questions that ADR 0021's first two falsification criteria need
  (#162).

This report says what the gate shows, what it does not, and what the corpus measured.

M4 opens the correctness plane to checkers Capsulet does not contain: compilers, test runners and
policy engines that run in pinned containers, a person deciding under a named authority, and
models that may endorse or dispute but never discharge. [ADR 0021](../adr/0021-the-verifier-protocol-boundary.md)
fixed the boundary first, and [ADR 0022](../adr/0022-signed-versioned-domain-packs.md) put
everything domain-specific into signed, versioned packs.

## The gate demonstration

The scenario the milestone is judged by is `crates/graph-worker/tests/m4_gate.rs`, run by
`verify --gate m4` on a stack of its own: the API, PostgreSQL and the object store.

1. It signs the three reference packs with a key made for the run, writes a trust store for the
   project into the directory the API reads them from, and installs the packs over the API.
2. It registers three definitions. Each pins its pack and declares the pack's obligations, as an
   ordinary IR definition under `enforce`.
3. It starts five runs with their inputs:

   | Run | Pack | Decided by | Verdict |
   | --- | --- | --- | --- |
   | Which law governs this agreement? | `contract-clauses` | the kernel's claim-reasoning family, a references tool, and a residual reading for `review:legal` | `conditional` |
   | A change that does not compile | `code-changes` | the compiler and the test runner, in a pinned Python image | `rejected`, owned by the proposer |
   | A sound fix | `code-changes` | the same tools, and a residual review for `review:code` | `conditional` |
   | A public bucket | `infrastructure-policy` | OPA over the pack's Rego policies, and the scope scanner | `rejected`, naming the rule |
   | A compliant change | `infrastructure-policy` | the same tools | `accepted` |

4. It drives the runs with this checkout's graph worker. A pack's validators run in containers, and
   nothing on the stack holds a container runtime, so the worker runs on the host, where Docker is,
   against the stack's database.
5. It seals each run's certificate over the API (`POST /v1/ir/runs/{id}/certificate`), twice, and
   checks the second seal is the first. It reads each bundle back from the API and replays it
   offline. Each reproduces its verdict. The kernel re-decides the claim-reasoning family from the
   proposal and snapshot the certificate pins, and every replayable tool runs again in the container
   its record pins and reaches its record.

```text
$ cargo run -p capsulet-xtask --locked -- verify --gate m4
[verify] m4               running
[verify] m4               passed
[verify] passed 1 required gates: m4
```

No workload has an exception anywhere. The kernel decides the clause answer with the rules it
applies to every answer. The runtime runs five runs of three definitions exactly as it runs any
other. The worker's two verifier nodes are generic: one runs whichever pinned pack a node names,
the other the kernel's one deterministic family. The `domain-free` gate holds that at the source.
It reads every pack's vocabulary and fails if the kernel, the runtime or the graph worker names any
of it in code. The vocabulary is 102 terms from three packs: names, tools, authorities, obligation
ids, relations, phrasings and cues.

## What was delivered

| Issue | Deliverable | Where | Claim |
| --- | --- | --- | --- |
| #152 | The verifier protocol's boundary | [ADR 0021](../adr/0021-the-verifier-protocol-boundary.md) | |
| #153 | An answer held to the question it was asked: typed queries | `crates/kernel/src/question.rs` | `CAP-CORRECTNESS-013`, `-014` |
| #154 | Every discharge records what kind of checker made it; a model never discharges | `crates/ir/src/correctness/obligation.rs` | `CAP-ASSURANCE-009` |
| #155 | `capsulet.verifier/v1`: validators in isolated, pinned containers, re-run from a bundle | `crates/verifier` | `CAP-ASSURANCE-010` |
| #157 | A residual for what qualifies a cited clause | `crates/kernel/src/context.rs` | `CAP-CORRECTNESS-019` |
| #158 | Signed, versioned domain packs and their harness | `crates/pack` | `CAP-ASSURANCE-011` |
| #159 | The contract-clauses pack | `packs/contract-clauses` | `CAP-CORRECTNESS-020` |
| #160 | The code-changes pack; `decide` and `replay_tools` | `packs/code-changes`, `crates/pack/src/decide.rs` | `CAP-ASSURANCE-012` |
| #161 | The infrastructure-policy pack; waivers recorded as waivers | `packs/infrastructure-policy`, `crates/pack/src/waiver.rs` | `CAP-ASSURANCE-013` |
| #163 | The gate: runs take inputs, verifier nodes, certificates sealed from the log, the `m4` and `domain-free` gates | `crates/graph-worker/src/agent/verify.rs`, `crates/kernel/src/decision.rs`, `crates/xtask/src/domain.rs` | `CAP-ASSURANCE-014`, `-015` |

## The corpus, against ADR 0021

ADR 0021 names three ways it could be wrong. Two need a corpus M4 does not have yet.

| Criterion | What would make the ADR wrong | Measured |
| --- | --- | --- |
| Splitting responsiveness pays | Reviewers judge question readings no faster than whole answers | **No.** Needs reviewers timed on real questions (#162). |
| The typed query is the right first form | Most real questions do not fit one relation and one slot | **No.** Needs real questions (#162). |
| The protocol is general | A pack cannot express its workload without a kernel or runtime change | **Yes:** three packs did, and the gate is built to fail if one could not. See below for what the platform gained to carry them. |

What M4 can report is the **misreading corpus**: ten answers to questions about one agreement the
project wrote for it, each labelled by the people who wrote the kernel. It is a fixture that bounds
the mechanisms on sentences like these. It is not a measurement of real contracts or real questions.
Its verdicts, by the kernel alone and under the contract-clauses pack alike, are pinned by tests
(`the_verdicts_the_completion_report_states_are_what_the_corpus_measures` in
`crates/kernel/tests/misreading_corpus.rs`, and its twin under the pack in
`crates/pack/tests/contract_clauses.rs`):

| Verdict | Answers | Of which |
| --- | --- | --- |
| `accepted` | 3 | 2 sound; 1 misread, the recorded limitation (`CAP-CORRECTNESS-019`) |
| `conditional` | 1 | 1 misread: a defined term read in its ordinary sense, waiting on a reading |
| `rejected` | 6 | 4 misread, caught by a qualifier or a cue; 2 non-responsive, caught by the query |

**The accepted-to-conditional ratio is 3 to 1.** That is ten cases written to exercise the
mechanisms, so the ratio says nothing about how often real answers will need a reviewer. ADR 0021
expected more answers to be `conditional` than before. This corpus cannot confirm or refute that,
because it was written before the change with answers built to be caught.

The gate's five runs come to one `accepted`, two `conditional` and two `rejected`. They were chosen
to cover each outcome, so they measure nothing either.

### What the protocol needed, to carry three workloads

"No kernel or runtime change" is read here as no change that names a domain. M4 did change the
platform to carry the three packs, and every change is generic:

- the verifier host takes input trees, so a code change can be two revisions (#160);
- a pack's manifest says what each validator `decides` and `reads` (#160, #161);
- an obligation may name a `waiver_authority`, and a waived state records who granted it and the
  waiver it rests on (#161);
- IR runs take inputs, verifier nodes run a pinned pack or the kernel's family, and a run's
  certificate is sealed from its log (#163).

None of these names a pack, and the `domain-free` gate fails if one ever does. Whether a fourth,
unlike workload would need more is the same question again. The protocol is general for these
three, and nothing here proves it for the next one.

## Residual risks

- **Validators do not run on the deployed stack.** A pack node needs a container runtime, which
  no Compose service or Helm workload holds. The gate runs the worker on the host. A worker without
  Docker decides nothing: each obligation stays residual, saying its validator could not start. A
  verifier transport the deployed stack can use, such as Kubernetes jobs under the existing pod
  policies, is the first entry condition below.
- **Reviews and waivers have no endpoint for IR certificates.** A residual reading waits for
  `review:legal`, and a waiver can be applied with `capsulet_pack::waive`, but the API takes
  neither for an IR run's certificate yet. That is #156.
- **The corpus is a fixture.** No claim here rests on real questions. #162 is what would let one.
- **Certificates are sealed on request.** A finished run has a certificate once someone calls
  `POST /v1/ir/runs/{id}/certificate`. Sealing is a pure function of the log, so it cannot drift.
  But nothing seals one automatically yet, and the dashboard lists only what was sealed.
- **The domain check reads text.** It catches a pack's vocabulary in a string or an identifier. It
  cannot catch domain behaviour written without that vocabulary: a threshold, say, tuned to one
  pack's cases. Review still has to.
- **One deterministic family.** The kernel can re-decide only claim reasoning offline. Every other
  checker is an oracle whose word a replay takes, or re-runs in its container.

## Entry conditions for what follows

- `verify --gate m4` and `verify --gate domain-free` pass in CI on every change. `domain-free` is
  in both profiles and `m4` in the full one, and the `Required` job waits for both.
- Before a deployed stack runs packs: a verifier transport the stack can run, holding the same
  isolation the container transport does (no network, read-only root and inputs, dropped
  capabilities, limits), checked by the `verifier-isolation` gate's probe.
- Before M4 closes: #156, so a person with the named authority can decide a reading or grant a
  waiver over the API, audited and project-isolated; and #162, so the first two falsification
  criteria have numbers.

## Verification run 2026-11-13

Locally, on Windows with Docker Desktop, the 21 gates this change touches passed in one run:

- `format`, `lint`, `unit` (957 tests), `ir`, `verifier`, `replay`;
- `claims`, `claims-evidence`, `api-contracts`, `authorization`, `domain-free`, `ci-contract`,
  `release`, `supply-chain-contract`, `security`;
- `verifier-isolation`, `postgres`, `chaos`, `migrations`;
- `m4` and `compose`, each on a stack of its own.

In CI, the full profile runs on a clean checkout of the commit, with `m4` in a job of its own and
`Required` waiting for it. The run that closed #163 is linked from the issue.

Three things the gate found that the unit tests had not:

- **A pack node reported obligations it did not decide.** `decide` returns every obligation a pack
  states, residual where no validator decides it, and the node passed all of them on. In the legal
  run, the references node finished after the kernel's node, and its "no validator decides it"
  overwrote the kernel's discharge of `answers-the-question`. A node's decision now carries only
  what its pack's validators decide. The run's certificate holds the rest residual unless another
  node decides it, which is the layer that knows about every node.
- **A trust store read only a key's first entry.** The gate first trusted its key once per pack.
  Verifying reads the first entry for a key, so the second pack was refused as signed by a key "not
  trusted for" it, though the store said otherwise. An operator writing one entry per pack would
  have hit the same refusal. A trust store that lists a key twice is now refused as malformed,
  saying to list it once with every pack it is trusted for.
- **The stack had no way to install a pack.** The API reads trust stores from
  `CAPSULET_PACK_TRUST_DIR`, and Compose never set it, so every pack was refused for its key. The
  API now mounts a trust directory, read-only. `stack up` and each gate's stack make their own.
