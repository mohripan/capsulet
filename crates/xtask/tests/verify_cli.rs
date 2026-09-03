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

/// A prerequisite probe that guesses `--version` for every tool reports
/// installed tools as missing. `helm` and `kubectl` reject that flag outright,
/// so the doctor would tell an operator to install what they already have and
/// the full profile would stop at a gate that could have run.
#[test]
fn version_probes_use_the_flag_each_tool_actually_accepts() {
    let output = output(&["verify", "doctor"]);
    let report = String::from_utf8_lossy(&output.stdout).into_owned() + &stderr(&output);

    for tool in ["helm", "kubectl"] {
        if which(tool) {
            assert!(
                report.contains(&format!("ok      {tool}")),
                "`{tool}` is installed, so the doctor must not report it missing:
{report}"
            );
        }
    }
}

/// Whether a tool is on PATH, checked without assuming any version flag.
fn which(tool: &str) -> bool {
    let finder = if cfg!(windows) { "where" } else { "which" };
    Command::new(finder)
        .arg(tool)
        .output()
        .is_ok_and(|found| found.status.success())
}

/// Security exceptions must be owned and must expire.
///
/// An advisory silenced in `.cargo/audit.toml` with no entry in the exceptions
/// document is an exception nobody owns; one whose review date has passed is
/// permanent by accident. Both fail here rather than in an audit six months
/// from now.
#[test]
fn every_silenced_advisory_is_owned_and_unexpired() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let audit = std::fs::read_to_string(root.join(".cargo/audit.toml")).unwrap_or_default();
    let document = std::fs::read_to_string(root.join("docs/contracts/security-exceptions.md"))
        .expect("the exceptions document is readable");

    for advisory in silenced(&audit) {
        let row = document
            .lines()
            .find(|line| line.contains(&format!("`{advisory}`")) && line.starts_with('|'))
            .unwrap_or_else(|| {
                panic!("`{advisory}` is silenced but not documented as an owned exception")
            });

        let review = review_date(row)
            .unwrap_or_else(|| panic!("the exception for `{advisory}` records no review date"));
        assert!(
            review.as_str() > &today(),
            "the exception for `{advisory}` was due for review on {review}"
        );
    }
}

/// Advisory identifiers listed in the audit configuration's ignore list.
fn silenced(audit: &str) -> Vec<String> {
    audit
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .flat_map(|line| {
            line.match_indices("RUSTSEC-")
                .map(|(at, _)| {
                    line[at..]
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The `YYYY-MM-DD` cell of a table row.
fn review_date(row: &str) -> Option<String> {
    row.split('|')
        .map(str::trim)
        .find(|cell| {
            cell.len() == 10
                && cell.as_bytes()[4] == b'-'
                && cell.as_bytes()[7] == b'-'
                && cell.chars().filter(char::is_ascii_digit).count() == 8
        })
        .map(str::to_string)
}

/// Today, as `YYYY-MM-DD`, without pulling in a date library.
fn today() -> String {
    let output = Command::new(if cfg!(windows) { "powershell" } else { "date" })
        .args(if cfg!(windows) {
            vec![
                "-NoProfile",
                "-Command",
                "(Get-Date).ToString('yyyy-MM-dd')",
            ]
        } else {
            vec!["+%Y-%m-%d"]
        })
        .output()
        .expect("the date is readable");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}
