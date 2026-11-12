//! Holds the test evidence a product claim cites to what its gate actually runs.
//!
//! A claim's test evidence names a verifier gate, one of that gate's commands,
//! the file the test lives in and the test. The registry used to carry its own
//! list of what each command "collects" beside the evidence, and that list was a
//! copy of the evidence: it could not disagree with it, so it proved nothing.
//! Here the command itself is asked. Cargo test targets are built and listed,
//! Python's loader enumerates what `unittest discover` would run, the dashboard
//! suite reports what it ran through Node's `junit` reporter, and a PowerShell test
//! script lists its cases. What the commands report is written to
//! `docs/contracts/test-manifest.json`, which the claims checker reads, and
//! `--check` fails when that file no longer matches what the gates collect.

mod collect;

use std::{collections::BTreeMap, fs, path::Path};

use serde::Serialize;
use serde_json::Value;

use crate::verify::{
    catalog::{CommandSpec, Gate, gates},
    workspace_root,
};

pub(crate) use collect::Collected;

const REGISTRY: &str = "docs/contracts/product-claims.json";
const MANIFEST: &str = "docs/contracts/test-manifest.json";
const GENERATED_BY: &str = "cargo run -p capsulet-xtask --locked -- claims collect";

pub(crate) fn run(arguments: &[String]) -> Result<(), String> {
    let check = match arguments {
        [command] if command == "collect" => false,
        [command, flag] if command == "collect" && flag == "--check" => true,
        _ => return Err("usage: capsulet-xtask claims collect [--check]".to_string()),
    };
    let root = workspace_root();
    let evidence = read_evidence(&root.join(REGISTRY))?;
    let manifest = build_manifest(&gates(), &evidence, &mut |command| {
        collect::collect(&root, command)
    })?;
    let rendered = render_manifest(&manifest)?;
    let path = root.join(MANIFEST);
    let records = manifest
        .commands
        .iter()
        .map(|command| command.tests.len())
        .sum::<usize>();
    if check {
        compare(&path, &rendered)?;
        println!(
            "{MANIFEST} matches what {} gate commands collect ({records} cited tests)",
            manifest.commands.len()
        );
    } else {
        fs::write(&path, rendered)
            .map_err(|error| format!("could not write {}: {error}", path.display()))?;
        println!(
            "wrote {MANIFEST}: {records} cited tests collected by {} gate commands",
            manifest.commands.len()
        );
    }
    Ok(())
}

/// One piece of test evidence a claim cites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Evidence {
    pub(crate) claim: String,
    pub(crate) gate: String,
    pub(crate) command: String,
    pub(crate) path: String,
    pub(crate) selector: String,
}

fn read_evidence(registry: &Path) -> Result<Vec<Evidence>, String> {
    let raw = fs::read_to_string(registry)
        .map_err(|error| format!("could not read {}: {error}", registry.display()))?;
    let document: Value = serde_json::from_str(&raw)
        .map_err(|error| format!("{} is not JSON: {error}", registry.display()))?;
    parse_evidence(&document)
}

pub(crate) fn parse_evidence(document: &Value) -> Result<Vec<Evidence>, String> {
    let claims = document
        .get("claims")
        .and_then(Value::as_array)
        .ok_or("the claim registry has no claims array")?;
    let mut evidence = Vec::new();
    let mut problems = Vec::new();
    for claim in claims {
        let id = claim.get("id").and_then(Value::as_str).unwrap_or("<no id>");
        let records = claim
            .get("evidence")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice);
        for record in records {
            if record.get("type").and_then(Value::as_str) != Some("test") {
                continue;
            }
            let field = |name: &str| record.get(name).and_then(Value::as_str);
            match (
                field("gate"),
                field("command"),
                field("path"),
                field("selector"),
            ) {
                (Some(gate), Some(command), Some(path), Some(selector)) => {
                    evidence.push(Evidence {
                        claim: id.to_string(),
                        gate: gate.to_string(),
                        command: command.to_string(),
                        path: path.to_string(),
                        selector: selector.to_string(),
                    });
                }
                (None, ..) => problems.push(format!(
                    "test evidence for '{id}' does not name the gate that runs it"
                )),
                _ => problems.push(format!(
                    "test evidence for '{id}' needs a command, a path and a selector"
                )),
            }
        }
    }
    if problems.is_empty() {
        Ok(evidence)
    } else {
        Err(problems.join("\n"))
    }
}

/// A gate command as evidence spells it.
///
/// PowerShell is written `pwsh` whichever host runs it: the catalog starts
/// Windows PowerShell on Windows and PowerShell 7 elsewhere, and evidence that
/// named one of them would match the gate on only one kind of machine.
pub(crate) fn render_command(command: &CommandSpec) -> String {
    let program = if command.program == "powershell" {
        "pwsh"
    } else {
        command.program.as_str()
    };
    let line = std::iter::once(program)
        .chain(command.arguments.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ");
    match &command.working_directory {
        Some(directory) => format!("cd {directory} && {line}"),
        None => line,
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct Manifest {
    schema_version: u32,
    generated_by: &'static str,
    pub(crate) commands: Vec<ManifestCommand>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct ManifestCommand {
    pub(crate) gate: String,
    pub(crate) command: String,
    pub(crate) tests: Vec<ManifestTest>,
}

#[derive(Debug, Serialize, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub(crate) struct ManifestTest {
    pub(crate) path: String,
    pub(crate) selector: String,
}

/// Checks every piece of evidence against what its gate command collects.
///
/// Every problem is reported, not only the first: a registry edited by hand
/// usually has several, and finding them one run at a time is slow when a run
/// builds the workspace's tests.
pub(crate) fn build_manifest(
    catalog: &[Gate],
    evidence: &[Evidence],
    collector: &mut dyn FnMut(&CommandSpec) -> Result<Vec<Collected>, String>,
) -> Result<Manifest, String> {
    let mut problems = Vec::new();
    // Keyed by position in the gate graph, so the manifest reads in the order
    // the verifier runs things and is the same wherever it is generated.
    let mut groups: BTreeMap<(usize, usize), Vec<&Evidence>> = BTreeMap::new();
    for record in evidence {
        let Some(gate_index) = catalog.iter().position(|gate| gate.name == record.gate) else {
            problems.push(format!(
                "'{}' cites gate '{}', which the verifier does not define",
                record.claim, record.gate
            ));
            continue;
        };
        let gate = &catalog[gate_index];
        let Some(command_index) = gate
            .commands
            .iter()
            .position(|command| render_command(command) == record.command)
        else {
            let commands = gate
                .commands
                .iter()
                .map(|command| format!("`{}`", render_command(command)))
                .collect::<Vec<_>>()
                .join(", ");
            problems.push(format!(
                "'{}' cites `{}`, which is not a command of gate '{}' (its commands: {commands})",
                record.claim, record.command, gate.name
            ));
            continue;
        };
        groups
            .entry((gate_index, command_index))
            .or_default()
            .push(record);
    }

    let mut commands = Vec::new();
    for ((gate_index, command_index), records) in groups {
        let gate = &catalog[gate_index];
        let command = &gate.commands[command_index];
        let rendered = render_command(command);
        let collected = match collector(command) {
            Ok(collected) => collected,
            Err(error) => {
                problems.push(format!(
                    "gate '{}' could not list `{rendered}`: {error}",
                    gate.name
                ));
                continue;
            }
        };
        let mut tests = Vec::new();
        for record in records {
            match judge(record, &collected) {
                Ok(()) => tests.push(ManifestTest {
                    path: record.path.clone(),
                    selector: record.selector.clone(),
                }),
                Err(problem) => problems.push(problem),
            }
        }
        tests.sort();
        tests.dedup();
        commands.push(ManifestCommand {
            gate: gate.name.to_string(),
            command: rendered,
            tests,
        });
    }

    if problems.is_empty() {
        Ok(Manifest {
            schema_version: 1,
            generated_by: GENERATED_BY,
            commands,
        })
    } else {
        Err(format!(
            "{} claim evidence problem(s):\n{}",
            problems.len(),
            problems.join("\n")
        ))
    }
}

/// Says whether a piece of evidence is a test its command runs, and if not, why.
fn judge(record: &Evidence, collected: &[Collected]) -> Result<(), String> {
    let named = collected
        .iter()
        .filter(|test| test.is_named(&record.selector))
        .collect::<Vec<_>>();
    let here = named
        .iter()
        .filter(|test| test.declared_in(&record.path))
        .collect::<Vec<_>>();
    if here.iter().any(|test| test.runs) {
        return Ok(());
    }
    if let Some(test) = here.first() {
        return Err(format!(
            "'{}' cites `{}` in {}, which `{}` collects as {} but never runs (ignored or skipped)",
            record.claim, record.selector, record.path, record.command, test.name
        ));
    }
    if let Some(test) = named.first() {
        return Err(format!(
            "'{}' cites `{}` in {}, but the test `{}` collects by that name comes from {}",
            record.claim, record.selector, record.path, record.command, test.source
        ));
    }
    Err(format!(
        "'{}' cites `{}` in {}, which `{}` does not collect",
        record.claim, record.selector, record.path, record.command
    ))
}

pub(crate) fn render_manifest(manifest: &Manifest) -> Result<String, String> {
    serde_json::to_string_pretty(manifest)
        .map(|text| text + "\n")
        .map_err(|error| error.to_string())
}

fn compare(path: &Path, rendered: &str) -> Result<(), String> {
    let regenerate = format!("run `{GENERATED_BY}`");
    let committed = fs::read_to_string(path)
        .map_err(|error| format!("{MANIFEST} could not be read ({error}); {regenerate}"))?
        .replace("\r\n", "\n");
    if committed == rendered {
        return Ok(());
    }
    // Say where it diverges: the gate that fails is usually on a machine nobody
    // can look at afterwards.
    let committed_lines = committed.lines().collect::<Vec<_>>();
    let rendered_lines = rendered.lines().collect::<Vec<_>>();
    let at = committed_lines
        .iter()
        .zip(&rendered_lines)
        .take_while(|(left, right)| left == right)
        .count();
    let line = |lines: &[&str]| {
        lines.get(at).map_or_else(
            || "<end of file>".to_string(),
            |text| text.trim().to_string(),
        )
    };
    Err(format!(
        "{MANIFEST} is stale; {regenerate} (line {}: committed '{}' but collected '{}')",
        at + 1,
        line(&committed_lines),
        line(&rendered_lines)
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::{Collected, Evidence, build_manifest, parse_evidence, render_command};
    use crate::verify::catalog::{CommandSpec, Gate, Profile};

    fn command(program: &str, arguments: &[&str], directory: Option<&str>) -> CommandSpec {
        CommandSpec {
            program: program.to_string(),
            arguments: arguments.iter().map(|word| (*word).to_string()).collect(),
            working_directory: directory.map(str::to_string),
            environment: BTreeMap::new(),
        }
    }

    fn catalog() -> Vec<Gate> {
        vec![Gate {
            name: "unit",
            description: "",
            prerequisites: Vec::new(),
            timeout_seconds: 1,
            artifacts: Vec::new(),
            profiles: vec![Profile::Fast],
            commands: vec![
                command("cargo", &["test", "-p", "demo", "--locked"], None),
                command("npm", &["test"], Some("dashboard")),
            ],
            provision: Vec::new(),
            teardown: Vec::new(),
            every_test_runs: false,
            stack: None,
        }]
    }

    fn evidence(gate: &str, command: &str, path: &str, selector: &str) -> Evidence {
        Evidence {
            claim: "CAP-DEMO-001".to_string(),
            gate: gate.to_string(),
            command: command.to_string(),
            path: path.to_string(),
            selector: selector.to_string(),
        }
    }

    fn rust_test(name: &str, runs: bool) -> Collected {
        Collected::rust("crates/demo", "crates/demo/tests/flow.rs", name, runs)
    }

    #[test]
    fn evidence_without_a_gate_is_refused() {
        let registry = json!({"claims": [{"id": "CAP-DEMO-001", "evidence": [
            {"type": "test", "path": "a.rs", "command": "cargo test", "selector": "x"},
            {"type": "source", "path": "b.rs"}
        ]}]});
        let error = parse_evidence(&registry).expect_err("no gate");
        assert!(
            error.contains("does not name the gate that runs it"),
            "{error}"
        );
    }

    #[test]
    fn commands_are_spelled_the_same_on_every_host() {
        assert_eq!(
            render_command(&command("powershell", &["-File", "x.ps1"], None)),
            "pwsh -File x.ps1"
        );
        assert_eq!(
            render_command(&command("npm", &["test"], Some("dashboard"))),
            "cd dashboard && npm test"
        );
    }

    #[test]
    fn a_command_the_gate_does_not_run_is_refused() {
        let cited = [evidence(
            "unit",
            "cargo test -p demo",
            "crates/demo/tests/flow.rs",
            "runs",
        )];
        let error = build_manifest(&catalog(), &cited, &mut |_| Ok(Vec::new()))
            .expect_err("not a gate command");
        assert!(error.contains("is not a command of gate 'unit'"), "{error}");
    }

    #[test]
    fn an_unknown_gate_is_refused() {
        let cited = [evidence("nightly", "cargo test", "a.rs", "x")];
        let error =
            build_manifest(&catalog(), &cited, &mut |_| Ok(Vec::new())).expect_err("unknown gate");
        assert!(
            error.contains("which the verifier does not define"),
            "{error}"
        );
    }

    #[test]
    fn a_selector_that_exists_but_is_ignored_is_not_evidence() {
        let cited = [evidence(
            "unit",
            "cargo test -p demo --locked",
            "crates/demo/tests/flow.rs",
            "slow_path",
        )];
        let error = build_manifest(&catalog(), &cited, &mut |_| {
            Ok(vec![rust_test("slow_path", false)])
        })
        .expect_err("ignored");
        assert!(error.contains("never runs"), "{error}");
    }

    #[test]
    fn a_selector_the_command_does_not_collect_is_not_evidence() {
        let cited = [evidence(
            "unit",
            "cargo test -p demo --locked",
            "crates/demo/tests/flow.rs",
            "missing",
        )];
        let error = build_manifest(&catalog(), &cited, &mut |_| {
            Ok(vec![rust_test("present", true)])
        })
        .expect_err("not collected");
        assert!(error.contains("does not collect"), "{error}");
    }

    #[test]
    fn a_test_of_the_same_name_in_another_file_is_not_evidence() {
        let cited = [evidence(
            "unit",
            "cargo test -p demo --locked",
            "crates/demo/tests/other.rs",
            "present",
        )];
        let error = build_manifest(&catalog(), &cited, &mut |_| {
            Ok(vec![rust_test("present", true)])
        })
        .expect_err("wrong file");
        assert!(
            error.contains("comes from crates/demo/tests/flow.rs"),
            "{error}"
        );
    }

    #[test]
    fn collected_evidence_is_recorded_once_in_gate_order() {
        let cited = [
            evidence(
                "unit",
                "cd dashboard && npm test",
                "dashboard/tests/api.test.ts",
                "formats sizes",
            ),
            evidence(
                "unit",
                "cargo test -p demo --locked",
                "crates/demo/tests/flow.rs",
                "present",
            ),
            evidence(
                "unit",
                "cargo test -p demo --locked",
                "crates/demo/tests/flow.rs",
                "present",
            ),
        ];
        let manifest = build_manifest(&catalog(), &cited, &mut |command| {
            Ok(if command.program == "npm" {
                vec![Collected::file(
                    "dashboard/tests/api.test.ts",
                    "formats sizes",
                    "helpers > formats sizes",
                    true,
                )]
            } else {
                vec![rust_test("present", true)]
            })
        })
        .expect("collected");
        let order = manifest
            .commands
            .iter()
            .map(|command| (command.command.as_str(), command.tests.len()))
            .collect::<Vec<_>>();
        assert_eq!(
            order,
            [
                ("cargo test -p demo --locked", 1),
                ("cd dashboard && npm test", 1)
            ]
        );
    }
}
