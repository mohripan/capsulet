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
