//! `ci check` on a copy of this repository's workflows. Each test breaks one
//! rule and requires the check to name it.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const CI: &str = ".github/workflows/ci.yml";

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct Fixture {
    directory: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("temporary directory");
        let workflows = directory.path().join(".github/workflows");
        fs::create_dir_all(&workflows).expect("directory");
        for entry in fs::read_dir(repository().join(".github/workflows")).expect("workflows") {
            let entry = entry.expect("workflow");
            fs::copy(entry.path(), workflows.join(entry.file_name())).expect("copy");
        }
        Self { directory }
    }

    fn edit(&self, file: &str, from: &str, to: &str) {
        let path = self.directory.path().join(file);
        let text = fs::read_to_string(&path).expect("workflow");
        assert!(text.contains(from), "{file} has no {from:?}");
        fs::write(path, text.replacen(from, to, 1)).expect("write");
    }

    fn check(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_capsulet-xtask"))
            .args(["ci", "check", "--root"])
            .arg(self.directory.path())
            .output()
            .expect("xtask runs")
    }

    fn refuses(&self, expected: &[&str]) {
        let output = self.check();
        let report = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "the check passed");
        for text in expected {
            assert!(
                report.contains(text),
                "the check did not say {text:?}:\n{report}"
            );
        }
    }
}

#[test]
fn the_workflows_as_checked_in_hold_to_the_contract() {
    let output = Fixture::new().check();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn a_check_run_by_hand_in_a_workflow_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        CI,
        "        run: cargo run -p capsulet-xtask --locked -- verify --gate dashboard\n",
        "        run: cargo run -p capsulet-xtask --locked -- verify --gate dashboard\n\n      - name: Quick tests\n        run: cargo test --workspace\n",
    );
    fixture.refuses(&["job dashboard runs `cargo test` itself"]);
}

#[test]
fn a_gate_job_that_can_be_skipped_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        CI,
        "  helm:\n    name: Helm\n    runs-on: ubuntu-latest\n",
        "  helm:\n    name: Helm\n    if: github.event_name == 'push'\n    runs-on: ubuntu-latest\n",
    );
    fixture.refuses(&["job helm runs a gate only `if: github.event_name == 'push'`"]);
}

#[test]
fn a_gate_step_that_can_be_skipped_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        CI,
        "      - name: Verify\n        run: cargo run -p capsulet-xtask --locked -- verify --gate dashboard\n",
        "      - name: Verify\n        if: github.ref == 'refs/heads/main'\n        run: cargo run -p capsulet-xtask --locked -- verify --gate dashboard\n",
    );
    fixture.refuses(&[
        "job dashboard runs a gate in a step only `if: github.ref == 'refs/heads/main'`",
    ]);
}

#[test]
fn a_required_gate_no_job_runs_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        CI,
        "run: cargo run -p capsulet-xtask --locked -- verify --gate kind",
        "run: cargo run -p capsulet-xtask --locked -- verify --gate helm",
    );
    fixture.refuses(&["no job runs the kind gate, which the full profile requires"]);
}

#[test]
fn a_gate_job_that_hides_its_summary_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        CI,
        "          name: verification-dashboard\n          path: .capsulet/verify/*/verification-summary.json",
        "          name: verification-dashboard\n          path: .capsulet/verify/*/dashboard.log",
    );
    fixture
        .refuses(&["job dashboard runs gates but does not upload its verification-summary.json"]);
}

#[test]
fn a_required_job_that_can_pass_without_a_job_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(CI, "      - kind\n", "");
    fixture.refuses(&["`required` does not wait for job kind"]);
}

#[test]
fn a_required_job_that_is_skipped_when_a_job_fails_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(CI, "    if: always()\n    needs:\n", "    needs:\n");
    fixture.refuses(&["`required` lacks `if: always()`"]);
}

#[test]
fn a_required_job_that_does_not_check_the_gates_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        CI,
        "verify summary --require full",
        "verify --list --require full",
    );
    fixture.refuses(&["`required` does not run `verify summary --require full --needs"]);
}

#[test]
fn a_matrix_over_gates_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        CI,
        "  dashboard:\n    name: Dashboard\n    runs-on: ubuntu-latest\n",
        "  dashboard:\n    name: Dashboard\n    runs-on: ubuntu-latest\n    strategy:\n      matrix:\n        gate: [dashboard, sdk]\n",
    );
    fixture.refuses(&["job dashboard has a matrix over `gate`"]);
}
