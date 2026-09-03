use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Profile {
    Fast,
    Full,
}

impl Profile {
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "fast" => Ok(Self::Fast),
            "full" => Ok(Self::Full),
            other => Err(format!("unknown verification profile: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CommandSpec {
    pub(crate) program: String,
    pub(crate) arguments: Vec<String>,
    pub(crate) working_directory: Option<String>,
    pub(crate) environment: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Gate {
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) prerequisites: Vec<String>,
    pub(crate) timeout_seconds: u64,
    pub(crate) artifacts: Vec<&'static str>,
    pub(crate) profiles: Vec<Profile>,
    pub(crate) commands: Vec<CommandSpec>,
    pub(crate) provision: Vec<CommandSpec>,
    pub(crate) teardown: Vec<CommandSpec>,
}

fn command(program: &str, arguments: &[&str]) -> CommandSpec {
    CommandSpec {
        program: program.to_string(),
        arguments: arguments.iter().map(|value| (*value).to_string()).collect(),
        working_directory: None,
        environment: BTreeMap::new(),
    }
}

fn in_dashboard(mut command: CommandSpec) -> CommandSpec {
    command.working_directory = Some("dashboard".to_string());
    command
}

fn gate(
    name: &'static str,
    description: &'static str,
    prerequisites: &[&str],
    timeout_seconds: u64,
    profiles: &[Profile],
    commands: Vec<CommandSpec>,
) -> Gate {
    Gate {
        name,
        description,
        prerequisites: prerequisites
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        timeout_seconds,
        artifacts: vec![".capsulet/verify/<run>/<gate>.log"],
        profiles: profiles.to_vec(),
        commands,
        provision: Vec::new(),
        teardown: Vec::new(),
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "the declarative gate graph is kept together"
)]
pub(crate) fn gates() -> Vec<Gate> {
    use Profile::{Fast, Full};
    let powershell = if cfg!(windows) { "powershell" } else { "pwsh" };
    vec![
        gate(
            "format",
            "Rust formatting",
            &["cargo"],
            120,
            &[Fast, Full],
            vec![command("cargo", &["fmt", "--all", "--", "--check"])],
        ),
        gate(
            "lint",
            "strict workspace lint",
            &["cargo"],
            900,
            &[Full],
            vec![command(
                "cargo",
                &[
                    "clippy",
                    "--workspace",
                    "--all-targets",
                    "--all-features",
                    "--locked",
                    "--",
                    "-D",
                    "warnings",
                ],
            )],
        ),
        gate(
            "unit",
            "locked workspace tests",
            &["cargo"],
            1_800,
            &[Fast, Full],
            vec![
                command(
                    "cargo",
                    &[
                        "test",
                        "--workspace",
                        "--exclude",
                        "capsulet-xtask",
                        "--exclude",
                        "capsulet-postgres",
                        "--locked",
                    ],
                ),
                command(
                    "cargo",
                    &["test", "-p", "capsulet-postgres", "--lib", "--locked"],
                ),
            ],
        ),
        gate(
            "ir",
            "verified-computation IR contracts, purity, and adapter coverage",
            &["cargo"],
            600,
            &[Fast, Full],
            vec![
                command("cargo", &["test", "-p", "capsulet-ir", "--locked"]),
                command("cargo", &["test", "-p", "capsulet-ir-adapters", "--locked"]),
            ],
        ),
        gate(
            "replay",
            "offline certificate replay, including the tamper case",
            &["cargo"],
            600,
            &[Fast, Full],
            vec![command(
                "cargo",
                &["test", "-p", "capsulet-replay", "--locked"],
            )],
        ),
        gate(
            "api-contracts",
            "runtime and OpenAPI contracts",
            &["cargo"],
            600,
            &[Fast, Full],
            vec![
                command(
                    "cargo",
                    &[
                        "test",
                        "-p",
                        "capsulet-api",
                        "--test",
                        "openapi_contract",
                        "--locked",
                    ],
                ),
                command(
                    "cargo",
                    &[
                        "run",
                        "-p",
                        "capsulet-api",
                        "--bin",
                        "export-openapi",
                        "--locked",
                        "--",
                        "--check",
                    ],
                ),
            ],
        ),
        gate(
            "claims",
            "public claims and lifecycle contracts",
            &[powershell],
            300,
            &[Fast, Full],
            vec![
                command(
                    powershell,
                    &["-NoProfile", "-File", "scripts/tests/check-contracts.ps1"],
                ),
                command(
                    powershell,
                    &[
                        "-NoProfile",
                        "-File",
                        "scripts/tests/check-product-claims.ps1",
                    ],
                ),
                command(
                    powershell,
                    &["-NoProfile", "-File", "scripts/check-product-claims.ps1"],
                ),
            ],
        ),
        gate(
            "sdk",
            "Python and dashboard transport contracts",
            &["python", "npm", "npx"],
            600,
            &[Fast, Full],
            vec![
                command(
                    "python",
                    &[
                        "-m",
                        "unittest",
                        "discover",
                        "-s",
                        "sdk/python/tests",
                        "-p",
                        "test_*.py",
                    ],
                ),
                in_dashboard(command("npm", &["test"])),
                in_dashboard(command("npx", &["tsc", "--noEmit"])),
            ],
        ),
        gate(
            "dashboard",
            "dashboard lint, typecheck, and build",
            &["npm"],
            900,
            &[Full],
            vec![
                in_dashboard(command("npm", &["run", "lint"])),
                in_dashboard(command("npm", &["run", "build"])),
            ],
        ),
        gate(
            "postgres",
            "isolated PostgreSQL integration suite",
            &["cargo", "docker"],
            1_200,
            &[Full],
            vec![
                command(
                    "cargo",
                    &[
                        "test",
                        "-p",
                        "capsulet-postgres",
                        "--test",
                        "postgres_integration",
                        "--locked",
                    ],
                ),
                command(
                    "cargo",
                    &[
                        "test",
                        "-p",
                        "capsulet-postgres",
                        "--test",
                        "ir_and_assurance",
                        "--locked",
                    ],
                ),
            ],
        ),
        gate(
            "migrations",
            "forward migration compatibility",
            &["cargo", "docker"],
            1_200,
            &[Full],
            vec![command(
                "cargo",
                &[
                    "test",
                    "-p",
                    "capsulet-postgres",
                    "--test",
                    "migrations",
                    "--locked",
                ],
            )],
        ),
        gate(
            "security",
            "dependency and source security checks",
            &["cargo-audit", "npm"],
            600,
            &[Full],
            vec![
                // `cargo audit` reads Cargo.lock by itself and rejects
                // `--locked`, which is a cargo flag it does not forward.
                command("cargo", &["audit"]),
                in_dashboard(command("npm", &["audit", "--audit-level=high"])),
            ],
        ),
        gate(
            "compose",
            "Compose configuration and smoke test",
            &["docker"],
            1_200,
            &[Full],
            vec![command("docker", &["compose", "config", "--quiet"])],
        ),
        gate(
            "helm",
            "Helm lint and render",
            &["helm"],
            300,
            &[Full],
            vec![
                command("helm", &["lint", "charts/capsulet"]),
                command("helm", &["template", "capsulet", "charts/capsulet"]),
            ],
        ),
        gate(
            "kind",
            "Kind deployment smoke test",
            &["kind", "kubectl", "helm", "docker"],
            1_800,
            &[Full],
            vec![command("kind", &["get", "clusters"])],
        ),
    ]
}
