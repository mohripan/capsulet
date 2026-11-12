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
| `claims-evidence` | yes | yes | Every claim's test evidence is collected, and not ignored or skipped, by the gate command it names; `docs/contracts/test-manifest.json` matches that collection | Cargo, Python, Node.js 24+ with npm, PowerShell |
| `sdk` | yes | yes | Python and dashboard transports | Python, npm |
| `dashboard` | no | yes | ESLint and production build | npm |
| `postgres` | no | yes | Isolated persistence integration | Cargo, Docker |
| `chaos` | no | yes | IR worker crash recovery at step boundaries | Cargo, Docker |
| `migrations` | no | yes | Supported forward migrations | Cargo, Docker |
| `security` | no | yes | Rust/npm/Python dependency and license policy, including expiring exceptions | cargo-audit, cargo-deny, npm, Python |
| `gate-regressions` | no | yes | The `lint`, `dashboard` and `security` gates' own commands fail on a fixture with a Clippy warning, a crashing ESLint configuration, an unreviewed duplicate dependency and a crate with a published advisory, and pass on a clean one | Cargo with Clippy, cargo-audit, cargo-deny, npm with the dashboard installed |
| `compose` | no | yes | On a stack of its own (every service built and healthy, seeded alike twice): Compose configuration, agents run over HTTP with the graph worker killed mid-run, and a job leaves logs and a downloadable artifact; reseeding afterwards must leave the fixture as it was | Docker, PowerShell |
| `dashboard-e2e` | no | yes | On a stack of its own, seeded the same way: Playwright as admin, operator, viewer and service account, including the project-isolation flows for artifacts, memory review, agents and audit events; reseeding afterwards must leave the fixture as it was | Docker, npm |
| `helm` | no | yes | Chart lint and deterministic render | Helm |
| `kind` | no | yes | Cluster discovery only; install/smoke pending [#102](https://github.com/mohripan/capsulet/issues/102) | Kind, kubectl, Helm, Docker |
| `wasm-python` | no | no | WASI Python runner, nightly by name | Cargo, Wasmtime |

`fast` prints its exact omissions at both the start and end. `full` cannot report success unless
every full-profile gate ran successfully. Per-gate logs are written below `.capsulet/verify/`.
The verifier's own tests build in `.capsulet/verify/xtask-tests`, separate from the running
executable's target directory so Windows does not block replacing an executable in use.
Run directories include a timestamp and process ID and must be newly created, preventing
concurrent verifier commands from merging or overwriting their captured evidence.

The current graph is provisional, not the complete M1 release gate. In particular, a green `kind`
result does not prove a deployment was built, installed, exercised, or uninstalled. The `compose`
and `dashboard-e2e` stacks, their seed and their teardown are described in
[test identities](../development/test-identities.md). Disposable PostgreSQL containers are isolated from host data, and each database-backed
test gets a database of its own on them. The `postgres`, `migrations` and `chaos` gates fail when a
test binary ran nothing, or ignored or filtered out any test. Every run writes each binary's counts
to `.capsulet/verify/<run>/test-counts.json`; CI keeps that file as an artifact and shows the counts
in the job summary.
Container scanning, secret scanning, immutable artifact attestations, release identity, required-job
aggregation and the clean-checkout rehearsal remain open under
[the integrity tracker](https://github.com/mohripan/capsulet/issues/104). Do not infer M1 completion
from the current `full` summary.

Supported baseline tools are Rust 1.96, Node.js 20/npm 10 or newer compatible releases, Python
3.12 or newer, Docker Engine 27 or newer with Compose v2, Helm 3.18 or newer, Kind 0.30 or newer,
and a kubectl release compatible with the target cluster. `verify doctor` asks each of these tools,
plus PowerShell, `cargo-audit` and `cargo-deny`, for its version without changing anything, and
fails when one is missing or older than the minimum above, or than cargo-audit 0.22.2 and
cargo-deny 0.20.2, the scanner versions CI pins. kubectl and PowerShell are only required to run.
Each gate separately fails when a declared prerequisite is missing.
