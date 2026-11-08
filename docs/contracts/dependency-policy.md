# Dependency security policy

Status: experimental. Required dependency checks run through the named `security` gate:

```powershell
cargo run -p capsulet-xtask --locked -- verify --gate lint --gate security --gate gate-regressions
```

The gate refreshes RustSec/npm/PyPI advisory inputs and records findings in local verification
logs. Auditing requires network access; unavailable or malformed scanner output is a failure,
not an empty vulnerability set. Do not confuse this gate with the still-unfinished container,
secret-scanning and supply-chain deliverables tracked in [#101](https://github.com/mohripan/capsulet/issues/101).

## Thresholds and exceptions

- Rust vulnerabilities fail unless explicitly registered and scoped; maintenance advisories are
  reviewed separately. `cargo-audit` and `cargo-deny` ignore lists must agree with the registry.
- npm high and critical findings fail. Only an exact advisory/package/version, development-only
  high finding may be excepted. Critical escalation, runtime reachability, a changed version or
  unknown advisory fails. Production dependencies run an additional audit with no exceptions.
- Python advisories fail without an exception mechanism. The SDK's runtime, optional and build
  requirements are resolved and audited, including its build backend when runtime dependencies
  are empty. Dynamic dependency declarations require a reviewed adapter and currently fail.
- Exceptions require an owner, rationale, compensating control, tracking issue and expiry in
  `security-exceptions.json`; expiry is enforced on the date itself. The JSON schema documents
  the format and the Rust checker rejects missing/unknown fields and invalid dates.

No npm exception is registered. The braces exception tracked in
[#118](https://github.com/mohripan/capsulet/issues/118) was retired once a patched version
resolved, and on 2026-11-08 the full `npm audit --audit-level=high`, development dependencies
included, passed with only moderate findings. Policy success never means the scanner reported
zero vulnerabilities.

## Tooling, artifacts and updates

Rust format and Clippy run with `-D warnings` across the workspace, all targets and features.
Dashboard ESLint, TypeScript, unit tests and production build are separate required checks.
Rust lockfiles and npm lockfiles must not be rewritten by verification commands.

`xtask security npm` writes `.capsulet/security/npm-audit.json`. Python uses `pip-audit` 2.10.1
in a fresh temporary virtual environment, writes `.capsulet/security/python-audit.json`, and
removes the environment during normal or failed teardown. It requires Python 3.11+ with venv
and pip; CI uses 3.12. No packages are installed into the developer's global Python environment.
The report records auditor version, declared requirements and resolved packages.

Review reachable advisories immediately, automated dependency proposals weekly, and maintenance
exceptions before expiry. Upgrade compatible versions before considering an exception; test
contracts and builds after upgrades. Do not downgrade supported tooling merely to conceal a finding.

## Licences, sources and duplicate versions

`deny.toml` allows only the licences a dependency uses today. A dependency under any other
licence fails the gate until someone reads the licence and adds it. Unknown registries and Git
sources are denied.

A second version of a crate fails `cargo deny check` (`multiple-versions = "deny"`) unless
`deny.toml` lists it under `[bans] skip` with a reason. The reason says who pulls the version in,
and what would let it go. Only older versions are listed: the newest is never the one waved
through. Each entry names an exact version, so an update that brings in a different one fails
and gets looked at. `every_skipped_duplicate_is_reviewed_and_still_in_the_lockfile` fails when
an entry has no reason, skips the newest version, or names a version the lockfile no longer
has.

Before adding an entry, check whether the workspace causes the duplicate, and upgrade if it
does. On 2026-11-08 `reqwest` 0.12 was the only duplicate the workspace caused, and moving to
0.13 removed it. The 32 crates still listed are all held back by third-party crates mid-move:

- RustCrypto 0.10 through sqlx
- `rand` 0.8 and 0.9 through sqlx, governor and OpenTelemetry
- `syn` 2 through most proc-macro crates
- `windows-sys` 0.48 and 0.52 through etcetera, winapi-util and ring

`cargo deny check` still prints one warning: it never meets RUSTSEC-2023-0071, because `rsa` is
reachable only through the uncompiled `sqlx-mysql`. `cargo-audit` reads the lockfile and does
meet it, and the two ignore lists are held equal on purpose.

## Scanner versions and regression checks

CI installs `cargo-audit` 0.22.2 and `cargo-deny` 0.20.2 by exact version
(`.github/workflows/supply-chain.yml`), so a scanner release cannot change what passes without a
commit. `verify doctor` holds a developer machine to those versions as minimums. `pip-audit` is
pinned to 2.10.1 as above. Bump a pin in a commit of its own, with the gate's result.

The `gate-regressions` gate shows each gate failing on the defect it exists to catch. It runs
the `lint`, `dashboard` and `security` gates' own commands, under the repository's own
`deny.toml` and `.cargo/audit.toml`, against four fixtures:

- a crate with a Clippy warning
- a crashing ESLint configuration
- an unreviewed duplicate dependency
- a lockfile with a crate that has a published advisory (RUSTSEC-2021-0003)

Each fixture has a clean twin that must pass the same command. The supply-chain workflow runs
it after `security`.
