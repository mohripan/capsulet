use std::{path::Path, process::Command};

fn xtask() -> Command {
    Command::new(env!("CARGO_BIN_EXE_capsulet-xtask"))
}

fn output(arguments: &[&str]) -> std::process::Output {
    xtask().args(arguments).output().expect("xtask runs")
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn rejects_unknown_profiles() {
    let result = output(&["verify", "--profile", "mystery"]);
    assert!(!result.status.success());
    assert!(stderr(&result).contains("unknown verification profile"));
}

#[test]
fn reports_absent_prerequisites() {
    let result = xtask()
        .args(["verify", "--gate", "format"])
        .env("CAPSULET_VERIFY_FORCE_MISSING", "cargo")
        .output()
        .expect("xtask runs");
    assert!(!result.status.success());
    assert!(stderr(&result).contains("missing prerequisite: cargo"));
}

#[test]
fn propagates_failed_children() {
    let result = xtask()
        .args(["verify", "--gate", "format"])
        .env("CAPSULET_VERIFY_TEST_COMMAND", "fail")
        .output()
        .expect("xtask runs");
    assert!(!result.status.success());
    assert!(stderr(&result).contains("format: failed"));
}

#[test]
fn terminates_timed_out_children() {
    let result = xtask()
        .args(["verify", "--gate", "format"])
        .env("CAPSULET_VERIFY_TEST_COMMAND", "sleep")
        .env("CAPSULET_VERIFY_TEST_TIMEOUT_MS", "20")
        .output()
        .expect("xtask runs");
    assert!(!result.status.success());
    assert!(stderr(&result).contains("format: timed out"));
}

#[test]
fn runs_cleanup_after_failure() {
    let directory = tempfile::tempdir().expect("temp directory");
    let marker = directory.path().join("cleanup.marker");
    let result = xtask()
        .args(["verify", "--gate", "format"])
        .env("CAPSULET_VERIFY_TEST_COMMAND", "fail")
        .env("CAPSULET_VERIFY_TEST_CLEANUP_MARKER", &marker)
        .output()
        .expect("xtask runs");
    assert!(!result.status.success());
    assert!(Path::new(&marker).is_file(), "cleanup hook did not run");
}

#[test]
fn reports_cleanup_failure() {
    let result = xtask()
        .args(["verify", "--gate", "format"])
        .env("CAPSULET_VERIFY_TEST_COMMAND", "fail")
        .env("CAPSULET_VERIFY_TEST_CLEANUP_FAIL", "1")
        .output()
        .expect("xtask runs");
    assert!(!result.status.success());
    assert!(stderr(&result).contains("cleanup failed"));
}

#[test]
fn cannot_skip_a_required_gate() {
    let result = output(&["verify", "--profile", "full", "--skip", "format"]);
    assert!(!result.status.success());
    assert!(stderr(&result).contains("cannot skip required gate: format"));
}

#[test]
fn lists_the_executable_gate_graph_as_json() {
    let result = output(&["verify", "--list", "--format", "json"]);
    assert!(result.status.success(), "{}", stderr(&result));
    let graph: serde_json::Value = serde_json::from_slice(&result.stdout).expect("JSON gate graph");
    for gate in [
        "format",
        "lint",
        "unit",
        "api-contracts",
        "sdk",
        "dashboard",
        "postgres",
        "migrations",
        "security",
        "compose",
        "helm",
        "kind",
    ] {
        assert!(
            graph
                .as_array()
                .expect("gate list")
                .iter()
                .any(|item| item["name"] == gate)
        );
    }
}
