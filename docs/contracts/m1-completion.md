# M1 Completion Report — Engineering Integrity

Status: **complete**. A fresh clone of `0d87265` passed `cargo run -p capsulet-xtask --locked --
verify --profile full` with only the documented prerequisites installed. The run passed all 27
gates and 1587 tests in 24 min 3 s, changed nothing in the checkout, and left nothing behind. [The record](clean-checkout-rehearsal.json) holds the evidence. On `main`, CI's `Required`
check holds every commit to the same gates: [run 43228636600](https://github.com/mohripan/capsulet/actions/runs/43228636600).

M1 makes "it passed" mean something you can check. One command verifies a clean checkout; CI runs
the same gates and cannot report green when one of them did not run. Every image, chart and crate
agrees with one release identity, and every published image is signed, attested and scanned by the
digest that was tested.

What M1 does not do: it does not make Capsulet alpha-ready. The public alpha needs M0 through M6
together, and the verifier ecosystem (M4), the interpretability product (M5) and self-hosting
maturity (M6) are not complete; see [the platform design](../superpowers/specs/2026-08-30-correctness-first-agent-workflow-platform-design.md#public-alpha-release-gate).

## The rehearsal

```powershell
git clone https://github.com/mohripan/capsulet.git capsulet-rehearsal
cd capsulet-rehearsal
git checkout 0d8726544054cfeb16c58ad5ea197d2ac16b4251
cargo run -p capsulet-xtask --locked -- verify --profile full
cargo run -p capsulet-xtask --locked -- verify record .capsulet/verify/<run> --output clean-checkout-rehearsal.json
```

| | |
| --- | --- |
| Revision | `0d8726544054cfeb16c58ad5ea197d2ac16b4251` |
| Host | Windows 11, x86_64 (`windows-x86_64`) |
| Tools | Cargo 1.96.0, Docker 29.6.2, Helm 4.1.4, Kind 0.31.0, kubectl 1.36.3, npm 11.19.0, Python 3.14.7, PowerShell 5.1.26100.9711, Git 2.55.0, cargo-audit 0.22.2, cargo-deny 0.20.2 |
| Duration | 24 min 3 s |
| Gates | 27 passed of 27 |
| Tests | 1587 passed, 6 ignored, 0 failed, in 145 test binaries |
| Images | 12 image IDs: 6 components (`api`, `dashboard`, `evaluator`, `graph-worker`, `scheduler`, `worker`), each built twice, by `containers` and by `kind` |
| Chart render | `sha256:99895f5e497548751f89faba2f93e34619648ae5a5bd35194a101c79d0c16220` (Secret values and `checksum/` annotations masked) |
| Verification summary | `sha256:9c8d1f7c7908afb625f086e3afc24af88ea0229f9d26ec7a84a6ddcd9c9e115b` |
| Cleanup | nothing left behind; checked Docker, Kind and the credential directories |
| Worktree | clean at the start, unchanged at the end |

The clone held no build products, local databases, generated secrets or tool caches from a
development checkout. It sat outside any other checkout, so no `.cargo` or formatter configuration
could be inherited. The machine's own caches were warm: Docker's build cache, Cargo's registry and
npm's downloads. That is why the run took under half an hour, and none of it came from a
checkout. It ran on Windows because that is where `verify --profile full` is hardest; Linux runs
every gate in CI on every commit, and did on this one.

The rehearsal found what the checks it closes on were built to find:

- **A fresh clone could not pass.** The `sdk`, `dashboard`, `claims-evidence` and `gate-regressions`
  gates used the dashboard's packages without installing them. CI had always run `npm ci` before
  the verifier, so no CI job could see this. Each of those gates now installs the packages itself,
  and CI no longer prepares the checkout.
- **The Kind gate looked for an image under the wrong tag.** Naming every image for its run changed
  the tag the image builder used, but not the one the Kind gate expected. CI's Kind job caught it on
  the first commit, and both now take the tag from one function.
- **The `sdk` gate rewrote tracked files.** Five Python 3.12 bytecode files had been committed long
  ago. CI's Python rewrote them in every run, so the new worktree check failed `Required`. On
  Python 3.14 the cache files have other names, which `.gitignore` already covered, so no local run
  could have shown this. The files are no longer tracked.
- **Two builds of one commit are not the same bytes.** The `containers` and `kind` gates each built
  every image from this commit, with the same pinned bases and the same labels, and each pair has
  two image IDs. An image's configuration records when it was built, and the dashboard's build
  generates its own id. What ships is still the image that was tested: `release.yml` builds each
  image once, then scans, attests, signs and verifies that one digest. But a rebuild cannot
  reproduce it, so nobody can check a published image by building it again.
- **A network failure stops the gate that needed the network.** An earlier attempt stopped at
  `security` when `cargo-audit` could not reach GitHub to fetch the advisory database. The gate
  reported a failure, not a skip, and the run that followed is the one recorded here.
- **During #102:**
  - `helm uninstall --wait` returned while the release's pods were still terminating, so the
    leftover check now waits for them to go.
  - The verifier's own tests had left deliberately failing runs where CI collects summaries.
    `Required` refused them, which is what it is for, and the tests now write their runs elsewhere.

## Criteria

Each row is an item of the plan's
[M1 release gate](../superpowers/plans/2026-08-31-engineering-integrity.md#m1-release-gate) (`G`)
or [exit criteria](../superpowers/plans/2026-08-31-engineering-integrity.md#m1-exit-criteria) (`E`),
word for word, with the gates that prove it. Every gate named here passed in
[the rehearsal](clean-checkout-rehearsal.json). `cargo run -p capsulet-xtask --locked -- completion
check` holds this table to the plan, to the gate catalog and to the record.

| ID | Criterion | Gates | Evidence |
| --- | --- | --- | --- |
| G1 | repository cleanliness and generated-contract checks | `api-contracts`, `authorization`, `claims`, `claims-evidence`, `verifier` | [rehearsal](clean-checkout-rehearsal.json): clean at the start, unchanged at the end; generated OpenAPI, policy, claims and test manifest must match their sources |
| G2 | Rust format, Clippy, unit, API, integration, and documentation tests | `format`, `lint`, `unit`, `ir`, `replay`, `api-contracts`, `postgres` | [rehearsal](clean-checkout-rehearsal.json); `unit` runs the workspace's doctests |
| G3 | mandatory PostgreSQL and supported migration-path tests | `postgres`, `migrations`, `chaos` | [rehearsal](clean-checkout-rehearsal.json); each fails when a database test is skipped, ignored or filtered |
| G4 | public-claim, lifecycle, OpenAPI, SDK, and authorization contract tests | `claims`, `claims-evidence`, `api-contracts`, `sdk`, `authorization` | [rehearsal](clean-checkout-rehearsal.json) |
| G5 | Python SDK tests and dependency audit | `sdk`, `security` | [rehearsal](clean-checkout-rehearsal.json); `security` runs `pip-audit` 2.10.1 |
| G6 | dashboard lint, typecheck, unit, build, dependency audit, and Playwright tests | `dashboard`, `sdk`, `security`, `dashboard-e2e` | [rehearsal](clean-checkout-rehearsal.json); `sdk` runs the dashboard's tests and `tsc`, `security` runs `npm audit` |
| G7 | Rust, JavaScript, Python, license, vulnerability, and secret checks | `security`, `secrets`, `dependency-images`, `gate-regressions` | [rehearsal](clean-checkout-rehearsal.json); `gate-regressions` shows the checks failing on the defects they exist to catch |
| G8 | deterministic container build, image scan, and SBOM/provenance contract checks | `containers`, `supply-chain-contract`, `dependency-images` | [rehearsal](clean-checkout-rehearsal.json): every image ID. Deterministic in its inputs and identity: pinned bases, and the plan and labels `release.toml` defines. Not yet byte-for-byte, so two builds differ ([below](#remaining-risks)). Capsulet's own images are scanned, attested and signed by the digest built once in `release.yml` ([#101](https://github.com/mohripan/capsulet/issues/101)) |
| G9 | Compose configuration/build/smoke tests | `compose`, `dashboard-e2e` | [rehearsal](clean-checkout-rehearsal.json) |
| G10 | Helm lint/schema/render checks and a real Kind install/smoke/uninstall test | `helm`, `kind` | [rehearsal](clean-checkout-rehearsal.json): the chart's render digest; [#102](https://github.com/mohripan/capsulet/issues/102) |
| G11 | release-version, image-name, chart, and artifact-identity checks | `release`, `helm`, `containers` | [rehearsal](clean-checkout-rehearsal.json); [#100](https://github.com/mohripan/capsulet/issues/100) |
| G12 | cleanup and final clean-worktree verification | `verifier` | [rehearsal](clean-checkout-rehearsal.json): nothing left behind, worktree unchanged; every run checks both after its last gate |
| E1 | the original M0 exit criteria pass with route declarations and real Rust wire types as the HTTP authority, discovered public surfaces, and evidence that executes the claimed behavior | `api-contracts`, `claims`, `claims-evidence` | [rehearsal](clean-checkout-rehearsal.json) |
| E2 | a clean checkout passes the documented full verification command | `verifier`, `format`, `lint`, `unit`, `ir`, `replay`, `api-contracts`, `authorization`, `claims`, `claims-evidence`, `sdk`, `dashboard`, `postgres`, `chaos`, `migrations`, `security`, `gate-regressions`, `compose`, `dashboard-e2e`, `release`, `supply-chain-contract`, `ci-contract`, `secrets`, `dependency-images`, `containers`, `helm`, `kind` | [rehearsal](clean-checkout-rehearsal.json): the whole full profile, from a fresh clone |
| E3 | a required check cannot report success when PostgreSQL, migration, authorization, security, Compose, Helm, or Kind coverage was skipped | `verifier`, `ci-contract` | [rehearsal](clean-checkout-rehearsal.json); `CAP-VERIFY-001`: `required_summary.rs` removes each coverage area's evidence and `Required` refuses |
| E4 | every public route is documented and has generated, executed authorization coverage | `api-contracts`, `authorization` | [rehearsal](clean-checkout-rehearsal.json); each operation is called as anonymous, every role, a service account and a member of another project |
| E5 | project-scoped reads and writes enforce tenant/project ownership atomically and wrong-project behavior is tested | `postgres`, `authorization`, `dashboard-e2e`, `kind` | [rehearsal](clean-checkout-rehearsal.json); ownership is enforced in the SQL that reads or writes ([#97](https://github.com/mohripan/capsulet/issues/97)) |
| E6 | fixtures, seeds, E2E identities, and migration inputs are deterministic and isolated | `compose`, `dashboard-e2e`, `migrations`, `postgres` | [rehearsal](clean-checkout-rehearsal.json); stacks are seeded twice and must match |
| E7 | workspace lint, dependency, vulnerability, license, and secret gates are green or covered by a visible, owned, unexpired exception | `lint`, `security`, `secrets`, `dependency-images`, `gate-regressions` | [rehearsal](clean-checkout-rehearsal.json); the exceptions are [below](#security-exceptions) |
| E8 | CI artifacts have SBOMs and signed provenance tied to the same tested immutable digest | `supply-chain-contract`, `containers` | [rehearsal](clean-checkout-rehearsal.json); `release.yml` verifies each signature and attestation before tagging ([#101](https://github.com/mohripan/capsulet/issues/101)) |
| E9 | Rust crates, API metadata, SDK compatibility metadata, dashboard, images, and Helm chart agree with the release contract | `release`, `helm`, `containers` | [rehearsal](clean-checkout-rehearsal.json) |
| E10 | Compose and a real Kind-installed Helm release both pass their distinct smoke paths | `compose`, `kind` | [rehearsal](clean-checkout-rehearsal.json); the Kind smoke runs inside the cluster against Service names, never Compose |
| E11 | documentation describes limitations honestly and does not call Capsulet alpha-ready | `claims`, `claims-evidence` | [rehearsal](clean-checkout-rehearsal.json); the registry's maturities stop at `implemented`, and `CAP-HELM-001` says the chart is not a public-alpha distribution |

## Remaining risks

- **Verification stops at the kernel's own rules.** Without M4's verifier protocol and domain packs,
  a certificate rests on structural checks, budgets and human gates. The graph worker refuses a
  verifier node rather than pretending to run it.
- **Images are not reproducible.** Two builds of one commit have different image IDs, from build
  timestamps in the image configuration and the dashboard's generated build id. The released digest
  is the tested one, because it is built once. A rebuild cannot confirm it.
- **Branch protection on `main` required only `PostgreSQL` when M1 closed.** Since 2026-11-13, ahead
  of M4, the ruleset requires `Required`, which covers every gate. Repository admins can still bypass
  it, so a direct push is held to `Required` by practice rather than by the ruleset.
- **One Kubernetes shape.** The Kind gate installs on one node, Kubernetes 1.35, with persistence
  off. Upgrade, rollback, backup and restore are untested; that is M6's gate.
- **Wrong-project resources.** The HTTP matrix calls every operation as a member of another project
  and expects 403. A wrong-project *resource* returning 404 is checked over HTTP for workflows only;
  for everything else it rests on the SQL ownership tests in `postgres`.
- **The rehearsal ran on one machine.** It ran on Windows; CI runs Linux on every commit. macOS is
  untested.
- **Local Windows runs.** Editing xtask's sources while a gate runs breaks the gate's next `cargo
  run -p capsulet-xtask`, because Windows will not replace a running executable.
- **The supply chain's settings.** No ruleset protects `v*` tags, and images published under an
  earlier, doubled path remain in the registry. Both are repository settings.

## Security exceptions

M1 closes with these exceptions, each owned by `platform` and expiring. The gates fail once one
expires, or once a scan no longer needs it.

| Exception | Ecosystem | Scope | Expires | Issue |
| --- | --- | --- | --- | --- |
| `RUSTSEC-2023-0071` | rust | uncompiled-optional | 2027-03-01 | [#143](https://github.com/mohripan/capsulet/issues/143) |
| `RUSTSEC-2026-0459` | rust | uncompiled-optional | 2027-03-01 | [#143](https://github.com/mohripan/capsulet/issues/143) |
| `RUSTSEC-2024-0449` | rust | unmaintained-parser | 2027-03-01 | [#99](https://github.com/mohripan/capsulet/issues/99) |
| `SECRET-2026-0001` | secret | test-fixture | 2027-05-12 | [#101](https://github.com/mohripan/capsulet/issues/101) |
| `IMAGE-2026-0001` | image | upstream-image | 2027-02-12 | [#101](https://github.com/mohripan/capsulet/issues/101) |
| `IMAGE-2026-0002` | image | upstream-image | 2027-02-12 | [#101](https://github.com/mohripan/capsulet/issues/101) |
| `IMAGE-2026-0003` | image | upstream-image | **2026-11-26** | [#101](https://github.com/mohripan/capsulet/issues/101) |
| `IMAGE-2026-0004` | image | upstream-image | 2027-02-12 | [#101](https://github.com/mohripan/capsulet/issues/101) |
| `IMAGE-2026-0005` | image | upstream-image | 2027-02-12 | [#101](https://github.com/mohripan/capsulet/issues/101) |
| `IMAGE-2026-0006` | image | upstream-image | 2026-12-12 | [#101](https://github.com/mohripan/capsulet/issues/101) |

`IMAGE-2026-0003` expires two weeks after M1 closes. When it does, `dependency-images` fails until
the image is bumped or the exception is renewed with a reason.
[`security-exceptions.json`](security-exceptions.json) is the registry, and
[`security-exceptions.md`](security-exceptions.md) the policy.

## Platform support

| | Supported | Verified on |
| --- | --- | --- |
| Rust | 1.96, pinned in `rust-toolchain.toml` | 1.96.0 |
| Node.js, npm | 20 or newer, npm 10 or newer; 24 or newer for `claims-evidence` | npm 11.19 (Node.js 26); CI uses 22 and 24 |
| Python | 3.12 or newer | 3.14; CI uses 3.12 |
| Docker | Engine 27 or newer, Compose v2 | 29.6; GitHub's `ubuntu-latest` |
| Helm | 3.18 or newer | 4.1.4, here and in CI |
| Kind, kubectl | Kind 0.30 or newer; a kubectl the cluster supports | Kind 0.31.0; Kubernetes 1.35 nodes |
| Hosts | Linux, and Windows with PowerShell | Windows 11 (this rehearsal); Linux x86_64 (CI, every commit) |

`verify doctor` reports each tool against these minimums.

## Entry conditions for the next milestone

M2 (the verified-computation IR) and M3 (the durable graph runtime) were built before M1 closed, and
their gates (`ir`, `replay`, `chaos`) are part of the full profile above, so they stay green with
everything else. The next milestone in the roadmap's order is **M4, the verifier and domain
ecosystem**. It starts from [M3's entry conditions for M4](m3-completion.md#entry-conditions-for-m4)
and needs, before its first change lands:

- `Required` green on `main`, and branch protection requiring it, so M4 work cannot land over a red
  gate;
- every M1 gate staying in the full profile, and this report's check passing, whatever M4 adds;
- an ADR fixing the verifier protocol's boundary: what a validator receives and returns, how it is
  isolated, and how its result enters a certificate ([ADR 0021](../adr/0021-the-verifier-protocol-boundary.md));
- the exceptions above renewed or retired before they expire, rather than by the gate failing in
  the middle of M4 work.

M5 (interpretability) already has open work, and the plan does not require it to wait; it is held
to the same gates.
