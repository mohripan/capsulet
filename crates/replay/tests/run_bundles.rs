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

/// Each fixture's decisions, made again from its own log.
#[test]
fn every_decision_in_every_fixture_is_the_one_the_runtime_makes_again() {
    for name in ["succeeded", "failed", "interrupted", "budget-exhausted"] {
        let stdout = replay_fixture(name);
        assert!(
            stdout.contains("and is one the runtime would have written"),
            "{name}: {stdout}"
        );
    }
}

#[test]
fn a_log_that_skips_a_node_the_runtime_would_have_run_is_a_disagreement() {
    // Sealed properly and still folding: the `check` node's two events are cut
    // out and the rest renumbered. Folding cannot tell; deciding again can.
    let mut bundle = sealed("succeeded").open().expect("the fixture opens");
    bundle.events.retain(|recorded| {
        !matches!(
            &recorded.event,
            capsulet_runtime::RunEvent::NodeStarted { node }
                | capsulet_runtime::RunEvent::NodeFinished { node, .. }
                if node.as_str() == "check"
        )
    });
    for (position, recorded) in bundle.events.iter_mut().enumerate() {
        recorded.position = position as u64;
    }
    let path = written("skipped", &bundle.seal().expect("it seals"));
    let output = replay(&path);
    let _ = std::fs::remove_file(&path);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains("folding to `completed`"), "{stdout}");
    assert!(
        stdout.contains(
            "event 13 records node_started `agent-continue`, but deciding again from the log \
             before it gives: start `check`"
        ),
        "{stdout}"
    );
}

#[test]
fn a_loop_stopped_for_a_reason_the_runtime_would_not_give_is_a_disagreement() {
    let bundle = sealed("succeeded").open().expect("the fixture opens");
    let body = serde_json::to_string(&bundle).expect("the bundle encodes");
    let changed = body.replacen(
        r#""reason":{"kind":"condition_false"}"#,
        r#""reason":{"kind":"budget_exhausted","budget":"iterations"}"#,
        1,
    );
    assert_ne!(body, changed, "the fixture's loop stops on its condition");
    let bundle: capsulet_runtime::bundle::RunBundle =
        serde_json::from_str(&changed).expect("still a bundle");
    let path = written("restopped", &bundle.seal().expect("it seals"));
    let output = replay(&path);
    let _ = std::fs::remove_file(&path);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(1), "{stdout}");
    assert!(
        stdout.contains("records loop_stopped `agent-loop`"),
        "{stdout}"
    );
}

/// Writes `name`'s page with `--html` and returns it, after the text report.
fn page_of(bundle_path: &Path, name: &str) -> (Output, String) {
    let page = std::env::temp_dir().join(format!(
        "capsulet-replay-page-{name}-{}.html",
        std::process::id()
    ));
    let output = Command::new(env!("CARGO_BIN_EXE_capsulet-replay"))
        .arg(bundle_path)
        .arg("--html")
        .arg(&page)
        .env_clear()
        .output()
        .expect("the replay binary runs");
    let html = std::fs::read_to_string(&page).unwrap_or_default();
    let _ = std::fs::remove_file(&page);
    (output, html)
}

#[test]
fn a_run_bundle_opens_as_one_page_that_walks_run_node_action_evidence_and_verification() {
    let bundle = sealed("succeeded").open().expect("the fixture opens");
    let (output, html) = page_of(&fixture("succeeded"), "succeeded");

    assert!(output.status.success(), "{output:?}");
    assert!(html.starts_with("<!doctype html>"), "{html}");
    // The run, and what its verification amounted to.
    assert!(html.contains(&bundle.run_id), "{html}");
    assert!(html.contains("Admitted in observe mode"), "{html}");
    // Each node, with what it was observed to do.
    for node in ["normalize", "prompt", "model", "check"] {
        assert!(
            html.contains(&format!("<summary><code>{node}</code>")),
            "{node}: {html}"
        );
    }
    // The evidence those events name, by digest.
    for recorded in &bundle.events {
        if let capsulet_runtime::RunEvent::NodeFinished { outputs, .. } = &recorded.event {
            for digest in outputs.values() {
                assert!(html.contains(&digest.to_string()), "{digest}");
            }
        }
    }
    // Every event once in the log, each marked with what the replay made of it.
    for position in 0..bundle.events.len() {
        assert_eq!(
            html.matches(&format!("id=\"event-{position}\"")).count(),
            1,
            "event {position}"
        );
    }
    assert!(html.contains("role decision"), "{html}");
    assert!(!html.contains(
        "<li id=\"event-0\"><span class=\"position\">0</span> <span class=\"role divergent\""
    ));
    // Nothing on it reaches anywhere else.
    assert!(!html.contains("<script"), "{html}");
    assert!(
        !html.contains("http://") && !html.contains("https://"),
        "{html}"
    );
}

#[test]
fn a_page_for_a_log_the_runtime_would_not_have_written_shows_where() {
    let mut bundle = sealed("succeeded").open().expect("the fixture opens");
    bundle.events.retain(|recorded| {
        !matches!(
            &recorded.event,
            capsulet_runtime::RunEvent::NodeStarted { node }
                | capsulet_runtime::RunEvent::NodeFinished { node, .. }
                if node.as_str() == "check"
        )
    });
    for (position, recorded) in bundle.events.iter_mut().enumerate() {
        recorded.position = position as u64;
    }
    let path = written("skipped-page", &bundle.seal().expect("it seals"));
    let (output, html) = page_of(&path, "skipped");
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        html.contains("<span class=\"bad\">disagrees</span>"),
        "{html}"
    );
    assert!(
        html.contains(
            "<li id=\"event-13\"><span class=\"position\">13</span> <span class=\"role divergent\">"
        ),
        "{html}"
    );
    assert!(html.contains("href=\"#event-13\">Event 13</a>"), "{html}");
}
