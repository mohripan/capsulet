pub(crate) mod catalog;
mod compose;
mod counts;
mod kind;
mod postgres;
pub(crate) mod process;
mod summary;

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use catalog::{Gate, Profile, gates};

/// The variable that tells a gate's steps which verifier run they belong to.
pub(crate) const RUN_VARIABLE: &str = "CAPSULET_VERIFY_RUN";
use process::{ProcessFailure, run_command};

/// `xtask kind smoke|delete`.
pub(crate) fn kind(arguments: &[String]) -> Result<(), String> {
    kind::run(arguments)
}

/// `xtask stack up|down`.
pub(crate) fn stack(arguments: &[String]) -> Result<(), String> {
    compose::run(arguments)
}

pub(crate) fn run(arguments: &[String]) -> Result<(), String> {
    let Some(command) = arguments.first() else {
        return Err("usage: capsulet-xtask verify [options]".to_string());
    };
    if command != "verify" {
        return Err(format!("unknown xtask command: {command}"));
    }
    if arguments
        .get(1)
        .is_some_and(|argument| argument == "summary")
    {
        return summary::run(&arguments[2..]);
    }
    let options = Options::parse(&arguments[1..])?;
    if options.doctor {
        return doctor();
    }
    if options.list {
        return list_gates(options.format.as_deref());
    }
    execute(&options)
}

#[derive(Default)]
struct Options {
    profile: Option<Profile>,
    selected_gates: Vec<String>,
    skipped_gates: BTreeSet<String>,
    list: bool,
    doctor: bool,
    format: Option<String>,
}

impl Options {
    fn parse(arguments: &[String]) -> Result<Self, String> {
        let mut options = Self::default();
        let mut index = 0;
        while index < arguments.len() {
            match arguments[index].as_str() {
                "--profile" => {
                    options.profile = Some(Profile::parse(
                        arguments
                            .get(index + 1)
                            .ok_or("--profile requires a value")?,
                    )?);
                    index += 2;
                }
                "--gate" => {
                    options.selected_gates.push(
                        arguments
                            .get(index + 1)
                            .ok_or("--gate requires a value")?
                            .clone(),
                    );
                    index += 2;
                }
                "--skip" => {
                    options.skipped_gates.insert(
                        arguments
                            .get(index + 1)
                            .ok_or("--skip requires a value")?
                            .clone(),
                    );
                    index += 2;
                }
                "--list" => {
                    options.list = true;
                    index += 1;
                }
                "--format" => {
                    options.format = Some(
                        arguments
                            .get(index + 1)
                            .ok_or("--format requires a value")?
                            .clone(),
                    );
                    index += 2;
                }
                "doctor" => {
                    options.doctor = true;
                    index += 1;
                }
                other => return Err(format!("unknown verify option: {other}")),
            }
        }
        Ok(options)
    }
}

fn list_gates(format: Option<&str>) -> Result<(), String> {
    match format.unwrap_or("text") {
        "json" => println!(
            "{}",
            serde_json::to_string_pretty(&gates()).map_err(|error| error.to_string())?
        ),
        "text" => {
            for gate in gates() {
                println!("{:<16} {}", gate.name, gate.description);
            }
        }
        other => return Err(format!("unsupported list format: {other}")),
    }
    Ok(())
}

/// The tools the gates use and the oldest release of each the documentation
/// supports (`docs/contracts/verification-matrix.md`). A tool with no minimum is
/// only required to run: kubectl has to match its cluster, not a release of ours.
const SUPPORTED_TOOLS: &[(&str, &str, &[&str], Option<&str>)] = &[
    ("cargo", "cargo", &["--version"], Some("1.96")),
    ("docker", "docker", &["--version"], Some("27")),
    (
        "docker compose",
        "docker",
        &["compose", "version"],
        Some("2"),
    ),
    ("node", "node", &["--version"], Some("20")),
    ("npm", "npm", &["--version"], Some("10")),
    ("python", "python", &["--version"], Some("3.12")),
    ("helm", "helm", &["version", "--short"], Some("3.18")),
    ("kind", "kind", &["--version"], Some("0.30")),
    ("kubectl", "kubectl", &["version", "--client"], None),
    // Only required to run: the release gates read the commit an image is built from.
    ("git", "git", &["--version"], None),
    // The versions CI pins in .github/workflows/ci.yml.
    ("cargo-audit", "cargo-audit", &["--version"], Some("0.22.2")),
    ("cargo-deny", "cargo-deny", &["--version"], Some("0.20.2")),
];

/// Reports every tool's version against its supported minimum, changing nothing.
fn doctor() -> Result<(), String> {
    let powershell = if cfg!(windows) { "powershell" } else { "pwsh" };
    let powershell_probe = process::version_arguments(powershell);
    let tools = SUPPORTED_TOOLS.iter().copied().chain(std::iter::once((
        powershell,
        powershell,
        powershell_probe.as_slice(),
        None,
    )));
    let mut problems = Vec::new();
    for (label, program, arguments, minimum) in tools {
        let Some(report) = process::version_report(program, arguments) else {
            println!("missing {label}");
            problems.push(format!("{label} is missing"));
            continue;
        };
        let found = process::parse_version(&report);
        let shown = found.as_ref().map_or_else(
            || "unknown version".to_string(),
            |version| {
                version
                    .iter()
                    .map(u64::to_string)
                    .collect::<Vec<_>>()
                    .join(".")
            },
        );
        let Some(minimum) = minimum else {
            println!("ok      {label:<15} {shown}");
            continue;
        };
        let required = process::parse_version(&format!("{minimum}.0"))
            .expect("supported minimums are dotted versions");
        match found {
            Some(version) if process::meets_minimum(&version, &required) => {
                println!("ok      {label:<15} {shown} (needs {minimum} or newer)");
            }
            Some(_) => {
                println!("old     {label:<15} {shown} (needs {minimum} or newer)");
                problems.push(format!("{label} {shown} is older than {minimum}"));
            }
            None => {
                println!("unknown {label:<15} (needs {minimum} or newer)");
                problems.push(format!("{label} did not report a version"));
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("unsupported toolchain: {}", problems.join("; ")))
    }
}

fn execute(options: &Options) -> Result<(), String> {
    process::install_interrupt_handler()?;
    let catalog = gates();
    let selected = select_gates(&catalog, options)?;
    let selected_names = selected
        .iter()
        .map(|gate| gate.name)
        .collect::<BTreeSet<_>>();
    for skipped in &options.skipped_gates {
        if selected_names.contains(skipped.as_str()) {
            return Err(format!("cannot skip required gate: {skipped}"));
        }
    }
    if options.profile == Some(Profile::Fast) {
        print_omitted(&catalog, &selected_names, "start");
    }
    let root = workspace_root();
    let log_directory = log_directory(&root)?;
    let mut passed = Vec::new();
    let mut ran = Vec::new();
    let mut records = Vec::new();
    let result = run_selected(
        &selected,
        &root,
        &log_directory,
        &mut passed,
        &mut ran,
        &mut records,
    );
    publish_summary(&ran);
    result?;
    if options.profile == Some(Profile::Fast) {
        print_omitted(&catalog, &selected_names, "end");
    }
    println!(
        "[verify] passed {} required gates: {}",
        passed.len(),
        passed.join(", ")
    );
    Ok(())
}

/// Runs the selected gates in order, stopping at the first that fails.
fn run_selected(
    selected: &[&Gate],
    root: &Path,
    log_directory: &Path,
    passed: &mut Vec<&'static str>,
    ran: &mut Vec<counts::GateCounts>,
    records: &mut Vec<summary::GateRecord>,
) -> Result<(), String> {
    for gate in selected {
        println!("[verify] {:<16} running", gate.name);
        let log_path = log_directory.join(format!("{}.log", gate.name));
        if let Err(missing) = check_prerequisites(gate) {
            records.push(summary::GateRecord::new(
                gate,
                "missing_prerequisite",
                0.0,
                &[],
                &log_path,
            ));
            summary::write(log_directory, records)?;
            return Err(missing);
        }
        let started = Instant::now();
        let mut commands = Vec::new();
        let outcome = run_gate(gate, root, &log_path, &mut commands);
        let status = match &outcome {
            Ok(()) => "passed",
            Err(ProcessFailure::Failed) => "failed",
            Err(ProcessFailure::TimedOut) => "timed_out",
            Err(ProcessFailure::Io(_)) => "error",
        };
        let mut record = summary::GateRecord::new(
            gate,
            status,
            started.elapsed().as_secs_f64(),
            &commands,
            &log_path,
        );
        // A gate whose every test must run fails below even though its commands
        // passed; the record must not say otherwise.
        let incomplete = outcome.is_ok()
            && gate.every_test_runs
            && counts::require_every_test_ran(gate.name, &commands).is_err();
        if incomplete {
            record.status = "failed".to_string();
        }
        records.push(record);
        summary::write(log_directory, records)?;
        ran.push(counts::GateCounts {
            gate: gate.name,
            passed: outcome.is_ok(),
            every_test_runs: gate.every_test_runs,
            commands,
        });
        write_counts(log_directory, ran)?;
        if let Err(failure) = outcome {
            // Print the tail before returning. A gate that fails on another
            // machine and only says "see the log" is a gate whose reason
            // nobody ever reads: the log is on the runner, and the runner is
            // gone by the time anybody looks.
            print_log_tail(&log_path);
            let reason = match failure {
                ProcessFailure::Failed => "failed".to_string(),
                ProcessFailure::TimedOut => "timed out".to_string(),
                ProcessFailure::Io(error) => error,
            };
            return Err(format!(
                "{}: {reason} (log: {})",
                gate.name,
                log_path.display()
            ));
        }
        let commands = &ran.last().expect("this gate was recorded").commands;
        if gate.every_test_runs {
            counts::require_every_test_ran(gate.name, commands)?;
        }
        println!("[verify] {:<16} passed{}", gate.name, tally(commands));
        passed.push(gate.name);
    }
    Ok(())
}

/// ` (3 binaries, 104 tests)`, or nothing for a gate that ran no tests.
fn tally(commands: &[counts::CommandCounts]) -> String {
    let binaries = commands
        .iter()
        .map(|command| command.binaries.len())
        .sum::<usize>();
    let tests = commands
        .iter()
        .flat_map(|command| &command.binaries)
        .map(|binary| binary.passed)
        .sum::<u64>();
    if binaries == 0 {
        String::new()
    } else {
        format!(" ({binaries} test binaries, {tests} tests passed)")
    }
}

/// Writes what every gate so far ran, so a run that fails still says what ran.
fn write_counts(log_directory: &Path, ran: &[counts::GateCounts]) -> Result<(), String> {
    let path = log_directory.join("test-counts.json");
    let json = serde_json::to_string_pretty(ran).map_err(|error| error.to_string())?;
    fs::write(&path, json + "\n")
        .map_err(|error| format!("could not write {}: {error}", path.display()))
}

/// Adds the counts to a GitHub Actions job summary when there is one.
fn publish_summary(ran: &[counts::GateCounts]) {
    let Some(path) = env::var_os("GITHUB_STEP_SUMMARY") else {
        return;
    };
    if ran.iter().all(|gate| {
        gate.commands
            .iter()
            .all(|command| command.binaries.is_empty())
    }) {
        return;
    }
    let table = format!("### Tests the verifier ran\n\n{}\n", counts::markdown(ran));
    let written = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut file| std::io::Write::write_all(&mut file, table.as_bytes()));
    if let Err(error) = written {
        eprintln!("[verify] could not write the job summary: {error}");
    }
}

/// How much of a failing gate's log to print.
///
/// Enough to carry a test failure and its assertion message, short enough that
/// a CI log stays readable.
const LOG_TAIL_LINES: usize = 60;

/// Prints the end of a failing gate's log.
fn print_log_tail(log_path: &Path) {
    let Ok(contents) = fs::read_to_string(log_path) else {
        return;
    };
    let lines: Vec<&str> = contents.lines().collect();
    let skipped = lines.len().saturating_sub(LOG_TAIL_LINES);
    if skipped > 0 {
        println!("[verify] ... {skipped} earlier lines omitted ...");
    }
    for line in lines.iter().skip(skipped) {
        println!("[verify] | {line}");
    }
}

fn run_gate(
    gate: &Gate,
    root: &Path,
    log_path: &Path,
    recorded: &mut Vec<counts::CommandCounts>,
) -> Result<(), ProcessFailure> {
    let _cleanup = CleanupMarker::from_environment();
    // Every step learns which verifier run it belongs to, so a gate that makes
    // something named for the run (the Kind cluster) can find exactly that
    // again in its teardown, and nothing another run made.
    let run_id = log_path
        .parent()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let with_run = |specification: &catalog::CommandSpec| {
        let mut specification = specification.clone();
        specification
            .environment
            .insert(RUN_VARIABLE.to_string(), run_id.clone());
        specification
    };
    let provision: Vec<catalog::CommandSpec> = gate.provision.iter().map(with_run).collect();
    let teardown_steps: Vec<catalog::CommandSpec> = gate.teardown.iter().map(with_run).collect();
    let mut fixture = if matches!(gate.name, "postgres" | "migrations" | "chaos") {
        Some(postgres::PostgresFixture::start().map_err(ProcessFailure::Io)?)
    } else {
        None
    };
    let mut commands: Vec<catalog::CommandSpec> = effective_commands(gate)
        .map_err(ProcessFailure::Io)?
        .iter()
        .map(with_run)
        .collect();
    let mut stack = match gate.stack {
        Some(services) => Some(
            compose::ComposeStack::for_gate(
                gate.name,
                services,
                root,
                log_path,
                gate.timeout_seconds,
            )
            .map_err(ProcessFailure::Io)?,
        ),
        None => None,
    };
    if let Some(stack) = &stack {
        for command in &mut commands {
            command.environment.extend(stack.environment());
        }
    }
    if let Some(database) = &fixture {
        for command in &mut commands {
            command.environment.insert(
                "CAPSULET_TEST_DATABASE_URL".to_string(),
                database.database_url().to_string(),
            );
            command.environment.insert(
                "DATABASE_URL".to_string(),
                database.database_url().to_string(),
            );
        }
    }
    let outcome = run_steps(&provision, gate, root, log_path)
        .and_then(|()| run_counted_steps(&commands, gate, root, log_path, recorded));
    let teardown = run_steps(&teardown_steps, gate, root, log_path);
    let stack_cleanup = stack.as_mut().map_or(Ok(()), |stack| {
        stack
            .finish(outcome.is_ok(), root, log_path, gate.timeout_seconds)
            .map_err(ProcessFailure::Io)
    });
    let database_cleanup = fixture.as_mut().map_or(Ok(()), |database| {
        database.cleanup().map_err(ProcessFailure::Io)
    });
    let simulated_cleanup = if env::var_os("CAPSULET_VERIFY_TEST_CLEANUP_FAIL").is_some() {
        Err(ProcessFailure::Io("cleanup failed".to_string()))
    } else {
        Ok(())
    };
    let cleanup = teardown
        .and(stack_cleanup)
        .and(database_cleanup)
        .and(simulated_cleanup);
    match cleanup {
        Ok(()) => outcome,
        Err(error) => Err(error),
    }
}

fn run_steps(
    commands: &[catalog::CommandSpec],
    gate: &Gate,
    root: &Path,
    log_path: &Path,
) -> Result<(), ProcessFailure> {
    for command in commands {
        run_command(command, gate.timeout_seconds, root, log_path)?;
    }
    Ok(())
}

/// Runs a gate's commands, recording the test summaries each one wrote.
///
/// A command's share of the log is what it appended, so its counts are read from
/// there, and recorded even when it fails.
fn run_counted_steps(
    commands: &[catalog::CommandSpec],
    gate: &Gate,
    root: &Path,
    log_path: &Path,
    recorded: &mut Vec<counts::CommandCounts>,
) -> Result<(), ProcessFailure> {
    for command in commands {
        let start = fs::metadata(log_path).map_or(0, |metadata| metadata.len());
        let outcome = run_command(command, gate.timeout_seconds, root, log_path);
        let written = fs::read(log_path).unwrap_or_default();
        let segment = written
            .get(usize::try_from(start).unwrap_or(usize::MAX)..)
            .unwrap_or_default();
        recorded.push(counts::CommandCounts {
            command: crate::claims::render_command(command),
            binaries: counts::parse(&String::from_utf8_lossy(segment)),
        });
        outcome?;
    }
    Ok(())
}

fn select_gates<'a>(catalog: &'a [Gate], options: &Options) -> Result<Vec<&'a Gate>, String> {
    if !options.selected_gates.is_empty() && options.profile.is_some() {
        return Err("choose either --profile or --gate, not both".to_string());
    }
    if !options.selected_gates.is_empty() {
        return options
            .selected_gates
            .iter()
            .map(|name| {
                catalog
                    .iter()
                    .find(|gate| gate.name == name)
                    .ok_or_else(|| format!("unknown verification gate: {name}"))
            })
            .collect();
    }
    let profile = options.profile.unwrap_or(Profile::Fast);
    Ok(catalog
        .iter()
        .filter(|gate| gate.profiles.contains(&profile))
        .collect())
}

fn check_prerequisites(gate: &Gate) -> Result<(), String> {
    let forced = env::var("CAPSULET_VERIFY_FORCE_MISSING").ok();
    for prerequisite in &gate.prerequisites {
        if forced.as_deref() == Some(prerequisite.as_str()) || !process::is_available(prerequisite)
        {
            return Err(format!(
                "{}: missing prerequisite: {prerequisite}",
                gate.name
            ));
        }
    }
    Ok(())
}

fn effective_commands(gate: &Gate) -> Result<Vec<catalog::CommandSpec>, String> {
    match env::var("CAPSULET_VERIFY_TEST_COMMAND").ok().as_deref() {
        Some(mode @ ("fail" | "sleep" | "report")) => Ok(vec![catalog::CommandSpec {
            program: env::current_exe()
                .map_err(|error| error.to_string())?
                .display()
                .to_string(),
            arguments: vec!["__test-child".to_string(), mode.to_string()],
            working_directory: None,
            environment: BTreeMap::default(),
        }]),
        Some(other) => Err(format!("unknown test command mode: {other}")),
        None => Ok(gate.commands.clone()),
    }
}

fn print_omitted(catalog: &[Gate], selected: &BTreeSet<&str>, position: &str) {
    let omitted = catalog
        .iter()
        .filter(|gate| gate.profiles.contains(&Profile::Full) && !selected.contains(gate.name))
        .map(|gate| gate.name)
        .collect::<Vec<_>>();
    println!(
        "[verify] fast profile omitted ({position}): {}",
        omitted.join(", ")
    );
}

pub(crate) fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("xtask is nested under crates")
        .to_path_buf()
}

fn log_directory(root: &Path) -> Result<PathBuf, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis();
    let directory = root
        .join(".capsulet")
        .join("verify")
        .join(format!("{timestamp}-{}", std::process::id()));
    fs::create_dir_all(root.join(".capsulet").join("verify")).map_err(|error| error.to_string())?;
    // Never append a new run's evidence to an existing run, even on a naming collision.
    fs::create_dir(&directory).map_err(|error| {
        format!(
            "could not create isolated verification logs at {}: {error}",
            directory.display()
        )
    })?;
    Ok(directory)
}

struct CleanupMarker(Option<PathBuf>);
impl CleanupMarker {
    fn from_environment() -> Self {
        Self(env::var_os("CAPSULET_VERIFY_TEST_CLEANUP_MARKER").map(PathBuf::from))
    }
}
impl Drop for CleanupMarker {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = fs::write(path, b"cleanup completed\n");
        }
    }
}
