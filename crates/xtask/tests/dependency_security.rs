use std::{fs, process::Command};

use serde_json::{Value, json};

fn registry() -> Value {
    json!({"schema_version": 1, "exceptions": [{
        "id": "GHSA-vfj7-8cjw-p6xm", "ecosystem": "npm", "package": "braces",
        "versions": ["3.0.3"], "scope": "development-only", "owner": "platform",
        "rationale": "No patched upstream version exists.",
        "compensating_control": "Only trusted repository glob patterns reach the lint tool.",
        "issue": "https://github.com/mohripan/capsulet/issues/118", "expires": "2026-10-25"
    }]})
}

fn report(id: &str, severity: &str) -> Value {
    json!({"auditReportVersion": 2, "vulnerabilities": {
        "braces": {"name":"braces", "severity":severity, "nodes":["node_modules/braces"],
            "via":[{"name":"braces", "url":format!("https://github.com/advisories/{id}"), "severity":severity}]},
        "micromatch": {"name":"micromatch", "severity":severity, "nodes":["node_modules/micromatch"], "via":["braces"]}
    }})
}

fn check(report: Value, registry: Value, dev: bool, today: &str) -> std::process::Output {
    let directory = tempfile::tempdir().expect("temporary directory");
    for (name, value) in [
        ("report.json", report),
        ("registry.json", registry),
        (
            "lock.json",
            json!({"lockfileVersion":3, "packages": {
                "node_modules/braces":{"version":"3.0.3","dev":dev},
                "node_modules/micromatch":{"version":"4.0.8","dev":dev}
            }}),
        ),
    ] {
        fs::write(directory.path().join(name), value.to_string()).expect("fixture");
    }
    Command::new(env!("CARGO_BIN_EXE_capsulet-xtask"))
        .args(["security", "check-npm", "--report"])
        .arg(directory.path().join("report.json"))
        .arg("--registry")
        .arg(directory.path().join("registry.json"))
        .arg("--lockfile")
        .arg(directory.path().join("lock.json"))
        .args(["--today", today])
        .output()
        .expect("checker runs")
}

#[test]
fn accepts_only_the_exact_owned_unexpired_dev_advisory_and_its_chain() {
    let output = check(
        report("GHSA-vfj7-8cjw-p6xm", "high"),
        registry(),
        true,
        "2026-10-11",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("EXCEPTION"));
}

#[test]
fn rejects_a_new_high_advisory() {
    let output = check(
        report("GHSA-new1-new2-new3", "high"),
        registry(),
        true,
        "2026-10-11",
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unexcepted"));
}

#[test]
fn rejects_an_expired_exception() {
    let output = check(
        report("GHSA-vfj7-8cjw-p6xm", "high"),
        registry(),
        true,
        "2026-10-25",
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("expired"));
}

#[test]
fn dev_exception_cannot_cover_a_production_dependency() {
    let output = check(
        report("GHSA-vfj7-8cjw-p6xm", "high"),
        registry(),
        false,
        "2026-10-11",
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("development-only"));
}

#[test]
fn advisory_exception_cannot_cover_a_critical_severity_escalation() {
    let output = check(
        report("GHSA-vfj7-8cjw-p6xm", "critical"),
        registry(),
        true,
        "2026-10-11",
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("critical"));
}

#[test]
fn malformed_or_error_audit_output_does_not_pass() {
    for report in [json!({}), json!({"error":{"code":"ENETUNREACH"}})] {
        let output = check(report, registry(), true, "2026-10-11");
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("invalid npm audit"));
    }
}

#[test]
fn unknown_severity_is_a_contract_error_not_a_silent_skip() {
    let output = check(
        report("GHSA-vfj7-8cjw-p6xm", "unknown"),
        registry(),
        true,
        "2026-10-11",
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid npm audit"));
}

#[test]
fn missing_exception_owner_is_rejected() {
    let mut exceptions = registry();
    exceptions["exceptions"][0]["owner"] = json!("");
    let output = check(
        report("GHSA-vfj7-8cjw-p6xm", "high"),
        exceptions,
        true,
        "2026-10-11",
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unowned"));
}

const FIXTURE: &str =
    "9dc3d5076d5409cf488cfa0801378649e4567299:crates/api/src/auth.rs:generic-api-key:149";

fn secret_registry(scope: &str, expires: &str) -> Value {
    json!({"schema_version": 1, "exceptions": [{
        "id": "SECRET-2026-0001", "ecosystem": "secret", "package": "crates/api/src/auth.rs",
        "versions": [FIXTURE], "scope": scope, "owner": "platform",
        "rationale": "A unit-test bearer token that no deployment accepts.",
        "compensating_control": "The secrets gate scans the whole history.",
        "issue": "https://github.com/mohripan/capsulet/issues/101", "expires": expires
    }]})
}

fn secret(fingerprint: &str, file: &str) -> Value {
    json!({"RuleID": "generic-api-key", "File": file, "StartLine": 149,
        "Commit": fingerprint.split(':').next(), "Fingerprint": fingerprint, "Secret": "REDACTED"})
}

fn check_secrets(report: &Value, registry: &Value, staged: bool) -> std::process::Output {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(directory.path().join("report.json"), report.to_string()).expect("report");
    fs::write(directory.path().join("registry.json"), registry.to_string()).expect("registry");
    let mut command = Command::new(env!("CARGO_BIN_EXE_capsulet-xtask"));
    command
        .args(["security", "check-secrets", "--report"])
        .arg(directory.path().join("report.json"))
        .arg("--registry")
        .arg(directory.path().join("registry.json"))
        .args(["--today", "2026-11-12"]);
    if staged {
        command.arg("--staged");
    }
    command.output().expect("checker runs")
}

#[test]
fn a_registered_test_fixture_passes_the_secret_policy() {
    let output = check_secrets(
        &json!([secret(FIXTURE, "crates/api/src/auth.rs")]),
        &secret_registry("test-fixture", "2027-05-12"),
        false,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("EXCEPTION SECRET-2026-0001"));
}

#[test]
fn the_same_string_added_by_a_later_commit_is_a_new_finding() {
    let later =
        "0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f:crates/api/src/auth.rs:generic-api-key:560";
    let output = check_secrets(
        &json!([
            secret(FIXTURE, "crates/api/src/auth.rs"),
            secret(later, "crates/api/src/auth.rs")
        ]),
        &secret_registry("test-fixture", "2027-05-12"),
        false,
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("1 secret(s) found outside the exception registry")
    );
}

#[test]
fn a_staged_secret_always_fails() {
    let output = check_secrets(
        &json!([secret(
            "crates/api/src/new.rs:generic-api-key:3",
            "crates/api/src/new.rs"
        )]),
        &secret_registry("test-fixture", "2027-05-12"),
        true,
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("found outside the exception registry")
    );
}

#[test]
fn a_registered_fingerprint_that_is_gone_is_stale() {
    let output = check_secrets(
        &json!([]),
        &secret_registry("test-fixture", "2027-05-12"),
        false,
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("stale secret exception SECRET-2026-0001")
    );
    // A staged scan sees only the change, so it cannot tell.
    let staged = check_secrets(
        &json!([]),
        &secret_registry("test-fixture", "2027-05-12"),
        true,
    );
    assert!(
        staged.status.success(),
        "{}",
        String::from_utf8_lossy(&staged.stderr)
    );
}

#[test]
fn an_expired_or_mis_scoped_secret_exception_is_refused() {
    let report = json!([secret(FIXTURE, "crates/api/src/auth.rs")]);
    let expired = check_secrets(
        &report,
        &secret_registry("test-fixture", "2026-11-01"),
        false,
    );
    assert!(
        String::from_utf8_lossy(&expired.stderr).contains("expired exception SECRET-2026-0001")
    );
    let scoped = check_secrets(
        &report,
        &secret_registry("development-only", "2027-05-12"),
        false,
    );
    assert!(
        String::from_utf8_lossy(&scoped.stderr)
            .contains("invalid or unowned exception SECRET-2026-0001")
    );
}
