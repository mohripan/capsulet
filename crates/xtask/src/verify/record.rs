//! `verify record <run directory> --output <file>`: the record of a
//! clean-checkout rehearsal, made from one `verify --profile full` run.
//!
//! It refuses a run that is not one. The run must:
//!
//! - have selected the full profile;
//! - have started from a clean Git checkout;
//! - have passed every gate of the profile;
//! - have changed nothing in the worktree;
//! - have left nothing behind.
//!
//! The record holds what the rehearsal's evidence is:
//!
//! - the commit, the host and the duration;
//! - every tool's reported version;
//! - each gate's status, duration and test counts, and its log's SHA-256;
//! - the ID of every image the run built;
//! - the chart's render digest;
//! - the SHA-256 of the run's `verification-summary.json`.
//!
//! The gate logs and the summary themselves stay with the run: they hold the
//! machine's paths.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    catalog::{Profile, gates},
    summary::{self, Summary},
};

pub(crate) const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Record {
    pub(crate) schema_version: u32,
    pub(crate) revision: String,
    pub(crate) verifier: String,
    pub(crate) profile: String,
    pub(crate) host: String,
    pub(crate) started_unix: u64,
    pub(crate) finished_unix: u64,
    pub(crate) duration_seconds: u64,
    pub(crate) clean_at_start: bool,
    pub(crate) worktree_changes: Vec<String>,
    pub(crate) cleanup_scope: Vec<String>,
    pub(crate) leftovers: Vec<String>,
    /// What each tool a gate needed reported as its version.
    pub(crate) tools: BTreeMap<String, String>,
    pub(crate) gates: Vec<GateResult>,
    pub(crate) totals: Totals,
    pub(crate) images: Vec<BuiltImage>,
    pub(crate) chart_render_sha256: String,
    pub(crate) verification_summary_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GateResult {
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) duration_seconds: f64,
    pub(crate) test_binaries: usize,
    pub(crate) tests_passed: u64,
    pub(crate) tests_failed: u64,
    pub(crate) tests_ignored: u64,
    pub(crate) log_sha256: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct Totals {
    pub(crate) gates: usize,
    pub(crate) gates_passed: usize,
    pub(crate) test_binaries: usize,
    pub(crate) tests_passed: u64,
    pub(crate) tests_failed: u64,
    pub(crate) tests_ignored: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BuiltImage {
    pub(crate) component: String,
    pub(crate) image: String,
    pub(crate) version: String,
    pub(crate) revision: String,
    pub(crate) id: String,
}

const USAGE: &str = "usage: capsulet-xtask verify record <run directory> --output <file>";

pub(crate) fn run(arguments: &[String]) -> Result<(), String> {
    let [directory, flag, output] = arguments else {
        return Err(USAGE.to_string());
    };
    if flag != "--output" {
        return Err(USAGE.to_string());
    }
    let record = make(Path::new(directory))?;
    let json = serde_json::to_string_pretty(&record).map_err(|error| error.to_string())?;
    fs::write(output, json + "\n").map_err(|error| format!("{output}: {error}"))?;
    println!(
        "[verify] recorded the clean-checkout rehearsal of {} in {output}: {} gates, {} tests \
         passed, {} images, chart {}",
        record.revision,
        record.totals.gates_passed,
        record.totals.tests_passed,
        record.images.len(),
        record.chart_render_sha256
    );
    Ok(())
}

pub(crate) fn make(directory: &Path) -> Result<Record, String> {
    let summary_path = directory.join(summary::FILE);
    let bytes =
        fs::read(&summary_path).map_err(|error| format!("{}: {error}", summary_path.display()))?;
    let summary: Summary = serde_json::from_slice(&bytes)
        .map_err(|error| format!("{}: {error}", summary_path.display()))?;
    let run = summary
        .run
        .as_ref()
        .ok_or("the summary records no run, so it cannot be a rehearsal")?;
    let required: Vec<String> = gates()
        .into_iter()
        .filter(|gate| gate.profiles.contains(&Profile::Full))
        .map(|gate| gate.name.to_string())
        .collect();
    let mut problems = summary::check(
        &required,
        &[(summary_path.display().to_string(), summary.clone())],
    );
    if run.profile.as_deref() != Some("full") {
        problems.push("the run did not select --profile full".to_string());
    }
    if !run.clean_at_start {
        problems.push("the checkout was not clean when the run started".to_string());
    }
    let Some(revision) = run.revision.clone() else {
        return Err("the run was not of a Git checkout".to_string());
    };
    let Some(finished) = run.finished_unix else {
        return Err("the run has no finishing time".to_string());
    };
    if !problems.is_empty() {
        return Err(format!(
            "run {} is not a clean-checkout rehearsal:\n  - {}",
            run.id,
            problems.join("\n  - ")
        ));
    }
    let images = images(directory)?;
    let chart = read_json(&directory.join("chart-render.json"))?;
    let chart_render_sha256 = chart["sha256"]
        .as_str()
        .ok_or("chart-render.json holds no sha256")?
        .to_string();
    let mut tools = BTreeMap::new();
    let mut totals = Totals::default();
    let gates: Vec<GateResult> = summary
        .gates
        .iter()
        .map(|gate| {
            tools.extend(gate.tools.clone());
            totals.gates += 1;
            totals.gates_passed += usize::from(gate.status == "passed");
            totals.test_binaries += gate.tests.binaries;
            totals.tests_passed += gate.tests.passed;
            totals.tests_failed += gate.tests.failed;
            totals.tests_ignored += gate.tests.ignored;
            GateResult {
                name: gate.name.clone(),
                status: gate.status.clone(),
                duration_seconds: gate.duration_seconds,
                test_binaries: gate.tests.binaries,
                tests_passed: gate.tests.passed,
                tests_failed: gate.tests.failed,
                tests_ignored: gate.tests.ignored,
                log_sha256: gate.log.sha256.clone(),
            }
        })
        .collect();
    Ok(Record {
        schema_version: SCHEMA_VERSION,
        revision,
        verifier: summary.verifier.clone(),
        profile: "full".to_string(),
        host: run.host.clone(),
        started_unix: run.started_unix,
        finished_unix: finished,
        duration_seconds: finished.saturating_sub(run.started_unix),
        clean_at_start: run.clean_at_start,
        worktree_changes: run.worktree_changes.clone(),
        cleanup_scope: run.cleanup_scope.clone(),
        leftovers: run.leftovers.clone(),
        tools,
        gates,
        totals,
        images,
        chart_render_sha256,
        verification_summary_sha256: format!("sha256:{}", hex(&Sha256::digest(&bytes))),
    })
}

/// Every image the run built, once per component and ID: the `containers`
/// and `kind` gates each build every component.
fn images(directory: &Path) -> Result<Vec<BuiltImage>, String> {
    let path = directory.join("images.jsonl");
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut seen = BTreeSet::new();
    let mut images = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|error| format!("{}: {error}", path.display()))?;
        let field = |name: &str| value[name].as_str().unwrap_or_default().to_string();
        let image = BuiltImage {
            component: field("component"),
            image: field("image"),
            version: field("version"),
            revision: field("revision"),
            id: field("id"),
        };
        if seen.insert((image.component.clone(), image.id.clone())) {
            images.push(image);
        }
    }
    if images.is_empty() {
        return Err(format!("{} names no image", path.display()));
    }
    Ok(images)
}

fn read_json(path: &PathBuf) -> Result<serde_json::Value, String> {
    let text = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}
