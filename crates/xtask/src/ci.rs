//! `ci check`: the CI workflows only schedule the verifier, and the primary
//! workflow cannot report green unless every required gate ran and passed.
//!
//! - No workflow step runs a check itself (`cargo test`, `helm lint`, `npx
//!   playwright test`...). Checks live in `capsulet-xtask verify --gate`, so a
//!   developer and CI run the same thing; a command written into a workflow is
//!   a second definition of green that drifts from the first.
//! - Nothing skips a gate silently. A job that runs a gate has no `if:`, and
//!   neither has the step that runs it.
//! - In `ci.yml`, a matrix varies only platforms or toolchains.
//! - Every job in `ci.yml` that runs a gate uploads its run's
//!   `verification-summary.json`.
//! - Every gate of the full profile is run by some job in `ci.yml`.
//! - `ci.yml`'s `required` job waits for every other job, runs even when one of
//!   them failed or was skipped (`if: always()`), and fails unless
//!   `verify summary --require full` finds every gate passed and every job it
//!   waited for successful. It is the one check branch protection requires.

use std::{collections::BTreeSet, fs, path::Path};

use serde_yaml::Value;

use crate::{
    release::split_root,
    verify::catalog::{Profile, gates},
};

const PRIMARY: &str = ".github/workflows/ci.yml";
const WORKFLOWS: &str = ".github/workflows";
const REQUIRED_JOB: &str = "required";
const SUMMARY_FILE: &str = "verification-summary.json";

/// Commands that are checks, and so belong in a gate rather than a workflow.
const CHECKS: &[&str] = &[
    "cargo test",
    "cargo clippy",
    "cargo fmt",
    "cargo audit",
    "cargo deny",
    "cargo nextest",
    "npm test",
    "npm run lint",
    "npm run build",
    "npx tsc",
    "npx eslint",
    "npx playwright test",
    "helm lint",
    "helm template",
    "python -m unittest",
    "pytest",
    "docker compose",
    "kubectl apply",
];

/// What a matrix in the primary workflow may vary.
const MATRIX_KEYS: &[&str] = &[
    "os",
    "platform",
    "runner",
    "toolchain",
    "rust",
    "node",
    "python",
];

const USAGE: &str = "usage: capsulet-xtask ci check [--root <directory>]";

pub(crate) fn run(arguments: &[String]) -> Result<(), String> {
    let (root, arguments) = split_root(arguments)?;
    if arguments != ["check"] {
        return Err(USAGE.to_string());
    }
    let findings = check(&root)?;
    if findings.is_empty() {
        println!(
            "[ci] the workflows only schedule gates, and `required` fails unless every gate of \
             the full profile passed"
        );
        return Ok(());
    }
    Err(format!(
        "{} CI contract violation(s):\n  - {}\nSee docs/development/ci.md.",
        findings.len(),
        findings.join("\n  - ")
    ))
}

fn load(path: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut document: Value =
        serde_yaml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    document
        .apply_merge()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(document)
}

fn jobs(workflow: &Value) -> Vec<(&str, &Value)> {
    workflow["jobs"]
        .as_mapping()
        .into_iter()
        .flatten()
        .filter_map(|(name, job)| Some((name.as_str()?, job)))
        .collect()
}

fn steps(job: &Value) -> Vec<&Value> {
    job["steps"].as_sequence().into_iter().flatten().collect()
}

fn script(step: &Value) -> &str {
    step["run"].as_str().unwrap_or_default()
}

/// The gates a script runs: every `--gate <name>`.
fn gates_run(script: &str) -> Vec<String> {
    let words: Vec<&str> = script.split_whitespace().collect();
    words
        .windows(2)
        .filter(|pair| pair[0] == "--gate")
        .map(|pair| pair[1].trim_matches(['"', '\'']).to_string())
        .collect()
}

pub(crate) fn check(root: &Path) -> Result<Vec<String>, String> {
    let mut findings = Vec::new();
    let mut paths: Vec<_> = fs::read_dir(root.join(WORKFLOWS))
        .map_err(|error| format!("{WORKFLOWS}: {error}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "yml" || extension == "yaml")
        })
        .collect();
    paths.sort();
    for path in &paths {
        let name = format!(
            "{WORKFLOWS}/{}",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        let workflow = load(path)?;
        for (job_name, job) in jobs(&workflow) {
            no_ad_hoc_checks(&name, job_name, job, &mut findings);
            no_silent_skips(&name, job_name, job, &mut findings);
        }
    }
    primary(root, &mut findings)?;
    Ok(findings)
}

fn no_ad_hoc_checks(path: &str, name: &str, job: &Value, findings: &mut Vec<String>) {
    for step in steps(job) {
        for line in script(step).lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            if let Some(check) = CHECKS.iter().find(|check| line.contains(*check)) {
                findings.push(format!(
                    "{path}: job {name} runs `{check}` itself; put it in a verify gate and run \
                     that, so CI and a developer run one definition"
                ));
            }
        }
    }
}

fn no_silent_skips(path: &str, name: &str, job: &Value, findings: &mut Vec<String>) {
    let runs_gate = steps(job)
        .iter()
        .any(|step| !gates_run(script(step)).is_empty());
    if !runs_gate {
        return;
    }
    if let Some(condition) = job["if"].as_str() {
        findings.push(format!(
            "{path}: job {name} runs a gate only `if: {condition}`; a skipped job reads as \
             green, so a gate must run unconditionally"
        ));
    }
    for step in steps(job) {
        if gates_run(script(step)).is_empty() {
            continue;
        }
        if let Some(condition) = step["if"].as_str() {
            findings.push(format!(
                "{path}: job {name} runs a gate in a step only `if: {condition}`"
            ));
        }
    }
}

fn primary(root: &Path, findings: &mut Vec<String>) -> Result<(), String> {
    let workflow = load(&root.join(PRIMARY))?;
    let jobs = jobs(&workflow);
    let mut gates_in_ci = BTreeSet::new();
    for (name, job) in &jobs {
        if let Some(matrix) = job["strategy"]["matrix"].as_mapping() {
            for key in matrix.keys().filter_map(Value::as_str) {
                if key != "include" && key != "exclude" && !MATRIX_KEYS.contains(&key) {
                    findings.push(format!(
                        "{PRIMARY}: job {name} has a matrix over `{key}`; a matrix here varies \
                         platforms or toolchains, and each gate is named in its job"
                    ));
                }
            }
        }
        let gates: Vec<String> = steps(job)
            .iter()
            .flat_map(|step| gates_run(script(step)))
            .collect();
        if gates.is_empty() {
            continue;
        }
        gates_in_ci.extend(gates);
        let uploads = steps(job).iter().any(|step| {
            step["uses"]
                .as_str()
                .is_some_and(|uses| uses.starts_with("actions/upload-artifact@"))
                && step["with"]["path"]
                    .as_str()
                    .is_some_and(|path| path.contains(SUMMARY_FILE))
                && step["if"]
                    .as_str()
                    .is_some_and(|condition| condition.contains("always()"))
        });
        if !uploads {
            findings.push(format!(
                "{PRIMARY}: job {name} runs gates but does not upload its {SUMMARY_FILE} \
                 `if: always()`, so `required` cannot see what it did"
            ));
        }
    }
    for gate in gates() {
        if gate.profiles.contains(&Profile::Full) && !gates_in_ci.contains(gate.name) {
            findings.push(format!(
                "{PRIMARY}: no job runs the {} gate, which the full profile requires",
                gate.name
            ));
        }
    }
    let Some((_, required)) = jobs.iter().find(|(name, _)| *name == REQUIRED_JOB) else {
        findings.push(format!(
            "{PRIMARY}: has no `{REQUIRED_JOB}` job, so nothing fails when a gate is missing"
        ));
        return Ok(());
    };
    let needs: BTreeSet<&str> = match &required["needs"] {
        Value::String(job) => BTreeSet::from([job.as_str()]),
        Value::Sequence(jobs) => jobs.iter().filter_map(Value::as_str).collect(),
        _ => BTreeSet::new(),
    };
    for (name, _) in &jobs {
        if *name != REQUIRED_JOB && !needs.contains(name) {
            findings.push(format!(
                "{PRIMARY}: `{REQUIRED_JOB}` does not wait for job {name}, so it can pass \
                 without it"
            ));
        }
    }
    if !required["if"]
        .as_str()
        .is_some_and(|condition| condition.contains("always()"))
    {
        findings.push(format!(
            "{PRIMARY}: `{REQUIRED_JOB}` lacks `if: always()`; when a job it needs fails, it is \
             skipped, and a skipped required check reads as passing"
        ));
    }
    let checks_summary = steps(required).iter().any(|step| {
        let script = script(step);
        script.contains("verify summary")
            && script.contains("--require full")
            && script.contains("--needs")
    });
    if !checks_summary {
        findings.push(format!(
            "{PRIMARY}: `{REQUIRED_JOB}` does not run `verify summary --require full --needs \
             ...`, so a green job can stand for a gate that never ran"
        ));
    }
    Ok(())
}
