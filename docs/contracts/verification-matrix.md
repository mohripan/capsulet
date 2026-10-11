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
| `sdk` | yes | yes | Python and dashboard transports | Python, npm |
| `dashboard` | no | yes | ESLint and production build | npm |
| `postgres` | no | yes | Isolated persistence integration | Cargo, Docker |
| `chaos` | no | yes | IR worker crash recovery at step boundaries | Cargo, Docker |
| `migrations` | no | yes | Supported forward migrations | Cargo, Docker |
| `security` | no | yes | Rust/npm/Python dependency and license policy, including expiring exceptions | cargo-audit, cargo-deny, npm, Python |
| `compose` | no | yes | Compose configuration validation only; smoke pending [#98](https://github.com/mohripan/capsulet/issues/98) | Docker |
| `helm` | no | yes | Chart lint and deterministic render | Helm |
| `kind` | no | yes | Cluster discovery only; install/smoke pending [#102](https://github.com/mohripan/capsulet/issues/102) | Kind, kubectl, Helm, Docker |
| `wasm-python` | no | no | WASI Python runner, nightly by name | Cargo, Wasmtime |

`fast` prints its exact omissions at both the start and end. `full` cannot report success unless
every full-profile gate ran successfully. Per-gate logs are written below `.capsulet/verify/`.
The verifier's own tests build in `.capsulet/verify/xtask-tests`, separate from the running
executable's target directory so Windows does not block replacing an executable in use.
Run directories include a timestamp and process ID and must be newly created, preventing
concurrent verifier commands from merging or overwriting their captured evidence.

The current graph is provisional, not the complete M1 release gate. In particular, a green
`compose` or `kind` result does not prove a deployment was built, installed, exercised, or
uninstalled. Disposable PostgreSQL containers are isolated from host data, but per-test mutable
database isolation still needs reconciliation in [#96](https://github.com/mohripan/capsulet/issues/96).
Container scanning, secret scanning, immutable artifact attestations, release identity, required-job
aggregation and the clean-checkout rehearsal remain open under
[the integrity tracker](https://github.com/mohripan/capsulet/issues/104). Do not infer M1 completion
from the current `full` summary.

Supported baseline tools are Rust 1.96, Node.js 20/npm 10 or newer compatible releases, Python
3.12 or newer, Docker Engine 27 or newer with Compose v2, Helm 3.18 or newer, Kind 0.30 or newer,
and a kubectl release compatible with the target cluster. `verify doctor` performs read-only
availability probes for its listed tools; it does not yet validate all minimum versions or every
gate-specific scanner. Each gate separately fails when a declared prerequisite is missing.
