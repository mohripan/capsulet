mod catalog;
mod process;

use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use catalog::{Gate, Profile, gates};
use process::{ProcessFailure, run_command};

pub(crate) fn run(arguments: &[String]) -> Result<(), String> {
    let Some(command) = arguments.first() else {
        return Err("usage: capsulet-xtask verify [options]".to_string());
    };
    if command != "verify" {
        return Err(format!("unknown xtask command: {command}"));
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

fn doctor() -> Result<(), String> {
    let tools = [
        "cargo",
        "docker",
        "node",
        "npm",
        "python",
        "helm",
        "kind",
        "kubectl",
        "cargo-audit",
    ];
    let mut missing = Vec::new();
    for tool in tools {
        if process::is_available(tool) {
            println!("ok      {tool}");
        } else {
            println!("missing {tool}");
            missing.push(tool);
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("missing prerequisites: {}", missing.join(", ")))
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
    for gate in selected {
        println!("[verify] {:<16} running", gate.name);
        check_prerequisites(gate)?;
        let log_path = log_directory.join(format!("{}.log", gate.name));
        match run_gate(gate, &root, &log_path) {
            Ok(()) => {}
            Err(ProcessFailure::Failed) => {
                return Err(format!(
                    "{}: failed (log: {})",
                    gate.name,
                    log_path.display()
                ));
            }
            Err(ProcessFailure::TimedOut) => {
                return Err(format!(
                    "{}: timed out (log: {})",
                    gate.name,
                    log_path.display()
                ));
            }
            Err(ProcessFailure::Io(error)) => {
                return Err(format!(
                    "{}: {error} (log: {})",
                    gate.name,
                    log_path.display()
                ));
            }
        }
        println!("[verify] {:<16} passed", gate.name);
        passed.push(gate.name);
    }
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

fn run_gate(gate: &Gate, root: &Path, log_path: &Path) -> Result<(), ProcessFailure> {
    let _cleanup = CleanupMarker::from_environment();
    let commands = effective_commands(gate).map_err(ProcessFailure::Io)?;
    let outcome = run_steps(&gate.provision, gate, root, log_path)
        .and_then(|()| run_steps(&commands, gate, root, log_path));
    let teardown = run_steps(&gate.teardown, gate, root, log_path);
    outcome.and(teardown)
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
        Some(mode @ ("fail" | "sleep")) => Ok(vec![catalog::CommandSpec {
            program: env::current_exe()
                .map_err(|error| error.to_string())?
                .display()
                .to_string(),
            arguments: vec!["__test-child".to_string(), mode.to_string()],
            working_directory: None,
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

fn workspace_root() -> PathBuf {
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
        .as_secs();
    let directory = root
        .join(".capsulet")
        .join("verify")
        .join(timestamp.to_string());
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
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
