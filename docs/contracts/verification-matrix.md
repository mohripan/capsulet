<!-- capsulet-claims: CAP-OPENAPI-001 -->
# Verification Matrix

The executable source of truth is `cargo run -p capsulet-xtask --locked -- verify --list --format json`.
Every required gate records its prerequisites, timeout, commands, log artifact, and profile membership.

| Gate | Fast | Full | Purpose | Primary prerequisites |
| --- | --- | --- | --- | --- |
| `verifier` | yes | yes | Verifier failure/timeout/cleanup and dependency-policy regression tests | Cargo |
| `format` | yes | yes | Rust formatting | Cargo |
| `lint` | no | yes | Strict workspace Clippy | Cargo |
| `unit` | yes | yes | Locked workspace tests | Cargo |
| `ir` | yes | yes | IR and runtime contracts, crate purity, adapter coverage | Cargo |
| `replay` | yes | yes | Offline certificate replay and tamper detection | Cargo |
| `api-contracts` | yes | yes | Runtime/OpenAPI equality and drift | Cargo |
| `authorization` | yes | yes | Executed HTTP role/scope matrix and generated policy drift | Cargo |
| `claims` | yes | yes | Claims, lifecycle, and public surfaces | PowerShell |
| `release` | yes | yes | `release.toml` against every crate, the Python SDK, the dashboard, the chart, OpenAPI, Compose and the Dockerfiles, and every image and repository reference in tracked files ([release process](release-process.md)) | Cargo, Git |
| `supply-chain-contract` | yes | yes | Every workflow pins its actions by SHA, gives pull request code no write permission, builds each image once, and scans, attests, signs and verifies the pushed digest before tagging it; base images are pinned by digest and refreshed by Dependabot ([supply chain](../security/supply-chain.md)) | Cargo, Git |
| `ci-contract` | yes | yes | CI only schedules gates: no workflow step runs a check itself or behind an `if:`, a `ci.yml` matrix varies only platforms or toolchains, every Full gate runs in some job that uploads its `verification-summary.json`, and `Required` waits for every job, runs `if: always()` and checks the summaries ([CI](../development/ci.md)) | Cargo |
| `completion` | yes | yes | Every M1 release-gate item and exit criterion, word for word from the [engineering integrity plan](../superpowers/plans/2026-08-31-engineering-integrity.md), has a row in the [M1 completion report](m1-completion.md) naming full-profile gates and linking the [clean-checkout rehearsal record](clean-checkout-rehearsal.json). The record must be of an ancestor commit, from a full run of a clean checkout that changed nothing, left nothing and passed every gate the report cites | Cargo, Git |
| `claims-evidence` | yes | yes | Every claim's test evidence is collected, and not ignored or skipped, by the gate command it names; `docs/contracts/test-manifest.json` matches that collection | Cargo, Python, Node.js 24+ with npm, PowerShell |
| `sdk` | yes | yes | Python and dashboard transports | Python, npm |
| `dashboard` | no | yes | ESLint and production build | npm |
| `postgres` | no | yes | Isolated persistence integration | Cargo, Docker |
| `chaos` | no | yes | IR worker crash recovery at step boundaries | Cargo, Docker |
| `verifier-isolation` | no | yes | A validator in a real container, from an image pinned by digest, reaches no network, cannot write its inputs or its root, is stopped past its memory or its time, leaves no container behind, and runs again to the same outputs ([verifier protocol](verifier-protocol.md)); and `packs/template` passes its own test there and, once signed, installs ([domain packs](domain-packs.md)) | Cargo, Docker |
| `migrations` | no | yes | Supported forward migrations | Cargo, Docker |
| `security` | no | yes | Rust/npm/Python dependency and license policy, including expiring exceptions | cargo-audit, cargo-deny, npm, Python |
| `secrets` | no | yes | gitleaks, pinned by digest, over the whole Git history; every finding must be a registered, unexpired test fixture ([supply chain](../security/supply-chain.md)) | Cargo, Docker, Git |
| `dependency-images` | no | yes | Trivy, pinned by digest, over every third-party image the chart and Compose pull, by the digest they pin (`release dependency-images`), under `trivy.yaml`'s thresholds; every failing finding must be a registered, unexpired `image` exception, and an exception the scan no longer needs fails too ([supply chain](../security/supply-chain.md#dependency-images)) | Cargo, Docker |
| `gate-regressions` | no | yes | The `lint`, `dashboard` and `security` gates' own commands fail on a fixture with a Clippy warning, a crashing ESLint configuration, an unreviewed duplicate dependency and a crate with a published advisory, and pass on a clean one | Cargo with Clippy, cargo-audit, cargo-deny, npm with the dashboard installed |
| `compose` | no | yes | On a stack of its own (every service built and healthy, seeded alike twice): Compose configuration, agents run over HTTP with the graph worker killed mid-run, and a job leaves logs and a downloadable artifact; reseeding afterwards must leave the fixture as it was | Docker, PowerShell |
| `dashboard-e2e` | no | yes | On a stack of its own, seeded the same way: Playwright as admin, operator, viewer and service account, including the project-isolation flows for artifacts, memory review, agents and audit events; reseeding afterwards must leave the fixture as it was | Docker, npm |
| `helm` | no | yes | Chart lint and deterministic render; every workload runs the image `release.toml` names, by tag and by digest, in a non-root, seccomp-confined pod with no privilege escalation, every capability dropped, read-only root filesystems for Capsulet's own containers, probes, a service-account token only where it uses the Kubernetes API, and only secrets the chart renders or its values name | Helm, Cargo |
| `containers` | no | yes | Every component image built from `release.toml`'s plan as CI builds it, its OCI labels and non-root user checked, then removed | Cargo, Docker, Git |
| `kind` | no | yes | On a Kind cluster of its own, named for the run: images built here and loaded into it, generated test credentials, the workload rules and a server-side dry run before install, `helm test`, and an in-cluster smoke over Service names (health, dashboard login, a job run as a Kubernetes Job, an agent run under the correctness kernel, cross-project and role denial); then an uninstall that must leave nothing, diagnostics on failure, and the cluster deleted ([CI](../development/ci.md#the-kind-gate)) | Kind, kubectl, Helm, Docker, Cargo, Git |
| `trace-viewer` | no | no | On a stack of its own: a synthetic agent run's trace reaches Jaeger 2.23 (pinned by digest) over OTLP/HTTP and is found there by the trace id the run's bundle names, with Capsulet's spans and attributes and none of the run's input; on demand, since the viewer is optional ([ADR 0020](../adr/0020-an-optional-operator-trace-viewer.md)) | Docker, PowerShell |
| `wasm-python` | no | no | WASI Python runner, nightly by name; its test is ignored elsewhere and must run here. The runner's command line and validation are tested in `unit` | Cargo, Wasmtime |

`fast` prints its exact omissions at both the start and end. `full` cannot report success unless
every full-profile gate ran successfully, nothing named for the run is left behind (containers,
volumes, networks, images, Kind clusters, generated credential directories), and the worktree is as
it was when the run started; any run fails on what it left behind ([CI](../development/ci.md)).
Per-gate logs are written below `.capsulet/verify/`.
The verifier's own tests build in `.capsulet/verify/xtask-tests`, separate from the running
executable's target directory so Windows does not block replacing an executable in use.
Run directories include a timestamp and process ID and must be newly created, preventing
concurrent verifier commands from merging or overwriting their captured evidence.

In CI, every job uploads its run's `verification-summary.json`, and the `Required` job fails
unless `verify summary --require full` finds every Full gate there and passed, and every job it
waited for succeeded ([CI](../development/ci.md)). The `compose`
and `dashboard-e2e` stacks, their seed and their teardown are described in
[test identities](../development/test-identities.md). Disposable PostgreSQL containers are isolated from host data, and each database-backed
test gets a database of its own on them. The `postgres`, `migrations` and `chaos` gates fail when a
test binary ran nothing, or ignored or filtered out any test. Every run writes each binary's counts
to `.capsulet/verify/<run>/test-counts.json`; CI keeps that file as an artifact and shows the counts
in the job summary.
The clean-checkout rehearsal remains open under
[the integrity tracker](https://github.com/mohripan/capsulet/issues/104). Do not infer M1 completion
from the current `full` summary.

Supported baseline tools are Rust 1.99, Node.js 20/npm 10 or newer compatible releases, Python
3.12 or newer, Docker Engine 27 or newer with Compose v2, Helm 3.18 or newer, Kind 0.30 or newer,
and a kubectl release compatible with the target cluster. `verify doctor` asks each of these tools,
plus Git, PowerShell, `cargo-audit` and `cargo-deny`, for its version without changing anything, and
fails when one is missing or older than the minimum above, or than cargo-audit 0.22.2 and
cargo-deny 0.20.2, the scanner versions CI pins. kubectl, Git and PowerShell are only required to run.
Each gate separately fails when a declared prerequisite is missing.
