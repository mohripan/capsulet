use std::{fs, path::Path, process::Command, thread, time::Duration};

use super::leftovers;

pub(crate) struct PostgresFixture {
    name: String,
    database_url: String,
    cleaned: bool,
}

impl PostgresFixture {
    /// A container named for the run and the gate, from the image Compose
    /// pins, so the database the suites run against is the one the stack and
    /// its scan use.
    pub(crate) fn start(root: &Path, run: &str, gate: &str) -> Result<Self, String> {
        let name = format!("{}-postgres-{gate}", leftovers::prefix(run));
        let image = image(root)?;
        let label = format!("capsulet.verify.run={run}");
        let output = Command::new("docker")
            .args([
                "run",
                "--detach",
                "--name",
                &name,
                "--label",
                &label,
                "--env",
                "POSTGRES_PASSWORD=capsulet_verify",
                "--env",
                "POSTGRES_USER=postgres",
                "--env",
                "POSTGRES_DB=capsulet_test",
                "--publish",
                "127.0.0.1::5432",
                &image,
            ])
            .output()
            .map_err(|error| format!("could not start PostgreSQL container: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "could not start disposable PostgreSQL container: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let mut fixture = Self {
            name,
            database_url: String::new(),
            cleaned: false,
        };
        fixture.wait_until_ready()?;
        fixture.database_url = fixture.discover_database_url()?;
        Ok(fixture)
    }

    pub(crate) fn database_url(&self) -> &str {
        &self.database_url
    }

    pub(crate) fn cleanup(&mut self) -> Result<(), String> {
        if self.cleaned {
            return Ok(());
        }
        let output = Command::new("docker")
            // `--volumes`: the image declares a volume for its data, which
            // would otherwise outlive the container.
            .args(["rm", "--force", "--volumes", &self.name])
            .output()
            .map_err(|error| format!("could not remove PostgreSQL container: {error}"))?;
        if output.status.success() {
            self.cleaned = true;
            Ok(())
        } else {
            Err(format!(
                "disposable PostgreSQL cleanup failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }

    fn wait_until_ready(&self) -> Result<(), String> {
        for _ in 0..100 {
            if Command::new("docker")
                .args([
                    "exec",
                    &self.name,
                    "pg_isready",
                    "--username",
                    "postgres",
                    "--dbname",
                    "capsulet_test",
                ])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
            {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(200));
        }
        Err("disposable PostgreSQL did not become ready".to_string())
    }

    fn discover_database_url(&self) -> Result<String, String> {
        let output = Command::new("docker")
            .args(["port", &self.name, "5432/tcp"])
            .output()
            .map_err(|error| format!("could not inspect PostgreSQL port: {error}"))?;
        if !output.status.success() {
            return Err("could not inspect disposable PostgreSQL port".to_string());
        }
        let binding = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
        let port = binding
            .trim()
            .rsplit(':')
            .next()
            .filter(|value| !value.is_empty())
            .ok_or("Docker did not publish a PostgreSQL port")?;
        Ok(format!(
            "postgres://postgres:capsulet_verify@127.0.0.1:{port}/capsulet_test"
        ))
    }
}

/// The image of `compose.yaml`'s `postgres` service, pinned by digest.
fn image(root: &Path) -> Result<String, String> {
    let path = root.join("compose.yaml");
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let compose: serde_yaml::Value =
        serde_yaml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    compose["services"]["postgres"]["image"]
        .as_str()
        .filter(|image| image.contains("@sha256:"))
        .map(str::to_string)
        .ok_or_else(|| "compose.yaml names no postgres image pinned by digest".to_string())
}

impl Drop for PostgresFixture {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}
