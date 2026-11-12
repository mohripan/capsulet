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

const SEAWEEDFS: &str = "chrislusf/seaweedfs:4.53@sha256:ccad23e18639c8b98cee17e7100b95830a06077b56d8e5b006d6dcf8ec5e94a0";
const POSTGRES: &str =
    "postgres:16-alpine@sha256:aa6cb33cf4c19c765cefcf48d96176258800870a9a1e930ad126200f9d825139";

fn image_registry(scope: &str, expires: &str) -> Value {
    json!({"schema_version": 1, "exceptions": [{
        "id": "IMAGE-2026-0001", "ecosystem": "image", "package": "chrislusf/seaweedfs",
        "versions": ["CVE-2026-78661"], "scope": scope, "owner": "platform",
        "rationale": "The HTML parser is not linked into the weed binary.",
        "compensating_control": "The gate fails once a newer pin no longer has it.",
        "issue": "https://github.com/mohripan/capsulet/issues/101", "expires": expires
    }]})
}

fn no_exceptions() -> Value {
    json!({"schema_version": 1, "exceptions": []})
}

/// A Trivy JSON report of one image.
fn image_report(reference: &str, vulnerabilities: &[Value]) -> Value {
    json!({"SchemaVersion": 2, "ArtifactName": reference, "ArtifactType": "container_image",
    "Results": [
        {"Target": "alpine 3.22", "Class": "os-pkgs", "Type": "alpine"},
        {"Target": "usr/bin/weed", "Class": "lang-pkgs", "Type": "gobinary",
            "Vulnerabilities": vulnerabilities}
    ]})
}

fn vulnerability(id: &str, severity: &str, status: &str) -> Value {
    json!({"VulnerabilityID": id, "PkgName": "golang.org/x/net", "PkgPath": "usr/bin/weed",
        "InstalledVersion": "v0.61.0", "FixedVersion": "0.62.0", "Status": status,
        "Severity": severity})
}

/// `security check-images` on these reports, under the repository's trivy.yaml.
fn check_images(reports: &[Value], registry: &Value, today: &str) -> std::process::Output {
    let directory = tempfile::tempdir().expect("temporary directory");
    let scans = directory.path().join("reports");
    fs::create_dir(&scans).expect("reports");
    for (index, report) in reports.iter().enumerate() {
        fs::write(scans.join(format!("{index}.json")), report.to_string()).expect("report");
    }
    fs::write(directory.path().join("registry.json"), registry.to_string()).expect("registry");
    Command::new(env!("CARGO_BIN_EXE_capsulet-xtask"))
        .args(["security", "check-images", "--reports"])
        .arg(&scans)
        .arg("--registry")
        .arg(directory.path().join("registry.json"))
        .args(["--today", today])
        .output()
        .expect("checker runs")
}

#[test]
fn a_registered_image_finding_passes() {
    let output = check_images(
        &[image_report(
            SEAWEEDFS,
            &[vulnerability("CVE-2026-78661", "HIGH", "fixed")],
        )],
        &image_registry("upstream-image", "2026-12-12"),
        "2026-11-12",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("EXCEPTION IMAGE-2026-0001: CVE-2026-78661 (HIGH) in golang.org/x/net"),
        "{stdout}"
    );
}

#[test]
fn an_unregistered_image_finding_fails() {
    let output = check_images(
        &[image_report(
            SEAWEEDFS,
            &[
                vulnerability("CVE-2026-78661", "HIGH", "fixed"),
                vulnerability("CVE-2026-99999", "CRITICAL", "fixed"),
            ],
        )],
        &image_registry("upstream-image", "2026-12-12"),
        "2026-11-12",
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(
            "1 finding(s) in dependency images fail trivy.yaml outside the exception registry"
        ) && stderr.contains("CVE-2026-99999 (CRITICAL)"),
        "{stderr}"
    );
}

#[test]
fn an_image_exception_covers_its_own_repository_only() {
    let output = check_images(
        &[
            image_report(
                SEAWEEDFS,
                &[vulnerability("CVE-2026-78661", "HIGH", "fixed")],
            ),
            image_report(
                POSTGRES,
                &[vulnerability("CVE-2026-78661", "HIGH", "fixed")],
            ),
        ],
        &image_registry("upstream-image", "2026-12-12"),
        "2026-11-12",
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!(
            "CVE-2026-78661 (HIGH) in golang.org/x/net v0.61.0, fixed in 0.62.0, at usr/bin/weed \
             in {POSTGRES}"
        )),
        "{stderr}"
    );
}

#[test]
fn a_finding_trivy_yaml_does_not_fail_on_needs_no_exception() {
    let output = check_images(
        &[image_report(
            SEAWEEDFS,
            &[
                vulnerability("CVE-2026-10001", "MEDIUM", "fixed"),
                vulnerability("CVE-2026-10002", "CRITICAL", "affected"),
                vulnerability("CVE-2026-10003", "HIGH", "will_not_fix"),
            ],
        )],
        &no_exceptions(),
        "2026-11-12",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("no image has a finding that fails trivy.yaml")
    );
}

#[test]
fn an_expired_image_exception_fails() {
    let output = check_images(
        &[image_report(
            SEAWEEDFS,
            &[vulnerability("CVE-2026-78661", "HIGH", "fixed")],
        )],
        &image_registry("upstream-image", "2026-12-12"),
        "2026-12-12",
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("expired exception IMAGE-2026-0001"));
}

#[test]
fn an_image_exception_the_scan_no_longer_needs_is_stale() {
    let registry = image_registry("upstream-image", "2026-12-12");
    // A newer pin that no longer has it, or no fix to move to any more.
    for vulnerabilities in [
        vec![],
        vec![vulnerability("CVE-2026-78661", "HIGH", "affected")],
    ] {
        let output = check_images(
            &[image_report(SEAWEEDFS, &vulnerabilities)],
            &registry,
            "2026-11-12",
        );
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(
                "stale image exception IMAGE-2026-0001: CVE-2026-78661 no longer fails trivy.yaml \
                 in chrislusf/seaweedfs"
            ),
            "{stderr}"
        );
    }
    // An image nothing pulls any more.
    let output = check_images(&[image_report(POSTGRES, &[])], &registry, "2026-11-12");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains(
        "stale image exception IMAGE-2026-0001: chrislusf/seaweedfs is not an image the chart or \
         Compose pulls"
    ));
}

#[test]
fn an_image_exception_must_be_scoped_to_an_upstream_image() {
    let report = image_report(
        SEAWEEDFS,
        &[vulnerability("CVE-2026-78661", "HIGH", "fixed")],
    );
    for scope in ["development-only", "test-fixture"] {
        let output = check_images(
            std::slice::from_ref(&report),
            &image_registry(scope, "2026-12-12"),
            "2026-11-12",
        );
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("invalid or unowned exception IMAGE-2026-0001")
        );
    }
}

#[test]
fn a_malformed_image_report_fails() {
    let mut old = image_report(SEAWEEDFS, &[]);
    old["SchemaVersion"] = json!(1);
    let unknown = image_report(
        SEAWEEDFS,
        &[vulnerability("CVE-2026-78661", "SEVERE", "fixed")],
    );
    let mut filesystem = image_report(SEAWEEDFS, &[]);
    filesystem["ArtifactType"] = json!("filesystem");
    for report in [old, unknown, filesystem] {
        let output = check_images(&[report], &no_exceptions(), "2026-11-12");
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("invalid Trivy report"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
