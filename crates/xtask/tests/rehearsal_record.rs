//! `verify record`: only a full-profile run of a clean checkout that passed
//! every gate, changed nothing and left nothing becomes a rehearsal record.

use std::{fs, path::Path, process::Command};

use serde_json::{Value, json};

fn xtask() -> Command {
    Command::new(env!("CARGO_BIN_EXE_capsulet-xtask"))
}

fn full_gates() -> Vec<String> {
    let output = xtask()
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

/// A run directory as `verify --profile full` leaves it.
fn rehearsal(directory: &Path) -> Value {
    let gates: Vec<Value> = full_gates()
        .iter()
        .map(|name| {
            json!({
                "name": name,
                "status": "passed",
                "duration_seconds": 2.5,
                "commands": [],
                "tools": {"cargo": "cargo 1.96.0"},
                "tests": {"binaries": 1, "passed": 4, "failed": 0, "ignored": 0},
                "log": {"path": format!("{name}.log"), "sha256": "a".repeat(64)},
            })
        })
        .collect();
    let summary = json!({
        "schema_version": 2,
        "verifier": "0.1.0",
        "run": {
            "id": "1700000000000-7",
            "revision": "b".repeat(40),
            "profile": "full",
            "host": "linux-x86_64",
            "started_unix": 1_700_000_000_u64,
            "finished_unix": 1_700_003_600_u64,
            "clean_at_start": true,
            "worktree_changes": [],
            "cleanup_checked": true,
            "cleanup_scope": ["docker", "kind", "files"],
            "leftovers": [],
        },
        "gates": gates,
    });
    let image = |id: &str| {
        json!({
            "component": "api",
            "image": "ghcr.io/mohripan/capsulet-api",
            "version": "0.1.0",
            "revision": "b".repeat(40),
            "reference": "capsulet-verify/capsulet-api:0.1.0-1700000000000-7-1",
            "id": id,
        })
    };
    fs::write(
        directory.join("images.jsonl"),
        format!(
            "{}\n{}\n",
            image(&format!("sha256:{}", "c".repeat(64))),
            image(&format!("sha256:{}", "c".repeat(64)))
        ),
    )
    .expect("images");
    fs::write(
        directory.join("chart-render.json"),
        json!({"sha256": format!("sha256:{}", "d".repeat(64))}).to_string(),
    )
    .expect("chart");
    summary
}

fn record(directory: &Path, summary: &Value) -> std::process::Output {
    fs::write(
        directory.join("verification-summary.json"),
        serde_json::to_string_pretty(summary).expect("json"),
    )
    .expect("summary");
    xtask()
        .args(["verify", "record"])
        .arg(directory)
        .arg("--output")
        .arg(directory.join("record.json"))
        .output()
        .expect("xtask runs")
}

#[test]
fn a_clean_full_run_is_recorded_with_its_evidence() {
    let directory = tempfile::tempdir().expect("temp directory");
    let summary = rehearsal(directory.path());
    let output = record(directory.path(), &summary);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let record: Value = serde_json::from_str(
        &fs::read_to_string(directory.path().join("record.json")).expect("record"),
    )
    .expect("record JSON");
    let gates = full_gates().len();
    assert_eq!(record["revision"], "b".repeat(40));
    assert_eq!(record["duration_seconds"], 3600);
    assert_eq!(record["totals"]["gates_passed"], gates);
    assert_eq!(record["totals"]["tests_passed"], 4 * gates);
    assert_eq!(record["tools"]["cargo"], "cargo 1.96.0");
    assert_eq!(
        record["images"].as_array().map(Vec::len),
        Some(1),
        "one image, once"
    );
    assert_eq!(
        record["chart_render_sha256"],
        format!("sha256:{}", "d".repeat(64))
    );
    assert!(
        record["verification_summary_sha256"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:") && digest.len() == 71)
    );
    assert!(
        !fs::read_to_string(directory.path().join("record.json"))
            .expect("record")
            .contains(".log\""),
        "the record names no log path"
    );
}

/// A name, how it spoils the run, and what the refusal says.
type Case = (&'static str, fn(&mut Value), &'static str);

#[test]
fn a_run_that_is_not_a_clean_full_rehearsal_is_refused() {
    let cases: [Case; 5] = [
        (
            "profile",
            |s| s["run"]["profile"] = json!("fast"),
            "did not select --profile full",
        ),
        (
            "dirty",
            |s| s["run"]["clean_at_start"] = json!(false),
            "was not clean when the run started",
        ),
        (
            "failed",
            |s| s["gates"][0]["status"] = json!("failed"),
            "failed",
        ),
        (
            "missing",
            |s| {
                s["gates"].as_array_mut().expect("gates").pop();
            },
            "did not run in any job",
        ),
        (
            "leftover",
            |s| s["run"]["leftovers"] = json!(["Kind cluster capsulet-verify-1700000000000-7"]),
            "left behind Kind cluster",
        ),
    ];
    for (case, change, said) in cases {
        let directory = tempfile::tempdir().expect("temp directory");
        let mut summary = rehearsal(directory.path());
        change(&mut summary);
        let output = record(directory.path(), &summary);
        let report = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{case}: recorded");
        assert!(
            report.contains(said),
            "{case}: did not say {said:?}:\n{report}"
        );
        assert!(
            !directory.path().join("record.json").exists(),
            "{case}: wrote a record"
        );
    }
}
