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
    /// Whether every test the gate's commands collect has to run: a binary that
    /// ran none, or ignored or filtered any, fails the gate. See `counts.rs`.
    pub(crate) every_test_runs: bool,
    /// A Compose stack of the gate's own, seeded with the stack fixture, that
    /// the gate's commands run against: `Some(&[])` for every service, or the
    /// services named and what they depend on. See `compose.rs`.
    pub(crate) stack: Option<&'static [&'static str]>,
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
        every_test_runs: false,
        stack: None,
    }
}

/// A gate whose every collected test has to run, which `verify` checks from the
/// counts the test binaries report. Used for the database suites, where a test
/// that could not reach its database must fail rather than pass by skipping.
fn every_test_must_run(gate: Gate) -> Gate {
    Gate {
        every_test_runs: true,
        ..gate
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
            "verifier",
            "verifier and dependency-policy regression tests",
            &["cargo"],
            600,
            &[Fast, Full],
            vec![command(
                "cargo",
                &[
                    "test",
                    "-p",
                    "capsulet-xtask",
                    "--locked",
                    "--target-dir",
                    ".capsulet/verify/xtask-tests",
                ],
            )],
        ),
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
                        // Both of these need a database the `postgres` and
                        // `chaos` gates provision. Running them here would make
                        // this gate fail for want of a fixture rather than for
                        // want of correctness.
                        "--exclude",
                        "capsulet-xtask",
                        "--exclude",
                        "capsulet-postgres",
                        "--exclude",
                        "capsulet-graph-worker",
                        "--locked",
                    ],
                ),
                command(
                    "cargo",
                    &["test", "-p", "capsulet-postgres", "--lib", "--locked"],
                ),
                command(
                    "cargo",
                    &["test", "-p", "capsulet-graph-worker", "--lib", "--locked"],
                ),
            ],
        ),
        gate(
            "ir",
            "IR contracts, runtime decisions, purity, and adapter coverage",
            &["cargo"],
            600,
            &[Fast, Full],
            vec![
                command("cargo", &["test", "-p", "capsulet-ir", "--locked"]),
                command("cargo", &["test", "-p", "capsulet-ir-adapters", "--locked"]),
                command("cargo", &["test", "-p", "capsulet-runtime", "--locked"]),
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
            "authorization",
            "executed HTTP authorization matrix and generated policy documentation",
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
                        "--lib",
                        "authorization_",
                        "--locked",
                    ],
                ),
                command(
                    "cargo",
                    &[
                        "test",
                        "-p",
                        "capsulet-api",
                        "--test",
                        "authorization_matrix",
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
                        "export-authorization",
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
        // Builds and lists the tests of every gate command a claim cites, so it
        // is as slow as a cold `unit` build. It needs no database: listing a
        // test does not run it.
        gate(
            "claims-evidence",
            "claim evidence is collected by the gate command it names",
            &["cargo", "python", "npm", powershell],
            2_400,
            &[Fast, Full],
            vec![command(
                "cargo",
                &[
                    "run",
                    "-p",
                    "capsulet-xtask",
                    "--locked",
                    "--",
                    "claims",
                    "collect",
                    "--check",
                ],
            )],
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
        every_test_must_run(gate(
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
                command(
                    "cargo",
                    &[
                        "test",
                        "-p",
                        "capsulet-postgres",
                        "--test",
                        "project_scope",
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
                        "ir_runs",
                        "--locked",
                    ],
                ),
                command(
                    "cargo",
                    &[
                        "test",
                        "-p",
                        "capsulet-graph-worker",
                        "--test",
                        "worker",
                        "--locked",
                    ],
                ),
                command(
                    "cargo",
                    &[
                        "test",
                        "-p",
                        "capsulet-graph-worker",
                        "--test",
                        "loops",
                        "--locked",
                    ],
                ),
                command(
                    "cargo",
                    &[
                        "test",
                        "-p",
                        "capsulet-graph-worker",
                        "--test",
                        "execution",
                        "--locked",
                    ],
                ),
                command(
                    "cargo",
                    &[
                        "test",
                        "-p",
                        "capsulet-graph-worker",
                        "--test",
                        "agent_runs",
                        "--locked",
                    ],
                ),
            ],
        )),
        every_test_must_run(gate(
            "chaos",
            "an IR run survives the worker dying at every step boundary",
            &["cargo", "docker"],
            1_800,
            &[Full],
            vec![command(
                "cargo",
                &[
                    "test",
                    "-p",
                    "capsulet-graph-worker",
                    "--test",
                    "chaos",
                    "--locked",
                ],
            )],
        )),
        every_test_must_run(gate(
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
        )),
        gate(
            "security",
            "Rust/npm/Python dependency and license policy",
            &["cargo-audit", "cargo-deny", "npm", "python"],
            900,
            &[Full],
            vec![
                // `cargo audit` reads Cargo.lock by itself and rejects
                // `--locked`, which is a cargo flag it does not forward.
                command("cargo", &["audit"]),
                // Licences and bans, which CI used to check and this gate did
                // not — a divergence that only showed up as a red pipeline.
                command("cargo", &["deny", "check"]),
                command(
                    "cargo",
                    &[
                        "run",
                        "-p",
                        "capsulet-xtask",
                        "--locked",
                        "--",
                        "security",
                        "npm",
                    ],
                ),
                in_dashboard(command(
                    "npm",
                    &["audit", "--omit=dev", "--audit-level=high"],
                )),
                command(
                    "python",
                    &[
                        "-m",
                        "unittest",
                        "discover",
                        "-s",
                        "scripts/tests",
                        "-p",
                        "test_audit_python.py",
                    ],
                ),
                command("python", &["scripts/audit-python.py"]),
            ],
        ),
        // The tests are ignored by default because they need these tools; this
        // gate declares them and runs the tests with `--ignored`, so they never
        // pass by being skipped.
        gate(
            "gate-regressions",
            "lint, dashboard and security gates fail on a Clippy warning, a crashing ESLint configuration, an unreviewed duplicate and a vulnerable crate",
            &["cargo", "cargo-audit", "cargo-deny", "npm"],
            1_200,
            &[Full],
            vec![command(
                "cargo",
                &[
                    "test",
                    "-p",
                    "capsulet-xtask",
                    "--test",
                    "gate_failures",
                    "--locked",
                    "--target-dir",
                    ".capsulet/verify/xtask-tests",
                    "--",
                    "--ignored",
                ],
            )],
        ),
        // Each of these gets a stack of its own, which comes down whatever its
        // commands did; see `compose.rs`.
        Gate {
            stack: Some(&[]),
            ..gate(
                "compose",
                "every service built and healthy, seeded alike twice; agents and jobs run through the stack with a worker killed",
                &["docker", powershell],
                3_600,
                &[Full],
                vec![
                    command("docker", &["compose", "config", "--quiet"]),
                    command(
                        powershell,
                        &["-NoProfile", "-File", "scripts/agent-smoke.ps1"],
                    ),
                    command(
                        powershell,
                        &["-NoProfile", "-File", "scripts/compose-smoke.ps1"],
                    ),
                ],
            )
        },
        Gate {
            stack: Some(&["api", "worker", "graph-worker", "dashboard"]),
            ..gate(
                "dashboard-e2e",
                "Playwright against a seeded stack, as every role, across both projects",
                &["docker", "npm", "npx"],
                3_600,
                &[Full],
                vec![
                    in_dashboard(command("npm", &["ci"])),
                    in_dashboard(command("npx", &["playwright", "install", "chromium"])),
                    in_dashboard(command("npx", &["playwright", "test"])),
                ],
            )
        },
        gate(
            "release",
            "release.toml against every crate, SDK, dashboard, chart, OpenAPI, Compose and \
             Dockerfile, and every image and repository reference",
            &["cargo", "git"],
            300,
            &[Fast, Full],
            vec![command(
                "cargo",
                &[
                    "run",
                    "-p",
                    "capsulet-xtask",
                    "--locked",
                    "--",
                    "release",
                    "check",
                ],
            )],
        ),
        gate(
            "supply-chain-contract",
            "Workflows pin every action by SHA, never privilege pull requests, build each image \
             once, and scan, attest, sign and verify its digest before tagging it; base images \
             are pinned by digest",
            &["cargo", "git"],
            300,
            &[Fast, Full],
            vec![command(
                "cargo",
                &[
                    "run",
                    "-p",
                    "capsulet-xtask",
                    "--locked",
                    "--",
                    "supply-chain",
                    "check",
                ],
            )],
        ),
        gate(
            "secrets",
            "gitleaks over the whole Git history; every finding a registered, unexpired test \
             fixture",
            &["cargo", "docker", "git"],
            900,
            &[Full],
            vec![command(
                "cargo",
                &[
                    "run",
                    "-p",
                    "capsulet-xtask",
                    "--locked",
                    "--",
                    "security",
                    "secrets",
                ],
            )],
        ),
        gate(
            "containers",
            "Every component image built from release.toml's plan, its labels and non-root user \
             checked, then removed",
            &["cargo", "docker", "git"],
            3_600,
            &[Full],
            vec![command(
                "cargo",
                &[
                    "run",
                    "-p",
                    "capsulet-xtask",
                    "--locked",
                    "--",
                    "release",
                    "images",
                ],
            )],
        ),
        gate(
            "helm",
            "Helm lint and render, and every workload's image against release.toml",
            &["helm", "cargo"],
            300,
            &[Full],
            vec![
                command("helm", &["lint", "charts/capsulet"]),
                command("helm", &["template", "capsulet", "charts/capsulet"]),
                command(
                    "cargo",
                    &[
                        "run",
                        "-p",
                        "capsulet-xtask",
                        "--locked",
                        "--",
                        "release",
                        "check-chart",
                    ],
                ),
            ],
        ),
        // Not in a profile: this one needs a Wasmtime runtime and a WASI Python
        // build that a normal checkout does not have. It runs nightly by name,
        // so its pass/fail still lives here rather than in YAML.
        gate(
            "wasm-python",
            "WASI Python runner integration",
            &["cargo", "wasmtime"],
            1_800,
            &[],
            vec![command(
                "cargo",
                &[
                    "test",
                    "-p",
                    "capsulet-runner",
                    "--test",
                    "wasm_python_runner",
                    "--locked",
                    "--",
                    "--nocapture",
                ],
            )],
        ),
        gate(
            "kind",
            "Kind cluster discovery (install/smoke pending #102)",
            &["kind", "kubectl", "helm", "docker"],
            1_800,
            &[Full],
            vec![command("kind", &["get", "clusters"])],
        ),
    ]
}
