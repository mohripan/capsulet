//! `completion check` on a copy of the plan, the M1 completion report and a
//! rehearsal record, in a Git repository of its own. Each test breaks one rule
//! and requires the check to name it.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use serde_json::{Value, json};

const PLAN: &str = "docs/superpowers/plans/2026-08-31-engineering-integrity.md";
const REPORT: &str = "docs/contracts/m1-completion.md";
const RECORD: &str = "docs/contracts/clean-checkout-rehearsal.json";

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn git(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .output()
        .expect("git runs");
    assert!(output.status.success(), "git {arguments:?}");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn full_gates() -> Vec<String> {
    let output = Command::new(env!("CARGO_BIN_EXE_capsulet-xtask"))
        .args(["verify", "--list", "--format", "json"])
        .output()
        .expect("xtask runs");
    let graph: Value = serde_json::from_slice(&output.stdout).expect("gate graph");
    graph
        .as_array()
        .expect("gates")
        .iter()
        .filter(|gate| {
            gate["profiles"]
                .as_array()
                .is_some_and(|p| p.iter().any(|p| p == "full"))
        })
        .map(|gate| gate["name"].as_str().expect("name").to_string())
        .collect()
}

/// A record of a clean full run in which every gate of the profile passed.
fn passing_record() -> Value {
    let gates: Vec<Value> = full_gates()
        .iter()
        .map(|name| {
            json!({
                "name": name, "status": "passed", "duration_seconds": 1.0, "test_binaries": 1,
                "tests_passed": 2, "tests_failed": 0, "tests_ignored": 0,
                "log_sha256": "a".repeat(64),
            })
        })
        .collect();
    let count = gates.len();
    json!({
        "schema_version": 1,
        "revision": "",
        "verifier": "0.1.0",
        "profile": "full",
        "host": "linux-x86_64",
        "started_unix": 1_700_000_000_u64,
        "finished_unix": 1_700_003_600_u64,
        "duration_seconds": 3600,
        "clean_at_start": true,
        "worktree_changes": [],
        "cleanup_scope": ["docker", "kind", "files"],
        "leftovers": [],
        "tools": {"cargo": "cargo 1.96.0"},
        "gates": gates,
        "totals": {
            "gates": count, "gates_passed": count, "test_binaries": count,
            "tests_passed": 2 * count, "tests_failed": 0, "tests_ignored": 0,
        },
        "images": [{
            "component": "api", "image": "ghcr.io/mohripan/capsulet-api", "version": "0.1.0",
            "revision": "b".repeat(40), "id": format!("sha256:{}", "c".repeat(64)),
        }],
        "chart_render_sha256": format!("sha256:{}", "d".repeat(64)),
        "verification_summary_sha256": format!("sha256:{}", "e".repeat(64)),
    })
}

struct Fixture {
    directory: tempfile::TempDir,
}

impl Fixture {
    /// The plan and the report as checked in, and `record`, committed; the
    /// record is then made to name that commit, as a real one names the commit
    /// it was rehearsed at.
    fn new(record: &Value) -> Self {
        let directory = tempfile::tempdir().expect("temp directory");
        let root = directory.path();
        for path in [PLAN, REPORT] {
            let target = root.join(path);
            fs::create_dir_all(target.parent().expect("parent")).expect("directory");
            fs::copy(repository().join(path), target).expect("copy");
        }
        git(root, &["init", "--quiet"]);
        git(
            root,
            &["config", "user.email", "completion@example.invalid"],
        );
        git(root, &["config", "user.name", "completion"]);
        git(root, &["add", "."]);
        git(root, &["commit", "--quiet", "-m", "rehearsed"]);
        let mut record = record.clone();
        record["revision"] = json!(git(root, &["rev-parse", "HEAD"]));
        let fixture = Self { directory };
        fixture.write_record(&record);
        fixture
    }

    fn write_record(&self, record: &Value) {
        fs::write(
            self.directory.path().join(RECORD),
            serde_json::to_string_pretty(record).expect("json"),
        )
        .expect("record");
    }

    fn record(&self) -> Value {
        serde_json::from_str(&fs::read_to_string(self.directory.path().join(RECORD)).expect("read"))
            .expect("record")
    }

    fn edit_report(&self, from: &str, to: &str) {
        let path = self.directory.path().join(REPORT);
        let text = fs::read_to_string(&path).expect("report");
        assert!(text.contains(from), "the report has no {from:?}");
        fs::write(path, text.replacen(from, to, 1)).expect("write");
    }

    fn check(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_capsulet-xtask"))
            .args(["completion", "check", "--root"])
            .arg(self.directory.path())
            .output()
            .expect("xtask runs")
    }

    fn refuses(&self, expected: &str) {
        let output = self.check();
        let report = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "the check passed");
        assert!(
            report.contains(expected),
            "it did not say {expected:?}:\n{report}"
        );
    }
}

/// The report and the record as checked in. Before either existed, this
/// failed; that is the check M1 had to close.
#[test]
fn the_checked_in_report_and_record_close_m1() {
    let record: Value = serde_json::from_str(
        &fs::read_to_string(repository().join(RECORD)).expect("the rehearsal record"),
    )
    .expect("record");
    let output = Fixture::new(&record).check();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn a_criterion_without_a_row_is_refused() {
    let fixture = Fixture::new(&passing_record());
    fixture.edit_report(
        "| G10 | Helm lint/schema/render",
        "| X10 | Helm lint/schema/render",
    );
    fixture.refuses("has no row for G10: Helm lint/schema/render checks");
}

#[test]
fn a_criterion_that_does_not_read_as_the_plan_does_is_refused() {
    let fixture = Fixture::new(&passing_record());
    fixture.edit_report(
        "| E3 | a required check cannot report success",
        "| E3 | a required check rarely reports success",
    );
    fixture.refuses("E3 reads \"a required check rarely reports success");
}

#[test]
fn a_gate_that_is_not_in_the_full_profile_is_refused() {
    for gate in ["kubernetes", "wasm-python"] {
        let fixture = Fixture::new(&passing_record());
        fixture.edit_report("| `helm`, `kind` |", &format!("| `helm`, `{gate}` |"));
        fixture.refuses(&format!(
            "G10 names `{gate}`, which is not a gate of the full profile"
        ));
    }
}

#[test]
fn a_row_that_does_not_link_the_record_is_refused() {
    let fixture = Fixture::new(&passing_record());
    fixture.edit_report(
        "| `helm`, `kind` | [rehearsal](clean-checkout-rehearsal.json)",
        "| `helm`, `kind` | trust us",
    );
    fixture.refuses("G10 does not link the clean-checkout record");
}

#[test]
fn a_report_without_its_risks_is_refused() {
    let fixture = Fixture::new(&passing_record());
    fixture.edit_report("## Remaining risks", "## Things we noticed");
    fixture.refuses("has no `## Remaining risks` section");
}

#[test]
fn a_record_in_which_a_cited_gate_did_not_pass_is_refused() {
    let fixture = Fixture::new(&passing_record());
    let mut record = fixture.record();
    for gate in record["gates"].as_array_mut().expect("gates") {
        if gate["name"] == "kind" {
            gate["status"] = json!("failed");
        }
    }
    fixture.write_record(&record);
    fixture.refuses("does not show `kind` passing");
}

#[test]
fn a_record_of_a_dirty_or_leaky_run_is_refused() {
    let fixture = Fixture::new(&passing_record());
    let mut record = fixture.record();
    record["leftovers"] = json!(["Kind cluster capsulet-verify-1700000000000-7"]);
    fixture.write_record(&record);
    fixture.refuses("the run left behind Kind cluster");

    let fixture = Fixture::new(&passing_record());
    let mut record = fixture.record();
    record["clean_at_start"] = json!(false);
    fixture.write_record(&record);
    fixture.refuses("is not of a full-profile run of a clean checkout");
}

#[test]
fn a_record_of_a_commit_this_checkout_does_not_descend_from_is_refused() {
    let fixture = Fixture::new(&passing_record());
    let mut record = fixture.record();
    record["revision"] = json!("f".repeat(40));
    fixture.write_record(&record);
    fixture.refuses("which is not a commit this checkout descends from");
}

#[test]
fn a_missing_record_is_refused() {
    let fixture = Fixture::new(&passing_record());
    fs::remove_file(fixture.directory.path().join(RECORD)).expect("remove");
    fixture.refuses("clean-checkout-rehearsal.json");
}
