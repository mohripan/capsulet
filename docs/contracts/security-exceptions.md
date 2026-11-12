# Security Exceptions

Status: experimental. Every advisory the `security` gate is told to ignore is listed here, with why
it does not apply, who owns it, and the date the reasoning must be re-checked.

`security-exceptions.json` is the structured registry, with exact package versions, scope,
owner, rationale, compensating control, issue and expiry. The gate validates it at runtime;
Rust tool ignores must also match the registry. This Markdown view provides review context.

`xtask security npm` preserves raw output in `.capsulet/security/npm-audit.json`, prints any exception
and checks its ownership, expiry and reachability. A policy pass is **not** a zero-advisory scanner
result. Production dependencies must pass an unexcepted audit.

An exception with no owner is nobody's problem, and an exception with no expiry is permanent by
accident. `crates/xtask/tests/verify_cli.rs` fails the build when `.cargo/audit.toml` ignores an
advisory this file does not list, and when a listed review date has passed. So an exception cannot
be added quietly and cannot outlive its reasoning.

An exception is a statement that an advisory does not apply here — never that it does not matter.
Anything genuinely reachable gets fixed or the gate stays red.

<!-- machine-readable: id | owner | review-by -->

| Advisory | Crate | Owner | Review by | Why it does not apply |
| --- | --- | --- | --- | --- |
| `RUSTSEC-2024-0449` | `unsafe-libyaml` 0.2.11 | platform | 2027-03-01 | An unmaintained-crate warning, not a vulnerability. It is reached through `serde_yaml`, which the workspace uses only for the execution pool configuration an operator writes, never a request or a run, and which `kube-client` uses for kubeconfig. Replacing the workspace's own use would not remove it while `kube-client` depends on it; review kube's YAML dependency before this expires. |
| `RUSTSEC-2023-0071` | `rsa` 0.9.10 | platform | 2027-03-01 | The Marvin timing side-channel needs the code to run. `rsa` reaches `Cargo.lock` only through `sqlx-mysql`, and the workspace pins `sqlx` with `default-features = false` and a postgres-only feature set, so `sqlx-mysql` is never compiled. `cargo tree -i rsa` reports nothing, and a workspace-wide `cargo tree -e normal` contains no `rsa` or `sqlx-mysql` node. `cargo audit` reads the lockfile rather than the build graph, so it cannot see that. The advisory has no fixed release, so the alternative to this exception is not a fix — it is either a red gate or dropping sqlx. Revisit when sqlx stops declaring the optional MySQL backend or a patched `rsa` ships; tracked in [#143](https://github.com/mohripan/capsulet/issues/143). |
| `RUSTSEC-2026-0459` | `libsqlite3-sys` 0.30.1 | platform | 2027-03-01 | A CVE in the bundled SQLite, which needs the code to run. `libsqlite3-sys` reaches `Cargo.lock` only through `sqlx-sqlite`, because `sqlx-macros-core` lists every driver as optional; the workspace enables only PostgreSQL, so it is never compiled. `every_uncompiled_exception_is_uncompiled` fails if that changes. The fix (0.39) needs a newer sqlx major; tracked in [#143](https://github.com/mohripan/capsulet/issues/143). |

## What is not an exception

Advisories that were fixed rather than excused, kept here so the distinction stays visible:

| Advisory | Crate | What happened |
| --- | --- | --- |
| `RUSTSEC-2026-0258` | `h2` | Reachable through `hyper` and `axum`, so it was fixed: `cargo update -p h2` moved 0.4.14 to 0.4.19. |
| `RUSTSEC-2026-0221` | `event-listener` | Unsound `!Send` handling, reached through `sqlx-core`. Fixed by `cargo update`. |
| yanked release | `chacha20` | 0.10.1 was yanked; `cargo update` moved to 0.10.2. |

## Container images

`trivy.yaml` is the image-scanning policy, and it carries one exception of its own: `ignore-unfixed`.

| Policy | Owner | Review by | Why |
| --- | --- | --- | --- |
| `ignore-unfixed` in `trivy.yaml` | platform | 2027-03-01 | Every HIGH and CRITICAL finding in the runtime image today is one Debian has marked `affected`, `fix_deferred`, or `will_not_fix` — 94 of them across 27 packages, and not one has a fixed version to move to. Failing the build on those does not make the image safer; it makes the scan permanently red and therefore unread. What remains after the filter is the actionable set: a published fix this image has not picked up, which one rebuild removes. The unfixed findings are still reported in full to GitHub code scanning by a separate step, so nothing is hidden — only the build-failing threshold is narrowed. Revisit when the base image moves to trixie, which resets the set. |

The runtime image runs `apt-get upgrade` before installing, so a fix Debian publishes lands on the
next build rather than waiting for someone to notice. That is what makes a fixable finding a real
defect rather than a fact about how old the base tag is.
