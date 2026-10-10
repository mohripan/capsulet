# Dependency security policy

Status: experimental. Required dependency checks run through the named `security` gate:

```powershell
cargo run -p capsulet-xtask --locked -- verify --gate lint --gate security
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

The current npm exception expires **2026-10-25** and is tracked in
[#118](https://github.com/mohripan/capsulet/issues/118). The raw npm audit still reports its
findings. Policy success never means the scanner reported zero vulnerabilities.

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

Allowed Rust licenses are declared in `deny.toml`; unknown registries/Git sources are denied.
The current duplicate-version warnings and full scanner-input/transitive-tool pinning still need
the policy reconciliation tracked in [#99](https://github.com/mohripan/capsulet/issues/99).
Do not claim those unfinished requirements are enforced merely because `cargo deny check` exits zero.
