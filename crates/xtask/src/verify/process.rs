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

/// What a tool says when asked to identify itself, or `None` when it cannot be run.
pub(crate) fn version_report(program: &str, arguments: &[&str]) -> Option<String> {
    let output = Command::new(platform_program(program))
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout).into_owned()
            + &String::from_utf8_lossy(&output.stderr)
    })
}

/// The first dotted version number in a tool's self-report.
///
/// Tools disagree about how to say it: `cargo 1.96.0 (…)`, `v26.8.1`,
/// `Docker version 29.6.2, build …`, `v4.1.4+g05fa379`, `Client Version:
/// v1.36.3`. Each is a word that is a version once a leading `v` is dropped and
/// whatever follows the digits and dots is ignored. A bare number such as a
/// year in a build date is not one: it has no dot.
pub(crate) fn parse_version(report: &str) -> Option<Vec<u64>> {
    report.split_whitespace().find_map(|word| {
        let word = word.strip_prefix('v').unwrap_or(word);
        let end = word
            .find(|character: char| !character.is_ascii_digit() && character != '.')
            .unwrap_or(word.len());
        let numeric = word[..end].trim_end_matches('.');
        if !numeric.contains('.') {
            return None;
        }
        numeric
            .split('.')
            .map(|part| part.parse().ok())
            .collect::<Option<Vec<u64>>>()
    })
}

/// Whether `found` is `minimum` or newer, comparing missing components as zero.
pub(crate) fn meets_minimum(found: &[u64], minimum: &[u64]) -> bool {
    let width = found.len().max(minimum.len());
    let padded = |version: &[u64]| {
        (0..width)
            .map(|index| version.get(index).copied().unwrap_or(0))
            .collect::<Vec<_>>()
    };
    padded(found) >= padded(minimum)
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

pub(crate) fn platform_program(program: &str) -> String {
    if cfg!(windows) && matches!(program, "npm" | "npx") {
        format!("{program}.cmd")
    } else {
        program.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{meets_minimum, parse_version};

    #[test]
    fn versions_are_read_from_each_tools_own_way_of_saying_it() {
        for (report, version) in [
            ("cargo 1.96.0 (30a34c682 2026-05-25)", vec![1, 96, 0]),
            ("v26.8.1\n", vec![26, 8, 1]),
            ("Python 3.14.7", vec![3, 14, 7]),
            ("Docker version 29.6.2, build dfc4efb", vec![29, 6, 2]),
            ("Docker Compose version v5.3.1", vec![5, 3, 1]),
            ("v4.1.4+g05fa379", vec![4, 1, 4]),
            ("kind version 0.31.0", vec![0, 31, 0]),
            (
                "Client Version: v1.36.3\nKustomize Version: v5.8.1",
                vec![1, 36, 3],
            ),
            ("cargo-deny 0.20.2", vec![0, 20, 2]),
            ("5.1.26100.7462", vec![5, 1, 26100, 7462]),
        ] {
            assert_eq!(parse_version(report), Some(version), "{report}");
        }
        assert_eq!(parse_version("built 2026 by nobody"), None);
    }

    #[test]
    fn a_minimum_is_met_by_itself_and_by_anything_newer() {
        assert!(meets_minimum(&[1, 96, 0], &[1, 96]));
        assert!(meets_minimum(&[3, 14, 7], &[3, 12]));
        assert!(meets_minimum(&[4, 1, 4], &[3, 18]));
        assert!(meets_minimum(&[20], &[20]));
        assert!(!meets_minimum(&[1, 95, 9], &[1, 96]));
        assert!(!meets_minimum(&[3, 11, 9], &[3, 12]));
        assert!(!meets_minimum(&[0, 29, 0], &[0, 30]));
    }
}
