<!-- capsulet-claims: CAP-OPENAPI-001 -->
# Verification Matrix

The executable source of truth is `cargo run -p capsulet-xtask --locked -- verify --list --format json`.
Every required gate records its prerequisites, timeout, commands, log artifact, and profile membership.

| Gate | Fast | Full | Purpose | Primary prerequisites |
| --- | --- | --- | --- | --- |
| `format` | yes | yes | Rust formatting | Cargo |
| `lint` | no | yes | Strict workspace Clippy | Cargo |
| `unit` | yes | yes | Locked workspace tests | Cargo |
| `ir` | yes | yes | IR canonical bytes, digests, and crate purity | Cargo |
| `replay` | yes | yes | Offline certificate replay and tamper detection | Cargo |
| `api-contracts` | yes | yes | Runtime/OpenAPI equality and drift | Cargo |
| `claims` | yes | yes | Claims, lifecycle, and public surfaces | PowerShell |
| `sdk` | yes | yes | Python and dashboard transports | Python, npm |
| `dashboard` | no | yes | ESLint and production build | npm |
| `postgres` | no | yes | Isolated persistence integration | Cargo, Docker |
| `migrations` | no | yes | Supported forward migrations | Cargo, Docker |
| `security` | no | yes | Rust and npm dependency audit | cargo-audit, npm |
| `compose` | no | yes | Compose validation and smoke | Docker |
| `helm` | no | yes | Chart lint and deterministic render | Helm |
| `kind` | no | yes | Installed-cluster smoke | Kind, kubectl, Helm, Docker |

`fast` prints its exact omissions at both the start and end. `full` cannot report success unless
every full-profile gate ran successfully. Per-gate logs are written below `.capsulet/verify/`.

Supported baseline tools are Rust 1.96, Node.js 20/npm 10 or newer compatible releases, Python
3.12 or newer, Docker Engine 27 or newer with Compose v2, Helm 3.18 or newer, Kind 0.30 or newer,
and a kubectl release compatible with the target cluster. `verify doctor` performs read-only
availability/version probes for these tools and the configured security scanners.
