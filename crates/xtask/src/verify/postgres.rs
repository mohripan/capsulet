use std::{
    process::Command,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub(crate) struct PostgresFixture {
    name: String,
    database_url: String,
    cleaned: bool,
}

impl PostgresFixture {
    pub(crate) fn start() -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis();
        let name = format!("capsulet-verify-postgres-{}-{nonce}", std::process::id());
        let output = Command::new("docker")
            .args([
                "run",
                "--detach",
                "--name",
                &name,
                "--env",
                "POSTGRES_PASSWORD=capsulet_verify",
                "--env",
                "POSTGRES_USER=postgres",
                "--env",
                "POSTGRES_DB=capsulet_test",
                "--publish",
                "127.0.0.1::5432",
                "postgres:16-alpine",
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
            .args(["rm", "--force", &self.name])
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

impl Drop for PostgresFixture {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}
