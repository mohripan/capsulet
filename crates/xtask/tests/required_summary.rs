//! `verify summary --require full`, which CI's `Required` job runs, against
//! summaries made here. A complete, clean set passes. Removing the evidence of
//! any one coverage area, or a job that did not succeed, or a run that left
//! something behind or changed the worktree, must fail it and say which.

use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use serde_json::{Value, json};

fn xtask() -> Command {
    Command::new(env!("CARGO_BIN_EXE_capsulet-xtask"))
}

/// Every gate of the full profile, from the verifier itself.
fn full_gates() -> Vec<String> {
    let output = xtask()
        .args(["verify", "--list", "--format", "json"])
        .output()
        .expect("xtask runs");
    assert!(output.status.success());
    let graph: Value = serde_json::from_slice(&output.stdout).expect("gate graph");
    graph
        .as_array()
        .expect("gates")
        .iter()
        .filter(|gate| {
            gate["profiles"]
                .as_array()
                .is_some_and(|profiles| profiles.iter().any(|profile| profile == "full"))
        })
        .map(|gate| gate["name"].as_str().expect("name").to_string())
        .collect()
}

fn gate(name: &str, status: &str) -> Value {
    json!({
        "name": name,
        "status": status,
        "duration_seconds": 1.0,
        "commands": [],
        "tools": {},
        "tests": {"binaries": 1, "passed": 3, "failed": 0, "ignored": 0},
        "log": {"path": format!("{name}.log"), "sha256": "0".repeat(64)},
    })
}

fn run(id: &str) -> Value {
    json!({
        "id": id,
        "revision": "0".repeat(40),
        "profile": null,
        "host": "linux-x86_64",
        "started_unix": 1_700_000_000_u64,
        "finished_unix": 1_700_000_100_u64,
        "clean_at_start": true,
        "worktree_changes": [],
        "cleanup_checked": true,
        "cleanup_scope": ["docker", "kind", "files"],
        "leftovers": [],
    })
}

/// One job per gate, as `verification-<job>/<run>/verification-summary.json`,
/// and the `needs` context of a `Required` job that waited for all of them.
struct Jobs {
    directory: tempfile::TempDir,
    needs: serde_json::Map<String, Value>,
}

impl Jobs {
    fn all_passing() -> Self {
        let directory = tempfile::tempdir().expect("temp directory");
        let mut jobs = Self {
            directory,
            needs: serde_json::Map::new(),
        };
        for (index, name) in full_gates().iter().enumerate() {
            jobs.write(
                name,
                &json!({
                    "schema_version": 2,
                    "verifier": "0.1.0",
                    "run": run(&format!("1700000000000-{index}")),
                    "gates": [gate(name, "passed")],
                }),
            );
        }
        jobs
    }

    fn path(&self, job: &str) -> std::path::PathBuf {
        self.directory
            .path()
            .join(format!("verification-{job}"))
            .join("run")
            .join("verification-summary.json")
    }

    fn write(&mut self, job: &str, summary: &Value) {
        let path = self.path(job);
        fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        fs::write(&path, serde_json::to_string_pretty(summary).expect("json")).expect("write");
        self.needs
            .insert(job.to_string(), json!({"result": "success", "outputs": {}}));
    }

    fn edit(&self, job: &str, change: impl FnOnce(&mut Value)) {
        let path = self.path(job);
        let mut summary: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("summary");
        change(&mut summary);
        fs::write(&path, serde_json::to_string(&summary).expect("json")).expect("write");
    }

    /// The job ran, but its upload is gone: as if its gate never ran.
    fn remove(&self, job: &str) {
        fs::remove_dir_all(self.path(job).parent().and_then(Path::parent).expect("job"))
            .expect("remove");
    }

    fn check(&self) -> Output {
        xtask()
            .args(["verify", "summary", "--require", "full", "--needs"])
            .arg(Value::Object(self.needs.clone()).to_string())
            .arg(self.directory.path())
            .env_remove("GITHUB_STEP_SUMMARY")
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

#[test]
fn every_gate_of_the_full_profile_passing_in_clean_runs_passes() {
    let output = Jobs::all_passing().check();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("required gates ran and passed"),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

/// The coverage M1 requires `Required` to notice the absence of.
#[test]
fn removing_the_evidence_of_any_required_coverage_is_refused() {
    for coverage in [
        "postgres",
        "migrations",
        "chaos",
        "authorization",
        "api-contracts",
        "security",
        "secrets",
        "compose",
        "helm",
        "kind",
    ] {
        let jobs = Jobs::all_passing();
        jobs.remove(coverage);
        jobs.refuses(&format!("{coverage} did not run in any job"));
    }
}

#[test]
fn a_gate_that_did_not_pass_is_refused_however_it_ended() {
    for (status, said) in [
        ("failed", "kind failed"),
        ("timed_out", "kind timed out"),
        ("error", "kind error"),
        ("missing_prerequisite", "kind missing prerequisite"),
    ] {
        let jobs = Jobs::all_passing();
        jobs.edit("kind", |summary| {
            summary["gates"][0]["status"] = json!(status);
        });
        jobs.refuses(said);
    }
}

#[test]
fn a_job_that_was_skipped_cancelled_or_failed_is_refused() {
    for result in ["skipped", "cancelled", "failure"] {
        let mut jobs = Jobs::all_passing();
        jobs.needs
            .insert("helm".to_string(), json!({"result": result, "outputs": {}}));
        jobs.refuses(&format!("job helm ended {result}"));
    }
}

#[test]
fn a_run_that_left_resources_or_changed_the_worktree_is_refused() {
    let jobs = Jobs::all_passing();
    jobs.edit("postgres", |summary| {
        summary["run"]["leftovers"] = json!(["volume capsulet-verify-1700000000000-12-compose_db"]);
    });
    jobs.refuses("left behind volume capsulet-verify-1700000000000-12-compose_db");

    let jobs = Jobs::all_passing();
    jobs.edit("dashboard", |summary| {
        summary["run"]["worktree_changes"] = json!(["dashboard/next-env.d.ts"]);
    });
    jobs.refuses("changed the worktree: dashboard/next-env.d.ts");

    let jobs = Jobs::all_passing();
    jobs.edit("kind", |summary| {
        summary["run"]["cleanup_checked"] = json!(false);
    });
    jobs.refuses("stopped before it checked what it left behind");
}

#[test]
fn a_passing_gate_whose_tests_failed_is_refused() {
    let jobs = Jobs::all_passing();
    jobs.edit("postgres", |summary| {
        summary["gates"][0]["tests"]["failed"] = json!(1);
    });
    jobs.refuses("postgres reported 1 failed test(s) although it passed");
}
