//! `completion check`: M1 is closed only by a record that links every one of
//! its deliverables and exit criteria to the gates that prove it, and to the
//! latest clean-checkout rehearsal in which those gates passed.
//!
//! The criteria come from the engineering integrity plan: each item of its
//! "M1 release gate" list (`G1`...) and of its "M1 exit criteria" (`E1`...).
//! `docs/contracts/m1-completion.md` must have, under `## Criteria`, one table
//! row per item:
//!
//! - its id;
//! - the criterion, word for word, so the plan cannot change underneath it;
//! - the gates, every one in the full profile;
//! - the evidence, which links the rehearsal record.
//!
//! The record (`docs/contracts/clean-checkout-rehearsal.json`, made by `verify
//! record`) must be of a commit this checkout descends from. It must show a
//! full-profile run of a clean checkout that changed nothing, left nothing, and
//! passed every gate the table names. The completion report must also say what
//! risks remain, which security exceptions it closes with, where it was run, and
//! what the next milestone needs.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    process::Command,
};

use crate::{
    release::split_root,
    verify::{
        catalog::{Profile, gates},
        record::{Record, SCHEMA_VERSION},
    },
};

const PLAN: &str = "docs/superpowers/plans/2026-08-31-engineering-integrity.md";
const REPORT: &str = "docs/contracts/m1-completion.md";
const RECORD: &str = "docs/contracts/clean-checkout-rehearsal.json";
const RECORD_LINK: &str = "(clean-checkout-rehearsal.json)";
const SECTIONS: &[&str] = &[
    "## Criteria",
    "## Remaining risks",
    "## Security exceptions",
    "## Platform support",
    "## Entry conditions for the next milestone",
];

const USAGE: &str = "usage: capsulet-xtask completion check [--root <directory>]";

pub(crate) fn run(arguments: &[String]) -> Result<(), String> {
    let (root, arguments) = split_root(arguments)?;
    if arguments != ["check"] {
        return Err(USAGE.to_string());
    }
    let findings = check(&root);
    if findings.is_empty() {
        println!(
            "[completion] every M1 deliverable and exit criterion names its gates, and the \
             clean-checkout rehearsal in {RECORD} passed them"
        );
        Ok(())
    } else {
        Err(format!(
            "{} M1 completion finding(s):\n  - {}",
            findings.len(),
            findings.join("\n  - ")
        ))
    }
}

pub(crate) fn check(root: &Path) -> Vec<String> {
    let mut findings = Vec::new();
    let criteria = match fs::read_to_string(root.join(PLAN)) {
        Ok(plan) => criteria(&plan),
        Err(error) => return vec![format!("{PLAN}: {error}")],
    };
    if criteria.is_empty() {
        findings.push(format!("{PLAN} lists no M1 release gate or exit criteria"));
    }
    let report = match fs::read_to_string(root.join(REPORT)) {
        Ok(report) => report,
        Err(error) => {
            findings.push(format!("{REPORT}: {error}"));
            return findings;
        }
    };
    for section in SECTIONS {
        if !has_section(&report, section) {
            findings.push(format!(
                "{REPORT} has no `{section}` section, or it is empty"
            ));
        }
    }
    let rows = rows(&report);
    let full: BTreeSet<&str> = gates()
        .into_iter()
        .filter(|gate| gate.profiles.contains(&Profile::Full))
        .map(|gate| gate.name)
        .collect();
    let mut named = BTreeSet::new();
    for (id, criterion) in &criteria {
        let Some(row) = rows.get(id) else {
            findings.push(format!("{REPORT} has no row for {id}: {criterion}"));
            continue;
        };
        if normalize(&row.criterion) != *criterion {
            findings.push(format!(
                "{REPORT}: {id} reads \"{}\", but the plan says \"{criterion}\"",
                normalize(&row.criterion)
            ));
        }
        if row.gates.is_empty() {
            findings.push(format!("{REPORT}: {id} names no gate"));
        }
        for gate in &row.gates {
            if full.contains(gate.as_str()) {
                named.insert(gate.clone());
            } else {
                findings.push(format!(
                    "{REPORT}: {id} names `{gate}`, which is not a gate of the full profile"
                ));
            }
        }
        if !row.evidence.contains(RECORD_LINK) {
            findings.push(format!(
                "{REPORT}: {id} does not link the clean-checkout record {RECORD_LINK}"
            ));
        }
    }
    for id in rows.keys() {
        if !criteria.contains_key(id) {
            findings.push(format!("{REPORT}: {id} is not an M1 criterion of {PLAN}"));
        }
    }
    findings.extend(record_findings(root, &named));
    findings
}

/// What the record says against itself, and against the gates the table names.
fn record_findings(root: &Path, named: &BTreeSet<String>) -> Vec<String> {
    let text = match fs::read_to_string(root.join(RECORD)) {
        Ok(text) => text,
        Err(error) => return vec![format!("{RECORD}: {error}")],
    };
    let record: Record = match serde_json::from_str(&text) {
        Ok(record) => record,
        Err(error) => return vec![format!("{RECORD}: {error}")],
    };
    let mut findings = Vec::new();
    if record.schema_version != SCHEMA_VERSION {
        findings.push(format!(
            "{RECORD} has schema version {}, not {SCHEMA_VERSION}",
            record.schema_version
        ));
    }
    if record.profile != "full" || !record.clean_at_start {
        findings.push(format!(
            "{RECORD} is not of a full-profile run of a clean checkout"
        ));
    }
    if !record.worktree_changes.is_empty() {
        findings.push(format!(
            "{RECORD}: the run changed {}",
            record.worktree_changes.join(", ")
        ));
    }
    if !record.leftovers.is_empty() {
        findings.push(format!(
            "{RECORD}: the run left behind {}",
            record.leftovers.join(", ")
        ));
    }
    for scope in ["docker", "kind", "files"] {
        if !record.cleanup_scope.iter().any(|checked| checked == scope) {
            findings.push(format!(
                "{RECORD}: the run did not check {scope} for what it left behind"
            ));
        }
    }
    let passed: BTreeSet<&str> = record
        .gates
        .iter()
        .filter(|gate| gate.status == "passed" && gate.tests_failed == 0)
        .map(|gate| gate.name.as_str())
        .collect();
    if passed.len() != record.gates.len() || record.totals.tests_failed > 0 {
        findings.push(format!("{RECORD} records a gate that did not pass"));
    }
    for gate in named {
        if !passed.contains(gate.as_str()) {
            findings.push(format!(
                "{RECORD} does not show `{gate}` passing, which {REPORT} cites"
            ));
        }
    }
    if record.images.is_empty() {
        findings.push(format!("{RECORD} records no image"));
    }
    for (what, digest) in [
        ("chart render", &record.chart_render_sha256),
        ("verification summary", &record.verification_summary_sha256),
    ] {
        let well_formed = digest
            .strip_prefix("sha256:")
            .is_some_and(|hex| hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()));
        if !well_formed {
            findings.push(format!(
                "{RECORD}: the {what} digest {digest:?} is not a SHA-256"
            ));
        }
    }
    if !descends_from(root, &record.revision) {
        findings.push(format!(
            "{RECORD} is of {}, which is not a commit this checkout descends from",
            record.revision
        ));
    }
    findings
}

fn descends_from(root: &Path, revision: &str) -> bool {
    revision.len() == 40
        && revision.chars().all(|c| c.is_ascii_hexdigit())
        && Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["merge-base", "--is-ancestor", revision, "HEAD"])
            .status()
            .is_ok_and(|status| status.success())
}

/// `G1`... for the plan's numbered M1 release gate, `E1`... for its exit
/// criteria bullets, each normalized.
fn criteria(plan: &str) -> BTreeMap<String, String> {
    let mut criteria = BTreeMap::new();
    for (index, item) in items(section(plan, "## M1 release gate"), true)
        .into_iter()
        .enumerate()
    {
        criteria.insert(format!("G{}", index + 1), item);
    }
    for (index, item) in items(section(plan, "## M1 exit criteria"), false)
        .into_iter()
        .enumerate()
    {
        criteria.insert(format!("E{}", index + 1), item);
    }
    criteria
}

/// The lines under `heading`, up to the next heading of its level.
fn section<'a>(text: &'a str, heading: &str) -> Vec<&'a str> {
    text.lines()
        .skip_while(|line| line.trim_end() != heading)
        .skip(1)
        .take_while(|line| !line.starts_with("## "))
        .collect()
}

fn has_section(text: &str, heading: &str) -> bool {
    section(text, heading)
        .iter()
        .any(|line| !line.trim().is_empty())
}

/// The items of the first list in `lines`: numbered (`1. `) or bulleted (`- `),
/// with their indented continuation lines.
fn items(lines: Vec<&str>, numbered: bool) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    let mut open = false;
    for line in lines {
        let starts = if numbered {
            line.split_once(". ")
                .filter(|(number, _)| {
                    !number.is_empty() && number.chars().all(|c| c.is_ascii_digit())
                })
                .map(|(_, rest)| rest)
        } else {
            line.strip_prefix("- ")
        };
        if let Some(rest) = starts {
            items.push(rest.to_string());
            open = true;
        } else if open && line.starts_with("  ") && !line.trim().is_empty() {
            let last = items.last_mut().expect("an open item");
            last.push(' ');
            last.push_str(line.trim());
        } else if open && line.trim().is_empty() {
            open = false;
        }
    }
    items.into_iter().map(|item| normalize(&item)).collect()
}

/// Whitespace collapsed, and a list item's closing `;`, `; and`, `.` or
/// `, and` dropped, so a row reads the criterion the way the plan does.
fn normalize(text: &str) -> String {
    let mut text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    for ending in ["; and", ", and", ";", "."] {
        if let Some(stripped) = text.strip_suffix(ending) {
            text = stripped.trim_end().to_string();
            break;
        }
    }
    text
}

struct Row {
    criterion: String,
    gates: Vec<String>,
    evidence: String,
}

/// The rows of the `## Criteria` table, by id.
fn rows(report: &str) -> BTreeMap<String, Row> {
    let mut rows = BTreeMap::new();
    for line in section(report, "## Criteria") {
        let cells: Vec<&str> = line
            .trim()
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        let [id, criterion, gates, evidence] = cells.as_slice() else {
            continue;
        };
        let id_ok = id.len() >= 2
            && (id.starts_with('G') || id.starts_with('E'))
            && id[1..].chars().all(|c| c.is_ascii_digit());
        if !id_ok {
            continue;
        }
        rows.insert(
            (*id).to_string(),
            Row {
                criterion: (*criterion).to_string(),
                gates: gates
                    .split(',')
                    .map(|gate| gate.trim().trim_matches('`').to_string())
                    .filter(|gate| !gate.is_empty())
                    .collect(),
                evidence: (*evidence).to_string(),
            },
        );
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::{criteria, normalize};

    #[test]
    fn the_plan_lists_twelve_release_gate_items_and_eleven_exit_criteria() {
        let plan = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../")
                .join(super::PLAN),
        )
        .expect("plan");
        let criteria = criteria(&plan);
        assert_eq!(criteria.keys().filter(|id| id.starts_with('G')).count(), 12);
        assert_eq!(criteria.keys().filter(|id| id.starts_with('E')).count(), 11);
        assert_eq!(
            criteria["G10"],
            "Helm lint/schema/render checks and a real Kind install/smoke/uninstall test"
        );
        assert!(criteria["E11"].starts_with("documentation describes limitations honestly"));
    }

    #[test]
    fn a_list_items_ending_is_not_part_of_the_criterion() {
        assert_eq!(
            normalize("release-version checks; and"),
            "release-version checks"
        );
        assert_eq!(
            normalize("  cleanup and\n   verification. "),
            "cleanup and verification"
        );
    }
}
