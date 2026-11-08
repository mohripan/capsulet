//! What each test binary a gate ran reported, read from the gate's own log.
//!
//! A database suite that cannot reach its database used to return early and
//! report success, which looks exactly like a suite that ran. libtest's summary
//! line for each binary says how many tests passed, failed, were ignored and
//! were filtered out, so the verifier records those numbers for every command and
//! writes them to `test-counts.json` beside the gate logs. A gate that declares
//! that every one of its tests runs fails when a binary ran none, or ignored or
//! filtered any.

use std::fmt::Write as _;

use serde::Serialize;

/// One test binary's summary line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct BinaryCount {
    /// What cargo said it was running: `tests/ir_runs.rs`, `unittests src/lib.rs`,
    /// or `doc-tests capsulet_core`.
    pub(crate) target: String,
    pub(crate) passed: u64,
    pub(crate) failed: u64,
    pub(crate) ignored: u64,
    pub(crate) measured: u64,
    pub(crate) filtered: u64,
}

/// Every binary one gate command ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct CommandCounts {
    pub(crate) command: String,
    pub(crate) binaries: Vec<BinaryCount>,
}

/// One gate's commands, as `test-counts.json` records them.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct GateCounts {
    pub(crate) gate: &'static str,
    pub(crate) passed: bool,
    pub(crate) every_test_runs: bool,
    pub(crate) commands: Vec<CommandCounts>,
}

/// Reads the summary of every test binary in a stretch of log.
///
/// Cargo announces each binary (`Running tests/x.rs (…)`, `Doc-tests crate`)
/// before the binary prints its summary, and both write to the same log in the
/// order they happen, so each summary belongs to the last binary announced.
pub(crate) fn parse(log: &str) -> Vec<BinaryCount> {
    let mut binaries = Vec::new();
    let mut current: Option<String> = None;
    for raw in log.lines() {
        let line = strip_ansi(raw);
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Running ") {
            let target = rest.split(" (").next().unwrap_or(rest);
            current = Some(target.replace('\\', "/"));
        } else if let Some(rest) = line.strip_prefix("Doc-tests ") {
            current = Some(format!("doc-tests {rest}"));
        } else if let Some(count) = line
            .strip_prefix("test result: ")
            .and_then(|summary| summary_counts(summary, current.take()))
        {
            binaries.push(count);
        }
    }
    binaries
}

/// `ok. 3 passed; 0 failed; 1 ignored; 0 measured; 2 filtered out; finished in 0.01s`
fn summary_counts(summary: &str, target: Option<String>) -> Option<BinaryCount> {
    let (_, counts) = summary.split_once(". ")?;
    let mut count = BinaryCount {
        target: target.unwrap_or_else(|| "<unannounced binary>".to_string()),
        passed: 0,
        failed: 0,
        ignored: 0,
        measured: 0,
        filtered: 0,
    };
    for part in counts.split(';') {
        let mut words = part.split_whitespace();
        let (Some(number), Some(kind)) = (words.next(), words.next()) else {
            continue;
        };
        let Ok(number) = number.parse::<u64>() else {
            continue;
        };
        match kind {
            "passed" => count.passed = number,
            "failed" => count.failed = number,
            "ignored" => count.ignored = number,
            "measured" => count.measured = number,
            "filtered" => count.filtered = number,
            _ => {}
        }
    }
    Some(count)
}

/// Removes terminal colour codes, which CI turns on with `CARGO_TERM_COLOR=always`.
fn strip_ansi(line: &str) -> String {
    let mut plain = String::with_capacity(line.len());
    let mut characters = line.chars();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' {
            // ESC [ parameters final-byte
            if characters.next() == Some('[') {
                for code in characters.by_ref() {
                    if code.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            plain.push(character);
        }
    }
    plain
}

/// Fails a gate whose tests must all run when any of them did not.
///
/// Every command has to report at least one binary, and every binary has to
/// have run at least one test and ignored or filtered none. A suite that skips
/// itself, a filter that selects nothing, and an `#[ignore]` added to a database
/// test all fail here instead of passing quietly.
pub(crate) fn require_every_test_ran(gate: &str, commands: &[CommandCounts]) -> Result<(), String> {
    let mut problems = Vec::new();
    for command in commands {
        if command.binaries.is_empty() {
            problems.push(format!("`{}` reported no test binary", command.command));
        }
        for binary in &command.binaries {
            if binary.passed == 0 {
                problems.push(format!(
                    "`{}` ran no tests in {}",
                    command.command, binary.target
                ));
            }
            if binary.ignored > 0 || binary.filtered > 0 {
                problems.push(format!(
                    "`{}` left tests unrun in {}: {} ignored, {} filtered out",
                    command.command, binary.target, binary.ignored, binary.filtered
                ));
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{gate}: every test must run, but {}",
            problems.join("; ")
        ))
    }
}

/// A Markdown table of what the gates ran, for a CI job summary.
pub(crate) fn markdown(gates: &[GateCounts]) -> String {
    let mut table = String::from(
        "| Gate | Command | Binary | Passed | Failed | Ignored | Filtered |\n| --- | --- | --- | ---: | ---: | ---: | ---: |\n",
    );
    for gate in gates {
        for command in &gate.commands {
            for binary in &command.binaries {
                writeln!(
                    table,
                    "| `{}` | `{}` | `{}` | {} | {} | {} | {} |",
                    gate.gate,
                    command.command,
                    binary.target,
                    binary.passed,
                    binary.failed,
                    binary.ignored,
                    binary.filtered
                )
                .expect("writing to a String cannot fail");
            }
        }
    }
    table
}

#[cfg(test)]
mod tests {
    use super::{BinaryCount, CommandCounts, parse, require_every_test_ran};

    fn binary(target: &str, passed: u64, ignored: u64, filtered: u64) -> BinaryCount {
        BinaryCount {
            target: target.to_string(),
            passed,
            failed: 0,
            ignored,
            measured: 0,
            filtered,
        }
    }

    #[test]
    fn each_summary_belongs_to_the_binary_cargo_announced_before_it() {
        let log = "\
   Compiling capsulet-postgres v0.1.0
\u{1b}[1m\u{1b}[92m     Running\u{1b}[0m tests\\ir_runs.rs (target\\debug\\deps\\ir_runs-1a2b.exe)

running 3 tests
test a ... ok
test result: \u{1b}[32mok\u{1b}[0m. 3 passed; 0 failed; 1 ignored; 0 measured; 2 filtered out; finished in 0.40s

     Running unittests src/lib.rs (target/debug/deps/capsulet_postgres-9f.exe)
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
   Doc-tests capsulet_postgres
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
";
        assert_eq!(
            parse(log),
            [
                binary("tests/ir_runs.rs", 3, 1, 2),
                binary("unittests src/lib.rs", 12, 0, 0),
                binary("doc-tests capsulet_postgres", 0, 0, 0),
            ]
        );
    }

    #[test]
    fn a_failed_binary_is_still_counted() {
        let log = "     Running tests/x.rs (x)\ntest result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.00s\n";
        let counts = parse(log);
        assert_eq!(counts[0].passed, 4);
        assert_eq!(counts[0].failed, 1);
    }

    #[test]
    fn a_suite_that_runs_nothing_or_skips_tests_fails_a_gate_that_needs_them_all() {
        let command = |binaries| CommandCounts {
            command: "cargo test -p capsulet-postgres --test ir_runs --locked".to_string(),
            binaries,
        };
        assert!(
            require_every_test_ran(
                "postgres",
                &[command(vec![binary("tests/ir_runs.rs", 27, 0, 0)])]
            )
            .is_ok()
        );

        let none = require_every_test_ran("postgres", &[command(vec![])]).expect_err("no binary");
        assert!(none.contains("reported no test binary"), "{none}");
        let empty = require_every_test_ran(
            "postgres",
            &[command(vec![binary("tests/ir_runs.rs", 0, 0, 0)])],
        )
        .expect_err("ran nothing");
        assert!(
            empty.contains("ran no tests in tests/ir_runs.rs"),
            "{empty}"
        );
        let skipped = require_every_test_ran(
            "postgres",
            &[command(vec![binary("tests/ir_runs.rs", 26, 1, 0)])],
        )
        .expect_err("ignored one");
        assert!(skipped.contains("1 ignored, 0 filtered out"), "{skipped}");
    }
}
