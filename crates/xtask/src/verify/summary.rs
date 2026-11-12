//! The machine-readable record of a verification run, and the check that a set
//! of runs covered a whole profile.
//!
//! Every run writes `verification-summary.json` beside its gate logs. For each
//! gate it holds the status, the duration, the commands it ran, the version of
//! every tool it needed, the test counts of every binary, and the SHA-256 of
//! its log. CI uploads each job's file. The final `required` job hands them all
//! to `verify summary --require full`, which fails unless every gate of the
//! profile is there and passed. A gate that is missing, skipped, failed or
//! timed out cannot hide behind a green job.
//!
//! Each summary also records its run: the commit, whether the worktree was
//! clean when it started and what changed while it ran, and what the run left
//! behind of the containers, volumes, networks, images, clusters and credential
//! files its gates made. A run that changed the worktree, left anything, or
//! stopped before it could look fails `verify summary` too.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{
    catalog::{Gate, Profile, gates},
    counts::CommandCounts,
    process,
};

pub(crate) const FILE: &str = "verification-summary.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Summary {
    pub(crate) schema_version: u32,
    /// The verifier's own version, which is the product's.
    pub(crate) verifier: String,
    /// Absent only in a summary this check cannot vouch for.
    #[serde(default)]
    pub(crate) run: Option<RunRecord>,
    pub(crate) gates: Vec<GateRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RunRecord {
    /// The run directory's name, `<millis>-<pid>`, which every disposable
    /// resource of the run is named with.
    pub(crate) id: String,
    /// The commit checked out, or `None` outside a Git checkout.
    pub(crate) revision: Option<String>,
    /// `--profile`, when the run selected one.
    pub(crate) profile: Option<String>,
    /// `<os>-<arch>` of the machine.
    pub(crate) host: String,
    pub(crate) started_unix: u64,
    pub(crate) finished_unix: Option<u64>,
    /// `git status` reported nothing when the run started.
    pub(crate) clean_at_start: bool,
    /// What changed in the worktree while the run ran: every path whose
    /// status or contents differ, and a moved `HEAD`.
    pub(crate) worktree_changes: Vec<String>,
    /// Whether the run got as far as looking for what it left behind.
    pub(crate) cleanup_checked: bool,
    /// What it looked at: `docker`, `kind`, `files`.
    pub(crate) cleanup_scope: Vec<String>,
    /// Every resource of this run still there after its last gate.
    pub(crate) leftovers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GateRecord {
    pub(crate) name: String,
    /// `passed`, `failed`, `timed_out`, `error` or `missing_prerequisite`.
    pub(crate) status: String,
    pub(crate) duration_seconds: f64,
    pub(crate) commands: Vec<String>,
    /// What each prerequisite reported when asked for its version.
    pub(crate) tools: BTreeMap<String, String>,
    pub(crate) tests: Tests,
    pub(crate) log: Log,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct Tests {
    pub(crate) binaries: usize,
    pub(crate) passed: u64,
    pub(crate) failed: u64,
    pub(crate) ignored: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Log {
    pub(crate) path: String,
    pub(crate) sha256: Option<String>,
}

impl GateRecord {
    pub(crate) fn new(
        gate: &Gate,
        status: &str,
        duration_seconds: f64,
        commands: &[CommandCounts],
        log_path: &Path,
    ) -> Self {
        let tools = gate
            .prerequisites
            .iter()
            .map(|tool| {
                let report = process::version_report(tool, &process::version_arguments(tool))
                    .and_then(|report| report.lines().next().map(|line| line.trim().to_string()))
                    .unwrap_or_else(|| "unavailable".to_string());
                (tool.clone(), report)
            })
            .collect();
        let binaries = commands.iter().flat_map(|command| &command.binaries);
        let tests = binaries.fold(Tests::default(), |mut tests, binary| {
            tests.binaries += 1;
            tests.passed += binary.passed;
            tests.failed += binary.failed;
            tests.ignored += binary.ignored;
            tests
        });
        let sha256 = fs::read(log_path).ok().map(|bytes| {
            Sha256::digest(&bytes)
                .iter()
                .fold(String::with_capacity(64), |mut hex, byte| {
                    use std::fmt::Write as _;
                    let _ = write!(hex, "{byte:02x}");
                    hex
                })
        });
        Self {
            name: gate.name.to_string(),
            status: status.to_string(),
            duration_seconds: (duration_seconds * 10.0).round() / 10.0,
            commands: gate
                .commands
                .iter()
                .map(crate::claims::render_command)
                .collect(),
            tools,
            tests,
            log: Log {
                path: log_path.display().to_string(),
                sha256,
            },
        }
    }
}

/// Rewrites the run's summary with every gate recorded so far, so a run that
/// stops early still says what ran.
pub(crate) fn write(
    directory: &Path,
    run: &RunRecord,
    records: &[GateRecord],
) -> Result<(), String> {
    let summary = Summary {
        schema_version: 2,
        verifier: env!("CARGO_PKG_VERSION").to_string(),
        run: Some(run.clone()),
        gates: records.to_vec(),
    };
    let path = directory.join(FILE);
    let json = serde_json::to_string_pretty(&summary).map_err(|error| error.to_string())?;
    fs::write(&path, json + "\n")
        .map_err(|error| format!("could not write {}: {error}", path.display()))
}

/// `verify summary --require <profile> [--needs <json>] <file or directory>...`
pub(crate) fn run(arguments: &[String]) -> Result<(), String> {
    let usage = "usage: capsulet-xtask verify summary --require <fast|full> [--needs <json>] \
                 <summary file or directory>...";
    let mut profile = None;
    let mut needs = None;
    let mut paths = Vec::new();
    let mut iterator = arguments.iter();
    while let Some(argument) = iterator.next() {
        match argument.as_str() {
            "--require" => profile = iterator.next().cloned(),
            "--needs" => needs = iterator.next().cloned(),
            path => paths.push(PathBuf::from(path)),
        }
    }
    let profile = match profile.as_deref() {
        Some("fast") => Profile::Fast,
        Some("full") => Profile::Full,
        _ => return Err(usage.to_string()),
    };
    if paths.is_empty() {
        return Err(usage.to_string());
    }
    let mut summaries = Vec::new();
    for path in &paths {
        collect(path, &mut summaries)?;
    }
    let required: Vec<String> = gates()
        .into_iter()
        .filter(|gate| gate.profiles.contains(&profile))
        .map(|gate| gate.name.to_string())
        .collect();
    let mut findings = check(&required, &summaries);
    if let Some(needs) = needs {
        findings.extend(check_needs(&needs)?);
    }
    publish(&required, &summaries, &findings);
    if findings.is_empty() {
        println!(
            "[verify] every one of the {} required gates ran and passed, across {} summaries",
            required.len(),
            summaries.len()
        );
        Ok(())
    } else {
        Err(format!(
            "the required gates did not all pass:\n  - {}",
            findings.join("\n  - ")
        ))
    }
}

fn collect(path: &Path, summaries: &mut Vec<(String, Summary)>) -> Result<(), String> {
    if path.is_dir() {
        let mut entries: Vec<PathBuf> = fs::read_dir(path)
            .map_err(|error| format!("{}: {error}", path.display()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for entry in entries {
            if entry.is_dir() || entry.file_name().is_some_and(|name| name == FILE) {
                collect(&entry, summaries)?;
            }
        }
        return Ok(());
    }
    let text = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let summary: Summary =
        serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    summaries.push((path.display().to_string(), summary));
    Ok(())
}

/// Every required gate ran at least once and passed every time it ran.
pub(crate) fn check(required: &[String], summaries: &[(String, Summary)]) -> Vec<String> {
    let mut findings = Vec::new();
    for gate in required {
        let records: Vec<(&str, &GateRecord)> = summaries
            .iter()
            .flat_map(|(source, summary)| {
                summary
                    .gates
                    .iter()
                    .filter(|record| record.name == *gate)
                    .map(move |record| (source.as_str(), record))
            })
            .collect();
        if records.is_empty() {
            findings.push(format!("{gate} did not run in any job"));
        }
        for (source, record) in records {
            if record.status != "passed" {
                findings.push(format!(
                    "{gate} {} ({source})",
                    record.status.replace('_', " ")
                ));
            } else if record.tests.failed > 0 {
                findings.push(format!(
                    "{gate} reported {} failed test(s) although it passed ({source})",
                    record.tests.failed
                ));
            }
        }
    }
    let known: BTreeSet<&str> = required.iter().map(String::as_str).collect();
    for (source, summary) in summaries {
        for record in &summary.gates {
            if !known.contains(record.name.as_str()) && record.status != "passed" {
                findings.push(format!("{} {} ({source})", record.name, record.status));
            }
        }
        findings.extend(run_findings(source, summary));
    }
    findings
}

/// What a run's own record says against it, whatever its gates did.
fn run_findings(source: &str, summary: &Summary) -> Vec<String> {
    let Some(run) = &summary.run else {
        return vec![format!(
            "{source} records no run, so nothing says what the run left behind"
        )];
    };
    let mut findings = Vec::new();
    if !run.cleanup_checked {
        findings.push(format!(
            "run {} stopped before it checked what it left behind ({source})",
            run.id
        ));
    }
    if !run.leftovers.is_empty() {
        findings.push(format!(
            "run {} left behind {} ({source})",
            run.id,
            run.leftovers.join(", ")
        ));
    }
    if !run.worktree_changes.is_empty() {
        findings.push(format!(
            "run {} changed the worktree: {} ({source})",
            run.id,
            run.worktree_changes.join(", ")
        ));
    }
    findings
}

/// The `needs` context of the job running the check: every job it waited for
/// must have succeeded, not been skipped or cancelled.
fn check_needs(needs: &str) -> Result<Vec<String>, String> {
    let needs: Value =
        serde_json::from_str(needs).map_err(|error| format!("--needs is not JSON: {error}"))?;
    let jobs = needs
        .as_object()
        .ok_or("--needs is not an object of jobs")?;
    Ok(jobs
        .iter()
        .filter_map(|(job, value)| {
            let result = value["result"].as_str().unwrap_or("unknown");
            (result != "success").then(|| format!("job {job} ended {result}"))
        })
        .collect())
}

fn publish(required: &[String], summaries: &[(String, Summary)], findings: &[String]) {
    use std::fmt::Write as _;

    let Some(path) = std::env::var_os("GITHUB_STEP_SUMMARY") else {
        return;
    };
    let mut table = String::from(
        "### Required gates\n\n| Gate | Status | Duration | Tests | Log SHA-256 |\n| --- | --- | --- | --- | --- |\n",
    );
    for gate in required {
        let record = summaries
            .iter()
            .flat_map(|(_, summary)| &summary.gates)
            .find(|record| record.name == *gate);
        let _ = if let Some(record) = record {
            writeln!(
                table,
                "| `{gate}` | {} | {:.1}s | {} passed, {} failed, {} ignored | `{}` |",
                record.status,
                record.duration_seconds,
                record.tests.passed,
                record.tests.failed,
                record.tests.ignored,
                record
                    .log
                    .sha256
                    .as_deref()
                    .map_or("none", |hash| &hash[..12])
            )
        } else {
            writeln!(table, "| `{gate}` | **missing** | | | |")
        };
    }
    if !findings.is_empty() {
        table.push_str("\n**Not passed:**\n\n");
        for finding in findings {
            let _ = writeln!(table, "- {finding}");
        }
    }
    let _ = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| std::io::Write::write_all(&mut file, table.as_bytes()));
}

#[cfg(test)]
mod tests {
    use super::{GateRecord, Log, RunRecord, Summary, Tests, check};

    fn record(name: &str, status: &str) -> GateRecord {
        GateRecord {
            name: name.to_string(),
            status: status.to_string(),
            duration_seconds: 1.0,
            commands: Vec::new(),
            tools: std::collections::BTreeMap::default(),
            tests: Tests::default(),
            log: Log {
                path: format!("{name}.log"),
                sha256: None,
            },
        }
    }

    fn clean_run() -> RunRecord {
        RunRecord {
            id: "1700000000000-1".to_string(),
            revision: Some("0".repeat(40)),
            profile: Some("full".to_string()),
            host: "linux-x86_64".to_string(),
            started_unix: 1_700_000_000,
            finished_unix: Some(1_700_000_100),
            clean_at_start: true,
            worktree_changes: Vec::new(),
            cleanup_checked: true,
            cleanup_scope: vec!["docker".to_string(), "files".to_string()],
            leftovers: Vec::new(),
        }
    }

    #[test]
    fn a_run_that_left_something_changed_the_worktree_or_stopped_early_is_refused() {
        let gates = || vec![record("format", "passed")];
        let mut left = summary(gates());
        left.1.run.as_mut().expect("run").leftovers =
            vec!["container capsulet-verify-1700000000000-1-postgres".to_string()];
        let mut changed = summary(gates());
        changed.1.run.as_mut().expect("run").worktree_changes =
            vec!["crates/api/openapi.json".to_string()];
        let mut stopped = summary(gates());
        stopped.1.run.as_mut().expect("run").cleanup_checked = false;
        let mut unrecorded = summary(gates());
        unrecorded.1.run = None;
        let required = ["format".to_string()];
        assert_eq!(
            check(&required, &[left]),
            vec![
                "run 1700000000000-1 left behind container capsulet-verify-1700000000000-1-postgres (job)"
            ]
        );
        assert_eq!(
            check(&required, &[changed]),
            vec!["run 1700000000000-1 changed the worktree: crates/api/openapi.json (job)"]
        );
        assert_eq!(
            check(&required, &[stopped]),
            vec!["run 1700000000000-1 stopped before it checked what it left behind (job)"]
        );
        assert_eq!(
            check(&required, &[unrecorded]),
            vec!["job records no run, so nothing says what the run left behind"]
        );
    }

    fn summary(records: Vec<GateRecord>) -> (String, Summary) {
        (
            "job".to_string(),
            Summary {
                schema_version: 2,
                verifier: "0.1.0".to_string(),
                run: Some(clean_run()),
                gates: records,
            },
        )
    }

    #[test]
    fn every_required_gate_must_have_run_and_passed() {
        let required = ["format".to_string(), "unit".to_string(), "kind".to_string()];
        let findings = check(
            &required,
            &[
                summary(vec![record("format", "passed")]),
                summary(vec![record("unit", "timed_out")]),
            ],
        );
        assert_eq!(
            findings,
            vec!["unit timed out (job)", "kind did not run in any job"]
        );
        let all = check(
            &required,
            &[summary(vec![
                record("format", "passed"),
                record("unit", "passed"),
                record("kind", "passed"),
            ])],
        );
        assert!(all.is_empty(), "{all:?}");
    }

    #[test]
    fn a_passed_gate_with_failed_tests_is_refused() {
        let mut unit = record("unit", "passed");
        unit.tests.failed = 2;
        let findings = check(&["unit".to_string()], &[summary(vec![unit])]);
        assert_eq!(
            findings,
            vec!["unit reported 2 failed test(s) although it passed (job)"]
        );
    }
}
