//! A Compose stack of its own for one gate, or for a developer.
//!
//! A gate's stack has a project name nobody else uses, ports nothing else is
//! listening on and secrets generated for it alone. It is seeded with the
//! fixture twice, and the two seeds must agree; after the gate's commands pass
//! it is seeded again and must still agree. Teardown removes exactly what
//! Compose labelled with that project name: containers, networks, volumes and
//! the images it built, and then the secrets.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

use super::{
    catalog::CommandSpec,
    process::{ProcessFailure, run_command},
};
use crate::fixtures::{self, Api, Ports, Secrets};

/// Where the stack's secrets live while it is up.
const STACK_DIRECTORY: &str = ".capsulet/stack";

/// The project name of a developer's own stack.
const LOCAL_PROJECT: &str = "capsulet-local";

/// Which stack to start: its name, where its secrets go, and what they are.
struct Plan {
    project: String,
    directory: PathBuf,
    secrets: Secrets,
    ports: Ports,
}

pub(crate) struct ComposeStack {
    project: String,
    directory: PathBuf,
    secrets: Secrets,
    ports: Ports,
    api: Api,
    service_token: String,
    baseline: Value,
    removed: bool,
}

impl ComposeStack {
    /// Brings up a fresh stack for `gate`, waits for every service's health
    /// check, and seeds it.
    ///
    /// With `services` empty, every service starts; otherwise those and what
    /// they depend on.
    pub(crate) fn for_gate(
        gate: &str,
        services: &[&str],
        root: &Path,
        log_path: &Path,
        timeout_seconds: u64,
    ) -> Result<Self, String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?;
        let project = format!(
            "capsulet-verify-{gate}-{}-{}",
            std::process::id(),
            now.as_millis()
        );
        let plan = Plan {
            directory: root.join(STACK_DIRECTORY).join(&project),
            project,
            secrets: Secrets::generate(now.as_secs())?,
            ports: Ports::free()?,
        };
        Self::start(plan, services, root, log_path, timeout_seconds)
    }

    fn start(
        plan: Plan,
        services: &[&str],
        root: &Path,
        log_path: &Path,
        timeout_seconds: u64,
    ) -> Result<Self, String> {
        fs::create_dir_all(&plan.directory)
            .map_err(|error| format!("could not create {}: {error}", plan.directory.display()))?;
        let mut stack = Self {
            api: Api::new(plan.ports.api_url())?,
            project: plan.project,
            directory: plan.directory,
            secrets: plan.secrets,
            ports: plan.ports,
            service_token: String::new(),
            baseline: Value::Null,
            removed: false,
        };
        // From here on, a failure still takes the stack down: `Drop` does that.
        fs::write(stack.env_file(), stack.secrets.env_file(stack.ports))
            .map_err(|error| format!("could not write the stack's environment file: {error}"))?;
        fs::write(
            stack.directory.join("database-url"),
            stack.secrets.database_url(stack.ports),
        )
        .map_err(|error| format!("could not write the stack's database URL: {error}"))?;
        let mut up = vec![
            "up",
            "--detach",
            "--build",
            "--wait",
            "--wait-timeout",
            "900",
        ];
        up.extend_from_slice(services);
        if let Err(failure) = stack.compose(&up, root, log_path, timeout_seconds) {
            stack.collect_logs(root, log_path);
            return Err(format!("the stack did not come up healthy: {failure}"));
        }
        match stack.seed_twice(root) {
            Ok(()) => Ok(stack),
            Err(error) => {
                stack.collect_logs(root, log_path);
                Err(error)
            }
        }
    }

    /// Seeds the stack twice and records the fixture graph, which the second
    /// seed must leave exactly as the first did.
    fn seed_twice(&mut self, root: &Path) -> Result<(), String> {
        if !self.runs_api(root)? {
            // Only data services, for running the API itself from cargo: there is
            // nothing to seed through.
            return Ok(());
        }
        self.api.wait_until_ready(60)?;
        let first = self.seed_and_snapshot()?;
        let second = self.seed_and_snapshot()?;
        if first != second {
            return Err(disagreement("seeding a seeded stack", &first, &second));
        }
        self.baseline = second;
        Ok(())
    }

    /// What every command of the gate needs: the stack's Compose project, and
    /// the fixture file with each identity's credentials.
    pub(crate) fn environment(&self) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("COMPOSE_PROJECT_NAME".to_string(), self.project.clone()),
            (
                "COMPOSE_ENV_FILES".to_string(),
                self.env_file().display().to_string(),
            ),
            (
                "CAPSULET_FIXTURE_FILE".to_string(),
                self.fixture_file().display().to_string(),
            ),
            ("CAPSULET_API_URL".to_string(), self.ports.api_url()),
            (
                "PLAYWRIGHT_BASE_URL".to_string(),
                self.ports.dashboard_url(),
            ),
        ])
    }

    /// Checks that the gate's commands left the fixture as seeded, collects the
    /// services' logs when anything failed, and takes the stack down.
    pub(crate) fn finish(
        &mut self,
        commands_passed: bool,
        root: &Path,
        log_path: &Path,
        timeout_seconds: u64,
    ) -> Result<(), String> {
        let unchanged = if commands_passed {
            self.seed_and_snapshot().and_then(|after| {
                if after == self.baseline {
                    Ok(())
                } else {
                    Err(disagreement(
                        "seeding after the gate's commands",
                        &self.baseline,
                        &after,
                    ))
                }
            })
        } else {
            Ok(())
        };
        if !commands_passed || unchanged.is_err() {
            self.collect_logs(root, log_path);
        }
        let down = self.down(root, log_path, timeout_seconds);
        unchanged.and(down)
    }

    fn seed_and_snapshot(&mut self) -> Result<Value, String> {
        self.service_token = fixtures::seed(&self.api, &self.secrets)?;
        fs::write(
            self.fixture_file(),
            serde_json::to_string_pretty(&self.secrets.identities(self.ports, &self.service_token))
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| format!("could not write the fixture file: {error}"))?;
        fixtures::snapshot(&self.api, &self.secrets, &self.service_token)
    }

    /// Whether the API is among the services that came up.
    fn runs_api(&self, root: &Path) -> Result<bool, String> {
        let specification = self.compose_command(&["ps", "--services", "--status", "running"]);
        let output = Command::new(&specification.program)
            .args(&specification.arguments)
            .current_dir(root)
            .output()
            .map_err(|error| format!("could not list the stack's services: {error}"))?;
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|service| service.trim() == "api"))
    }

    fn env_file(&self) -> PathBuf {
        self.directory.join("stack.env")
    }

    fn fixture_file(&self) -> PathBuf {
        self.directory.join("fixture.json")
    }

    fn compose_command(&self, arguments: &[&str]) -> CommandSpec {
        let mut all = vec![
            "compose".to_string(),
            "--project-name".to_string(),
            self.project.clone(),
            "--env-file".to_string(),
            self.env_file().display().to_string(),
        ];
        all.extend(arguments.iter().map(|argument| (*argument).to_string()));
        CommandSpec {
            program: "docker".to_string(),
            arguments: all,
            working_directory: None,
            environment: BTreeMap::new(),
        }
    }

    fn compose(
        &self,
        arguments: &[&str],
        root: &Path,
        log_path: &Path,
        timeout_seconds: u64,
    ) -> Result<(), String> {
        run_command(
            &self.compose_command(arguments),
            timeout_seconds,
            root,
            log_path,
        )
        .map_err(|failure| match failure {
            ProcessFailure::Failed => format!("docker compose {} failed", arguments.join(" ")),
            ProcessFailure::TimedOut => {
                format!("docker compose {} timed out", arguments.join(" "))
            }
            ProcessFailure::Io(error) => error,
        })
    }

    fn collect_logs(&self, root: &Path, log_path: &Path) {
        let _ = self.compose(
            &["logs", "--no-color", "--timestamps", "--tail", "400"],
            root,
            log_path,
            120,
        );
    }

    /// Removes what Compose labelled with this project, then the secrets.
    fn down(&mut self, root: &Path, log_path: &Path, timeout_seconds: u64) -> Result<(), String> {
        if self.removed {
            return Ok(());
        }
        self.compose(
            &[
                "down",
                "--volumes",
                "--remove-orphans",
                "--rmi",
                "local",
                "--timeout",
                "10",
            ],
            root,
            log_path,
            timeout_seconds,
        )?;
        self.removed = true;
        fs::remove_dir_all(&self.directory).map_err(|error| {
            format!(
                "the stack is down, but its secrets in {} remain: {error}",
                self.directory.display()
            )
        })
    }
}

impl Drop for ComposeStack {
    fn drop(&mut self) {
        if self.removed {
            return;
        }
        let specification = self.compose_command(&[
            "down",
            "--volumes",
            "--remove-orphans",
            "--rmi",
            "local",
            "--timeout",
            "10",
        ]);
        let _ = Command::new(&specification.program)
            .args(&specification.arguments)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn disagreement(what: &str, expected: &Value, actual: &Value) -> String {
    let pretty = |value: &Value| serde_json::to_string_pretty(value).unwrap_or_default();
    format!(
        "{what} changed the fixture graph.\nexpected:\n{}\nactual:\n{}",
        pretty(expected),
        pretty(actual)
    )
}

/// `xtask stack up|down`: a developer's stack, with the same fixture and
/// secrets generated for it, kept until `stack down`.
pub(crate) fn run(arguments: &[String]) -> Result<(), String> {
    let root = std::env::current_dir().map_err(|error| error.to_string())?;
    let directory = root.join(STACK_DIRECTORY).join(LOCAL_PROJECT);
    let log_path = directory.join("stack.log");
    match arguments.first().map(String::as_str) {
        Some("up") => {
            let secrets_file = directory.join("secrets.json");
            // A stack that is already up keeps its secrets: PostgreSQL only reads
            // its password when the volume is first created.
            let secrets = if let Ok(saved) = fs::read_to_string(&secrets_file) {
                serde_json::from_str(&saved)
                    .map_err(|error| format!("{}: {error}", secrets_file.display()))?
            } else {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|error| error.to_string())?;
                Secrets::generate(now.as_secs())?
            };
            fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
            fs::write(
                &secrets_file,
                serde_json::to_string(&secrets).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            println!(
                "[stack] starting {LOCAL_PROJECT}; Compose output goes to {}",
                log_path.display()
            );
            let services = arguments[1..]
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let plan = Plan {
                project: LOCAL_PROJECT.to_string(),
                directory,
                secrets,
                ports: Ports::STANDARD,
            };
            let mut stack = ComposeStack::start(plan, &services, &root, &log_path, 3_600)?;
            // The developer's stack stays up; only `stack down` removes it.
            stack.removed = true;
            if !stack.fixture_file().exists() {
                println!(
                    "[stack] up without the API, so nothing was seeded; its environment is {}",
                    stack.env_file().display()
                );
                return Ok(());
            }
            let fixture: Value = serde_json::from_str(
                &fs::read_to_string(stack.fixture_file()).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            println!(
                "[stack] dashboard: {}",
                fixture["dashboard_url"].as_str().unwrap_or_default()
            );
            println!(
                "[stack] sign in as {} with the password in {}",
                fixture["dashboard_login"]["username"]
                    .as_str()
                    .unwrap_or_default(),
                stack.fixture_file().display()
            );
            println!(
                "[stack] every identity's API token is in that file too; `stack down` deletes it"
            );
            Ok(())
        }
        Some("down") => {
            let env_file = directory.join("stack.env");
            if !env_file.exists() {
                println!(
                    "[stack] {LOCAL_PROJECT} is not up: {} does not exist",
                    env_file.display()
                );
                return Ok(());
            }
            let status = Command::new("docker")
                .args(["compose", "--project-name", LOCAL_PROJECT, "--env-file"])
                .arg(&env_file)
                .args(["down", "--volumes", "--remove-orphans", "--rmi", "local"])
                .status()
                .map_err(|error| format!("could not run docker compose: {error}"))?;
            if !status.success() {
                return Err("docker compose down failed".to_string());
            }
            if directory.exists() {
                fs::remove_dir_all(&directory).map_err(|error| error.to_string())?;
            }
            println!("[stack] {LOCAL_PROJECT} and its secrets are gone");
            Ok(())
        }
        _ => Err("usage: capsulet-xtask stack up [service ...] | stack down".to_string()),
    }
}
