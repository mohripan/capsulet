use std::{
    env,
    fs::{File, OpenOptions},
    path::Path,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use super::catalog::CommandSpec;

pub(crate) enum ProcessFailure {
    Failed,
    TimedOut,
    Io(String),
}

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

pub(crate) fn install_interrupt_handler() -> Result<(), String> {
    INTERRUPTED.store(false, Ordering::SeqCst);
    ctrlc::set_handler(|| INTERRUPTED.store(true, Ordering::SeqCst))
        .map_err(|error| format!("could not install interrupt handler: {error}"))
}

/// How to ask a tool to identify itself.
///
/// `--version` is not universal, and guessing wrong is worse than it sounds:
/// `helm --version` and `kubectl --version` both exit non-zero with "unknown
/// flag", so probing that way reports an installed tool as missing, tells an
/// operator to install what they already have, and stops the full profile at a
/// gate that could have run.
pub(crate) fn version_arguments(program: &str) -> Vec<&'static str> {
    match program {
        "powershell" | "pwsh" => vec![
            "-NoProfile",
            "-Command",
            "$PSVersionTable.PSVersion.ToString()",
        ],
        // Helm and kubectl take a `version` subcommand and reject `--version`.
        // `--client` keeps the kubectl probe from reaching for a cluster that
        // availability has nothing to do with.
        "helm" => vec!["version", "--short"],
        "kubectl" => vec!["version", "--client"],
        _ => vec!["--version"],
    }
}

pub(crate) fn is_available(program: &str) -> bool {
    Command::new(platform_program(program))
        .args(version_arguments(program))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub(crate) fn run_command(
    specification: &CommandSpec,
    timeout_seconds: u64,
    workspace_root: &Path,
    log_path: &Path,
) -> Result<(), ProcessFailure> {
    let timeout = env::var("CAPSULET_VERIFY_TEST_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map_or_else(
            || Duration::from_secs(timeout_seconds),
            Duration::from_millis,
        );
    let log = open_log(log_path)?;
    let error_log = log
        .try_clone()
        .map_err(|error| ProcessFailure::Io(error.to_string()))?;
    let working_directory = specification.working_directory.as_deref().map_or_else(
        || workspace_root.to_path_buf(),
        |path| workspace_root.join(path),
    );
    let mut child = Command::new(platform_program(&specification.program))
        .args(&specification.arguments)
        .envs(&specification.environment)
        .current_dir(working_directory)
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(error_log))
        .spawn()
        .map_err(|error| ProcessFailure::Io(error.to_string()))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => return Err(ProcessFailure::Failed),
            Ok(None) if started.elapsed() >= timeout => {
                child
                    .kill()
                    .map_err(|error| ProcessFailure::Io(error.to_string()))?;
                let _ = child.wait();
                return Err(ProcessFailure::TimedOut);
            }
            Ok(None) if INTERRUPTED.load(Ordering::SeqCst) => {
                child
                    .kill()
                    .map_err(|error| ProcessFailure::Io(error.to_string()))?;
                let _ = child.wait();
                return Err(ProcessFailure::Io("interrupted".to_string()));
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(error) => return Err(ProcessFailure::Io(error.to_string())),
        }
    }
}

fn open_log(path: &Path) -> Result<File, ProcessFailure> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| ProcessFailure::Io(error.to_string()))
}

fn platform_program(program: &str) -> String {
    if cfg!(windows) && matches!(program, "npm" | "npx") {
        format!("{program}.cmd")
    } else {
        program.to_string()
    }
}
