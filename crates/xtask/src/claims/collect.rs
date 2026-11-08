//! Asks a gate command which tests it would run.
//!
//! Each runner is asked in the way it can answer. Cargo builds the command's
//! test targets with `--no-run` and every target lists its tests, so nothing
//! needs a database. Python's loader enumerates what `unittest discover` loads.
//! Node's test runner has no listing mode, so the dashboard suite is run with the
//! `junit` reporter, which names each test and its file. A PowerShell test script
//! is asked with `-List`, which it answers without running its cases.

use std::{
    collections::BTreeSet,
    env,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde_json::Value;

use super::render_command;
use crate::verify::{catalog::CommandSpec, process::platform_program};

/// A test a gate command collects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Collected {
    /// For a Rust test, its package's directory: a unit test can sit in any
    /// module under the package's `src/`, and libtest names the module, not the file.
    package: Option<String>,
    /// The file the runner attributes the test to; for Rust, the target's root.
    pub(crate) source: String,
    /// The runner's full name for the test.
    pub(crate) name: String,
    /// The last segment of that name, which is what evidence cites.
    leaf: String,
    /// Whether the command runs it, rather than collecting it as ignored or skipped.
    pub(crate) runs: bool,
}

impl Collected {
    pub(crate) fn rust(package: &str, target: &str, name: &str, runs: bool) -> Self {
        Self {
            package: Some(package.to_string()),
            source: target.to_string(),
            name: name.to_string(),
            leaf: name.rsplit("::").next().unwrap_or(name).to_string(),
            runs,
        }
    }

    pub(crate) fn file(source: &str, leaf: &str, name: &str, runs: bool) -> Self {
        Self {
            package: None,
            source: source.to_string(),
            name: name.to_string(),
            leaf: leaf.to_string(),
            runs,
        }
    }

    pub(crate) fn is_named(&self, selector: &str) -> bool {
        self.leaf == selector || self.name == selector
    }

    /// Whether the test can be the one declared in `path`.
    pub(crate) fn declared_in(&self, path: &str) -> bool {
        if path == self.source {
            return true;
        }
        let Some(package) = &self.package else {
            return false;
        };
        let sources = format!("{package}/src/");
        if path.starts_with(&sources) && self.source.starts_with(&sources) {
            return true;
        }
        // A module of an integration test, `tests/<target>/helpers.rs`, belongs
        // to the target `tests/<target>.rs` or `tests/<target>/main.rs`.
        let tests = format!("{package}/tests/");
        path.strip_prefix(&tests)
            .and_then(|rest| rest.split_once('/'))
            .is_some_and(|(target, _)| {
                self.source == format!("{tests}{target}.rs")
                    || self.source == format!("{tests}{target}/main.rs")
            })
    }
}

pub(crate) fn collect(root: &Path, command: &CommandSpec) -> Result<Vec<Collected>, String> {
    let arguments = command
        .arguments
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    match (command.program.as_str(), arguments.as_slice()) {
        ("cargo", ["test", rest @ ..]) => cargo(root, command, rest),
        ("python", ["-m", "unittest", "discover", rest @ ..]) => python(root, command, rest),
        ("npm", ["test"]) => node(root, command),
        ("powershell" | "pwsh", _) => powershell(root, command),
        _ => Err(format!(
            "`{}` is not a test command this check knows how to list",
            render_command(command)
        )),
    }
}

fn working_directory(root: &Path, command: &CommandSpec) -> PathBuf {
    command
        .working_directory
        .as_deref()
        .map_or_else(|| root.to_path_buf(), |directory| root.join(directory))
}

fn output(command: &mut Command, what: &str) -> Result<String, String> {
    let output = command
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("could not start {what}: {error}"))?;
    if !output.status.success() {
        return Err(format!("{what} exited with {}", output.status));
    }
    String::from_utf8(output.stdout).map_err(|error| format!("{what} printed non-UTF-8: {error}"))
}

/// A `cargo test` command line, split the way cargo splits it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CargoInvocation {
    /// Options for cargo itself.
    pub(crate) cargo: Vec<String>,
    /// Test name filters, which cargo hands to every test binary.
    pub(crate) filters: Vec<String>,
    /// Everything after `--`, for the test harness.
    pub(crate) harness: Vec<String>,
}

/// Cargo options that take the next word as their value.
const CARGO_VALUE_OPTIONS: &[&str] = &[
    "-p",
    "--package",
    "--exclude",
    "--test",
    "--bin",
    "--example",
    "--bench",
    "-F",
    "--features",
    "--target",
    "--target-dir",
    "--profile",
    "-j",
    "--jobs",
    "--manifest-path",
    "--color",
    "--config",
    "-Z",
];

impl CargoInvocation {
    pub(crate) fn parse(arguments: &[&str]) -> Self {
        let mut invocation = Self {
            cargo: Vec::new(),
            filters: Vec::new(),
            harness: Vec::new(),
        };
        let mut words = arguments.iter();
        while let Some(word) = words.next() {
            if *word == "--" {
                invocation
                    .harness
                    .extend(words.by_ref().map(|word| (*word).to_string()));
            } else if CARGO_VALUE_OPTIONS.contains(word) {
                invocation.cargo.push((*word).to_string());
                if let Some(value) = words.next() {
                    invocation.cargo.push((*value).to_string());
                }
            } else if word.starts_with('-') {
                invocation.cargo.push((*word).to_string());
            } else {
                invocation.filters.push((*word).to_string());
            }
        }
        invocation
    }

    /// Whether the harness arguments already decide which ignored tests run.
    fn chooses_ignored(&self) -> bool {
        self.harness
            .iter()
            .any(|word| word == "--ignored" || word == "--include-ignored")
    }
}

fn cargo(root: &Path, command: &CommandSpec, arguments: &[&str]) -> Result<Vec<Collected>, String> {
    let invocation = CargoInvocation::parse(arguments);
    let build = output(
        Command::new("cargo")
            .arg("test")
            .args(&invocation.cargo)
            .args(&invocation.filters)
            .args(["--no-run", "--message-format=json"])
            .envs(&command.environment)
            .current_dir(working_directory(root, command)),
        "cargo test --no-run",
    )?;
    let mut collected = Vec::new();
    for target in test_targets(root, &build)? {
        let all = list(&target.executable, &invocation, false)?;
        let ignored = if invocation.chooses_ignored() {
            BTreeSet::new()
        } else {
            list(&target.executable, &invocation, true)?
        };
        collected.extend(all.iter().map(|name| {
            Collected::rust(
                &target.package,
                &target.source,
                name,
                !ignored.contains(name),
            )
        }));
    }
    Ok(collected)
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TestTarget {
    pub(crate) package: String,
    pub(crate) source: String,
    pub(crate) executable: PathBuf,
}

/// Reads the test executables out of cargo's JSON build messages.
pub(crate) fn test_targets(root: &Path, messages: &str) -> Result<Vec<TestTarget>, String> {
    let mut targets = Vec::new();
    for line in messages.lines().filter(|line| line.starts_with('{')) {
        let message: Value = serde_json::from_str(line)
            .map_err(|error| format!("unreadable cargo message: {error}"))?;
        if message.get("reason").and_then(Value::as_str) != Some("compiler-artifact")
            || message.pointer("/profile/test").and_then(Value::as_bool) != Some(true)
        {
            continue;
        }
        let (Some(executable), Some(source), Some(manifest)) = (
            message.get("executable").and_then(Value::as_str),
            message.pointer("/target/src_path").and_then(Value::as_str),
            message.get("manifest_path").and_then(Value::as_str),
        ) else {
            continue;
        };
        let package = Path::new(manifest)
            .parent()
            .ok_or_else(|| format!("manifest path {manifest} has no directory"))?;
        targets.push(TestTarget {
            package: relative(root, package)?,
            source: relative(root, Path::new(source))?,
            executable: PathBuf::from(executable),
        });
    }
    Ok(targets)
}

/// A path under the workspace, spelled with forward slashes the way evidence spells it.
fn relative(root: &Path, path: &Path) -> Result<String, String> {
    let stripped = path
        .strip_prefix(root)
        .map(Path::to_path_buf)
        .or_else(|_| {
            let root = root.canonicalize().map_err(|error| error.to_string())?;
            let path = path.canonicalize().map_err(|error| error.to_string())?;
            path.strip_prefix(&root)
                .map(Path::to_path_buf)
                .map_err(|_| format!("{} is outside the workspace", path.display()))
        })?;
    Ok(stripped
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

fn list(
    executable: &Path,
    invocation: &CargoInvocation,
    ignored: bool,
) -> Result<BTreeSet<String>, String> {
    let mut command = Command::new(executable);
    command
        .args(&invocation.filters)
        .args(&invocation.harness)
        .arg("--list");
    if ignored {
        command.arg("--ignored");
    }
    let text = output(&mut command, &format!("{} --list", executable.display()))?;
    Ok(parse_libtest_list(&text))
}

/// Reads `name: test` lines from a libtest `--list`, skipping benchmarks and the summary.
pub(crate) fn parse_libtest_list(text: &str) -> BTreeSet<String> {
    text.lines()
        .filter_map(|line| line.trim_end().strip_suffix(": test"))
        .map(str::to_string)
        .collect()
}

/// Enumerates what `python -m unittest discover` loads, with the loader it uses.
const PYTHON_LISTING: &str = r#"
import inspect, json, sys, unittest
start, pattern, top = sys.argv[1], sys.argv[2], sys.argv[3] or None
def walk(suite):
    for test in suite:
        if isinstance(test, unittest.TestSuite):
            yield from walk(test)
        else:
            yield test
found = []
for test in walk(unittest.TestLoader().discover(start, pattern=pattern, top_level_dir=top)):
    if type(test).__module__ == "unittest.loader":
        sys.exit("unittest could not load " + test.id())
    method = getattr(test, test._testMethodName)
    skipped = getattr(type(test), "__unittest_skip__", False) or getattr(method, "__unittest_skip__", False)
    found.append({"id": test.id(), "file": inspect.getsourcefile(type(test)), "skipped": bool(skipped)})
print(json.dumps(found))
"#;

fn python(
    root: &Path,
    command: &CommandSpec,
    arguments: &[&str],
) -> Result<Vec<Collected>, String> {
    let (start, pattern, top) = discover_arguments(arguments);
    let directory = working_directory(root, command);
    let text = output(
        Command::new("python")
            .args(["-c", PYTHON_LISTING, &start, &pattern, &top])
            .envs(&command.environment)
            .current_dir(&directory),
        "the unittest loader",
    )?;
    let found: Vec<Value> = serde_json::from_str(text.trim())
        .map_err(|error| format!("unreadable unittest listing: {error}"))?;
    found
        .iter()
        .map(|test| {
            let id = test.get("id").and_then(Value::as_str).unwrap_or_default();
            let file = test.get("file").and_then(Value::as_str).unwrap_or_default();
            let skipped = test.get("skipped").and_then(Value::as_bool).unwrap_or(true);
            let leaf = id.rsplit('.').next().unwrap_or(id);
            let source = relative(root, &directory.join(file))?;
            Ok(Collected::file(&source, leaf, id, !skipped))
        })
        .collect()
}

/// The start directory, pattern and top-level directory `unittest discover` would use.
pub(crate) fn discover_arguments(arguments: &[&str]) -> (String, String, String) {
    let mut start = ".".to_string();
    let mut pattern = "test*.py".to_string();
    let mut top = String::new();
    let mut positional = Vec::new();
    let mut words = arguments.iter();
    while let Some(word) = words.next() {
        let slot = match *word {
            "-s" | "--start-directory" => &mut start,
            "-p" | "--pattern" => &mut pattern,
            "-t" | "--top-level-directory" => &mut top,
            flag if flag.starts_with('-') => continue,
            value => {
                positional.push((*value).to_string());
                continue;
            }
        };
        if let Some(value) = words.next() {
            *slot = (*value).to_string();
        }
    }
    for (slot, value) in [&mut start, &mut pattern, &mut top]
        .into_iter()
        .zip(positional)
    {
        *slot = value;
    }
    (start, pattern, top)
}

fn node(root: &Path, command: &CommandSpec) -> Result<Vec<Collected>, String> {
    // Node's runner has no listing mode, so the suite runs, and reports through
    // JUnit because that report names each test's file. The reporter is chosen
    // through NODE_OPTIONS so that the command run is still the gate's own.
    let options = env::var("NODE_OPTIONS").map_or_else(
        |_| "--test-reporter=junit".to_string(),
        |existing| format!("{existing} --test-reporter=junit"),
    );
    let text = output(
        Command::new(platform_program(&command.program))
            .args(&command.arguments)
            .envs(&command.environment)
            .env("NODE_OPTIONS", options)
            .current_dir(working_directory(root, command)),
        &format!("`{}`", render_command(command)),
    )?;
    parse_junit(&text)
        .into_iter()
        .map(|case| {
            let source = relative(root, Path::new(&case.file))?;
            Ok(Collected::file(
                &source,
                &case.name,
                &format!("{} > {}", case.suite, case.name),
                !case.skipped,
            ))
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct JunitCase {
    pub(crate) suite: String,
    pub(crate) name: String,
    pub(crate) file: String,
    pub(crate) skipped: bool,
}

/// Reads the `<testcase>` elements of a `junit` report.
///
/// Only the handful of attributes this check needs are read, which does not
/// warrant an XML parser as a dependency of the verifier.
pub(crate) fn parse_junit(xml: &str) -> Vec<JunitCase> {
    const OPEN: &str = "<testcase";
    const CLOSE: &str = "</testcase>";
    let mut cases = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(OPEN) {
        let after = &rest[start + OPEN.len()..];
        let Some(tag_end) = start_tag_end(after) else {
            break;
        };
        let tag = &after[..tag_end];
        let (body, consumed) = if tag.ends_with('/') {
            ("", tag_end + 1)
        } else {
            match after[tag_end..].find(CLOSE) {
                Some(close) => (
                    &after[tag_end + 1..tag_end + close],
                    tag_end + close + CLOSE.len(),
                ),
                None => break,
            }
        };
        cases.push(JunitCase {
            suite: attribute(tag, "classname").unwrap_or_default(),
            name: attribute(tag, "name").unwrap_or_default(),
            file: attribute(tag, "file").unwrap_or_default(),
            skipped: body.contains("<skipped"),
        });
        rest = &after[consumed..];
    }
    cases
}

/// Where a start tag ends, not counting a `>` inside a quoted attribute value.
fn start_tag_end(text: &str) -> Option<usize> {
    let mut quoted = false;
    for (index, character) in text.char_indices() {
        match character {
            '"' => quoted = !quoted,
            '>' if !quoted => return Some(index),
            _ => {}
        }
    }
    None
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let marker = format!(" {name}=\"");
    let start = tag.find(&marker)? + marker.len();
    let length = tag[start..].find('"')?;
    Some(unescape(&tag[start..start + length]))
}

fn unescape(text: &str) -> String {
    let mut unescaped = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        unescaped.push_str(&rest[..at]);
        let entity = &rest[at..];
        let Some(end) = entity.find(';') else {
            unescaped.push_str(entity);
            return unescaped;
        };
        let decoded = match &entity[1..end] {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            numeric => numeric
                .strip_prefix("#x")
                .map(|hex| u32::from_str_radix(hex, 16))
                .or_else(|| numeric.strip_prefix('#').map(str::parse))
                .and_then(Result::ok)
                .and_then(char::from_u32),
        };
        if let Some(character) = decoded {
            unescaped.push(character);
        } else {
            unescaped.push_str(&entity[..=end]);
        }
        rest = &entity[end + 1..];
    }
    unescaped.push_str(rest);
    unescaped
}

fn powershell(root: &Path, command: &CommandSpec) -> Result<Vec<Collected>, String> {
    let script = command
        .arguments
        .iter()
        .position(|word| word == "-File")
        .and_then(|at| command.arguments.get(at + 1))
        .ok_or("only a PowerShell script run with -File can list its cases")?;
    let text = output(
        Command::new(platform_program(&command.program))
            .args(&command.arguments)
            .arg("-List")
            .envs(&command.environment)
            .current_dir(working_directory(root, command)),
        &format!("{script} -List"),
    )?;
    let source = script.replace('\\', "/");
    Ok(parse_libtest_list(&text)
        .iter()
        .map(|name| Collected::file(&source, name, name, true))
        .collect())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        CargoInvocation, Collected, JunitCase, discover_arguments, parse_junit, parse_libtest_list,
        test_targets,
    };

    #[test]
    fn cargo_words_are_split_into_options_filters_and_harness_arguments() {
        let invocation = CargoInvocation::parse(&[
            "-p",
            "capsulet-api",
            "--lib",
            "authorization_",
            "--locked",
            "--",
            "--nocapture",
        ]);
        assert_eq!(
            invocation,
            CargoInvocation {
                cargo: ["-p", "capsulet-api", "--lib", "--locked"]
                    .map(str::to_string)
                    .to_vec(),
                filters: vec!["authorization_".to_string()],
                harness: vec!["--nocapture".to_string()],
            }
        );
        // A value is never mistaken for a filter.
        let invocation = CargoInvocation::parse(&["--test", "agent_runs", "--exclude", "x"]);
        assert!(invocation.filters.is_empty());
    }

    #[test]
    fn a_libtest_listing_yields_tests_and_not_benchmarks_or_the_summary() {
        let listing = "tests::one: test\nbench_it: bench\nplain: test\n\n2 tests, 1 benchmark\n";
        assert_eq!(
            parse_libtest_list(listing).into_iter().collect::<Vec<_>>(),
            ["plain", "tests::one"]
        );
    }

    #[test]
    fn test_targets_come_from_test_profile_artifacts_only() {
        let root = Path::new("/work");
        let messages = [
            r#"{"reason":"compiler-artifact","manifest_path":"/work/crates/a/Cargo.toml","target":{"src_path":"/work/crates/a/tests/flow.rs"},"profile":{"test":true},"executable":"/work/target/flow"}"#,
            r#"{"reason":"compiler-artifact","manifest_path":"/work/crates/a/Cargo.toml","target":{"src_path":"/work/crates/a/src/lib.rs"},"profile":{"test":false},"executable":null}"#,
            r#"{"reason":"build-finished","success":true}"#,
        ]
        .join("\n");
        let targets = test_targets(root, &messages).expect("targets");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].package, "crates/a");
        assert_eq!(targets[0].source, "crates/a/tests/flow.rs");
    }

    #[test]
    fn a_unit_test_belongs_to_any_file_under_its_packages_sources() {
        let test = Collected::rust("crates/a", "crates/a/src/lib.rs", "tests::works", true);
        assert!(test.is_named("works"));
        assert!(test.declared_in("crates/a/src/tests.rs"));
        assert!(!test.declared_in("crates/a/tests/flow.rs"));
        assert!(!test.declared_in("crates/b/src/tests.rs"));
        let integration = Collected::rust("crates/a", "crates/a/tests/flow.rs", "works", true);
        assert!(integration.declared_in("crates/a/tests/flow/helpers.rs"));
        assert!(!integration.declared_in("crates/a/src/tests.rs"));
    }

    #[test]
    fn a_prefix_of_a_test_name_does_not_name_it() {
        let test = Collected::file(
            "dashboard/tests/api.test.ts",
            "calls nested endpoints with fields",
            "helpers > calls nested endpoints with fields",
            true,
        );
        assert!(!test.is_named("calls nested endpoints"));
        assert!(test.is_named("calls nested endpoints with fields"));
    }

    #[test]
    fn unittest_discover_arguments_fall_back_to_the_loader_defaults() {
        assert_eq!(
            discover_arguments(&["-s", "sdk/python/tests", "-p", "test_*.py", "-v"]),
            (
                "sdk/python/tests".to_string(),
                "test_*.py".to_string(),
                String::new()
            )
        );
        assert_eq!(
            discover_arguments(&[]),
            (".".to_string(), "test*.py".to_string(), String::new())
        );
        assert_eq!(
            discover_arguments(&["tests", "check_*.py"]),
            ("tests".to_string(), "check_*.py".to_string(), String::new())
        );
    }

    #[test]
    fn junit_cases_carry_their_file_and_whether_they_were_skipped() {
        let report = r#"<?xml version="1.0"?>
<testsuites><testsuite name="helpers">
  <testcase name="formats &quot;sizes&quot; &gt; 1kB" classname="helpers" file="C:\w\a.test.ts"/>
  <testcase name="later" classname="helpers" file="/w/a.test.ts"><skipped type="skipped" message="todo"/></testcase>
</testsuite></testsuites>"#;
        assert_eq!(
            parse_junit(report),
            [
                JunitCase {
                    suite: "helpers".to_string(),
                    name: "formats \"sizes\" > 1kB".to_string(),
                    file: r"C:\w\a.test.ts".to_string(),
                    skipped: false,
                },
                JunitCase {
                    suite: "helpers".to_string(),
                    name: "later".to_string(),
                    file: "/w/a.test.ts".to_string(),
                    skipped: true,
                },
            ]
        );
    }
}
