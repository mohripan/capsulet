# Security Exceptions

Status: experimental. Every advisory the `security` gate is told to ignore is listed here, with why
it does not apply, who owns it, and the date the reasoning must be re-checked.

An exception with no owner is nobody's problem, and an exception with no expiry is permanent by
accident. `crates/xtask/tests/verify_cli.rs` fails the build when `.cargo/audit.toml` ignores an
advisory this file does not list, and when a listed review date has passed. So an exception cannot
be added quietly and cannot outlive its reasoning.

An exception is a statement that an advisory does not apply here — never that it does not matter.
Anything genuinely reachable gets fixed or the gate stays red.

<!-- machine-readable: id | owner | review-by -->

| Advisory | Crate | Owner | Review by | Why it does not apply |
| --- | --- | --- | --- | --- |
| `RUSTSEC-2023-0071` | `rsa` 0.9.10 | platform | 2027-03-01 | The Marvin timing side-channel needs the code to run. `rsa` reaches `Cargo.lock` only through `sqlx-mysql`, and the workspace pins `sqlx` with `default-features = false` and a postgres-only feature set, so `sqlx-mysql` is never compiled. `cargo tree -i rsa` reports nothing, and a workspace-wide `cargo tree -e normal` contains no `rsa` or `sqlx-mysql` node. `cargo audit` reads the lockfile rather than the build graph, so it cannot see that. The advisory has no fixed release, so the alternative to this exception is not a fix — it is either a red gate or dropping sqlx. Revisit when sqlx stops declaring the optional MySQL backend or a patched `rsa` ships. |

## What is not an exception

Advisories that were fixed rather than excused, kept here so the distinction stays visible:

| Advisory | Crate | What happened |
| --- | --- | --- |
| `RUSTSEC-2026-0258` | `h2` | Reachable through `hyper` and `axum`, so it was fixed: `cargo update -p h2` moved 0.4.14 to 0.4.19. |
