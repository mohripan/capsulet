//! What a run made and should have removed. Everything disposable a gate
//! creates is named with [`prefix`], so after the last gate the verifier can
//! look for exactly this run's resources, and nothing another run made:
//!
//! - the database fixture's container (`<prefix>-postgres-<gate>`), started
//!   with its volume removed alongside it;
//! - each Compose stack's project (`<prefix>-<gate>`): its containers, volumes,
//!   networks and the images it built, and its directory of generated
//!   credentials under `.capsulet/stack`;
//! - the Kind cluster (`<prefix>`), its node container and network, and its
//!   directory of credentials and kubeconfig under `.capsulet/kind`;
//! - the images the `containers` and `kind` gates build
//!   (`capsulet-verify/<image>:<version>-<run>-<pid>`).

use std::{fs, path::Path, process::Command};

/// The name every disposable resource of run `run` starts with.
pub(crate) fn prefix(run: &str) -> String {
    format!("capsulet-verify-{run}")
}

#[derive(Debug, Default)]
pub(crate) struct Report {
    /// What was looked at: `docker`, `kind`, `files`. A tool that is not
    /// installed made nothing, so it is left out rather than failing the run.
    pub(crate) checked: Vec<String>,
    pub(crate) found: Vec<String>,
}

pub(crate) fn find(root: &Path, run: &str) -> Report {
    let prefix = prefix(run);
    let mut report = Report::default();
    let docker = [
        ("container", vec!["ps", "--all", "--format", "{{.Names}}"]),
        ("volume", vec!["volume", "ls", "--format", "{{.Name}}"]),
        ("network", vec!["network", "ls", "--format", "{{.Name}}"]),
        (
            "image",
            vec!["image", "ls", "--format", "{{.Repository}}:{{.Tag}}"],
        ),
    ];
    let mut docker_answered = false;
    for (kind, arguments) in docker {
        let Some(names) = lines("docker", &arguments) else {
            continue;
        };
        docker_answered = true;
        for name in names {
            let ours = named_for(&name, &prefix)
                || (kind == "image"
                    && name.starts_with("capsulet-verify/")
                    && name.contains(&format!("-{run}-")));
            if ours {
                report.found.push(format!("{kind} {name}"));
            }
        }
    }
    if docker_answered {
        report.checked.push("docker".to_string());
    }
    if let Some(clusters) = lines("kind", &["get", "clusters"]) {
        report.checked.push("kind".to_string());
        for cluster in clusters.into_iter().filter(|name| named_for(name, &prefix)) {
            report.found.push(format!("Kind cluster {cluster}"));
        }
    }
    report.checked.push("files".to_string());
    for directory in [".capsulet/stack", ".capsulet/kind"] {
        let Ok(entries) = fs::read_dir(root.join(directory)) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if named_for(&name, &prefix) {
                report.found.push(format!("{directory}/{name}"));
            }
        }
    }
    report.found.sort();
    report
}

/// `prefix` itself, or a name that continues it after a separator, so run
/// `...-4242` does not claim what run `...-42421` made.
fn named_for(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(['-', '_']))
}

/// The lines `program` printed, or `None` when it could not run or failed:
/// Docker without a daemon has nothing of this run's either.
fn lines(program: &str, arguments: &[&str]) -> Option<Vec<String>> {
    let output = Command::new(program).args(arguments).output().ok()?;
    output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::{find, named_for, prefix};

    #[test]
    fn a_name_belongs_to_a_run_only_up_to_a_separator() {
        let prefix = prefix("1700000000000-4242");
        for ours in [
            "capsulet-verify-1700000000000-4242",
            "capsulet-verify-1700000000000-4242-control-plane",
            "capsulet-verify-1700000000000-4242-postgres-chaos",
            "capsulet-verify-1700000000000-4242-compose_default",
        ] {
            assert!(named_for(ours, &prefix), "{ours}");
        }
        for theirs in [
            "capsulet-verify-1700000000000-42421",
            "capsulet-verify-1700000000000-42421-compose_default",
            "capsulet-local-postgres-1",
        ] {
            assert!(!named_for(theirs, &prefix), "{theirs}");
        }
    }

    #[test]
    fn a_directory_named_for_the_run_is_left_over_and_another_runs_is_not() {
        let directory = tempfile::tempdir().expect("temp directory");
        let root = directory.path();
        let run = "1700000000000-4242";
        std::fs::create_dir_all(
            root.join(".capsulet/stack")
                .join(format!("{}-compose", prefix(run))),
        )
        .expect("stack directory");
        std::fs::create_dir_all(
            root.join(".capsulet/kind")
                .join(prefix("1700000000000-42421")),
        )
        .expect("another run's cluster directory");
        let report = find(root, run);
        assert!(report.checked.contains(&"files".to_string()));
        let files: Vec<&String> = report
            .found
            .iter()
            .filter(|found| found.starts_with(".capsulet/"))
            .collect();
        assert_eq!(
            files,
            vec![&format!(".capsulet/stack/{}-compose", prefix(run))]
        );
    }
}
