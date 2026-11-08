//! Run bundles, read with nothing but the bundle.
//!
//! The fixtures are bundles exported from real runs by
//! `crates/graph-worker/tests/agent_runs.rs` with `CAPSULET_WRITE_BUNDLE_FIXTURES`
//! set: one that succeeded, one that failed, one interrupted mid-call, and one that
//! ran out of budget. Each is diagnosed here with no database, no network, and an
//! empty environment.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use capsulet_runtime::bundle::SealedRunBundle;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("run-{name}.json"))
}

fn replay(path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_capsulet-replay"))
        .arg(path)
        // Nothing inherited: no API URL, no token, no proxy, no database.
        .env_clear()
        .output()
        .expect("the replay binary runs")
}

fn replay_fixture(name: &str) -> String {
    let output = replay(&fixture(name));
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "`{name}` should agree with its log, got {:?}\n{stdout}{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("agrees: the log supports what was reported"),
        "{stdout}"
    );
    assert!(
        stdout.contains("reproduced here: the fold of every event"),
        "{stdout}"
    );
    stdout
}

fn sealed(name: &str) -> SealedRunBundle {
    let text = std::fs::read_to_string(fixture(name)).expect("the fixture is readable");
    serde_json::from_str(&text).expect("the fixture is a sealed bundle")
}

fn written(name: &str, bundle: &SealedRunBundle) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "capsulet-replay-run-{name}-{}.json",
        std::process::id()
    ));
    std::fs::write(
        &path,
        serde_json::to_string(bundle).expect("the bundle encodes"),
    )
    .expect("the bundle is writable");
    path
}

#[test]
fn a_run_that_succeeded_is_explained_offline() {
    let stdout = replay_fixture("succeeded");
    assert!(stdout.contains("folding to `completed`"), "{stdout}");
    assert!(
        stdout.contains("model (llm): 1 started, 1 finished"),
        "{stdout}"
    );
    assert!(
        stdout.contains("needing a model, which can answer differently each time: model"),
        "{stdout}"
    );
}

#[test]
fn a_run_that_failed_says_why_offline() {
    let stdout = replay_fixture("failed");
    assert!(stdout.contains("folding to `failed`"), "{stdout}");
    assert!(
        stdout.contains("ended because: node `model` failed with verifier_unavailable"),
        "{stdout}"
    );
}

#[test]
fn a_run_interrupted_mid_call_says_what_it_was_doing() {
    let stdout = replay_fixture("interrupted");
    assert!(stdout.contains("folding to `running`"), "{stdout}");
    assert!(
        stdout.contains("interrupted while running: model"),
        "{stdout}"
    );
}

#[test]
fn a_run_that_ran_out_of_budget_says_which_budget() {
    let stdout = replay_fixture("budget-exhausted");
    assert!(
        stdout.contains("ended because: the iterations budget ran out"),
        "{stdout}"
    );
    assert!(
        stdout.contains("model (llm): 2 started, 2 finished"),
        "{stdout}"
    );
}

#[test]
fn a_bundle_whose_body_was_changed_is_not_believed() {
    let mut bundle = sealed("succeeded");
    bundle.body = bundle.body.replacen("completed", "cancelled", 1);
    let path = written("tampered", &bundle);
    let output = replay(&path);
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("does not match its digest"),
        "{output:?}"
    );
}

#[test]
fn a_report_the_log_does_not_support_is_a_disagreement() {
    // Sealed properly, so the digest checks, but it says the failed run completed.
    let mut bundle = sealed("failed").open().expect("the fixture opens");
    bundle.reported.status = "completed".to_string();
    let path = written("misreported", &bundle.seal().expect("it seals"));
    let output = replay(&path);
    let _ = std::fs::remove_file(&path);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(1), "{stdout}");
    assert!(
        stdout.contains("reported `completed`, and the log folds to `failed`"),
        "{stdout}"
    );
}
