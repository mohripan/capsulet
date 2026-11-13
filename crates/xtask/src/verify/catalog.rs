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

/// A gate that runs the dashboard's own tools (its linter, `tsc`, its tests or
/// its build) installs the dashboard's packages from the lockfile first, so it
/// passes on a fresh clone exactly as it does on a checkout that has them.
fn with_dashboard_packages(gate: Gate) -> Gate {
    Gate {
        provision: vec![in_dashboard(command("npm", &["ci"]))],
        ..gate
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
        with_dashboard_packages(gate(
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
        )),
        with_dashboard_packages(gate(
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
        )),
        with_dashboard_packages(gate(
            "dashboard",
            "dashboard lint, typecheck, and build",
            &["npm"],
            900,
            &[Full],
            vec![
                in_dashboard(command("npm", &["run", "lint"])),
                in_dashboard(command("npm", &["run", "build"])),
            ],
        )),
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
        // Real containers, so the `unit` gate, which has no Docker, does not
        // build these: the feature gates the test target, and this gate holds
        // every one of them to running.
        every_test_must_run(gate(
            "verifier-isolation",
            "a validator in a real container reaches no network, writes only its outputs, is \
             stopped past its memory or its time, and runs again to the same outputs; and the pack \
             template and the reference packs pass their own tests there",
            &["cargo", "docker"],
            900,
            &[Full],
            vec![
                command(
                    "cargo",
                    &[
                        "test",
                        "-p",
                        "capsulet-verifier",
                        "--features",
                        "container-tests",
                        "--test",
                        "isolation",
                        "--locked",
                    ],
                ),
                command(
                    "cargo",
                    &[
                        "test",
                        "-p",
                        "capsulet-pack",
                        "--features",
                        "container-tests",
                        "--test",
                        "template",
                        "--locked",
                    ],
                ),
                command(
                    "cargo",
                    &[
                        "test",
                        "-p",
                        "capsulet-pack",
                        "--features",
                        "container-tests",
                        "--test",
                        "reference_packs",
                        "--locked",
                    ],
                ),
            ],
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
        with_dashboard_packages(gate(
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
        )),
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
            "domain-free",
            "The kernel, the runtime and the graph worker name nothing a pack declares: no pack, \
             tool, authority, obligation, relation, phrasing or cue in a string or an identifier",
            &["cargo"],
            300,
            &[Fast, Full],
            vec![xtask(&["domain", "check"])],
        ),
        // The M4 gate (#163), on a stack of its own: the API with PostgreSQL and
        // the object store. The runs are driven by this checkout's worker, on
        // the host, because a pack's validators run in containers and nothing
        // on the stack holds a container runtime.
        Gate {
            stack: Some(&["api"]),
            ..every_test_must_run(gate(
                "m4",
                "Three unrelated packs, contract clauses, code changes and infrastructure \
                 policy, installed signed, run end to end as IR runs on a stack and certified \
                 from their logs over the API; every bundle replays offline and every tool runs \
                 again to its record",
                &["cargo", "docker"],
                3_600,
                &[Full],
                vec![command(
                    "cargo",
                    &[
                        "test",
                        "-p",
                        "capsulet-graph-worker",
                        "--features",
                        "stack-tests",
                        "--test",
                        "m4_gate",
                        "--locked",
                    ],
                )],
            ))
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
            "ci-contract",
            "CI runs every check through the verifier: no job runs its own checks or skips one \
             silently, every Full gate runs in some job, each job uploads its summary, and \
             `required` waits for every job and checks those summaries",
            &["cargo"],
            300,
            &[Fast, Full],
            vec![xtask(&["ci", "check"])],
        ),
        // Needs the history: the rehearsal record names the commit it was run at,
        // which this checkout must descend from.
        gate(
            "completion",
            "Every M1 deliverable and exit criterion, word for word from the plan, names the \
             full-profile gates that prove it, and the clean-checkout rehearsal record shows them \
             passing in a clean run that left nothing behind",
            &["cargo", "git"],
            300,
            &[Fast, Full],
            vec![xtask(&["completion", "check"])],
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
            "dependency-images",
            "Trivy, pinned by digest, over every third-party image the chart and Compose pull; \
             every finding that fails trivy.yaml a registered, unexpired image exception",
            &["cargo", "docker"],
            1_800,
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
                    "images",
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
            "Helm lint and render; every workload's image against release.toml, and its pod held \
             to non-root, seccomp, no escalation, no capabilities, probes, tokens only where \
             used, and secrets the chart renders or its values name",
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
        // Not in a profile: the trace viewer is optional (ADR 0020), so a run that
        // does not use one does not need its image. On demand, on a stack of its
        // own, it shows the export reaches one and carries no content.
        Gate {
            stack: Some(&["api", "graph-worker"]),
            ..gate(
                "trace-viewer",
                "A synthetic agent run's trace reaches Jaeger over OTLP/HTTP and is found there by \
                 the trace id its bundle names, with Capsulet's spans and none of its input",
                &["docker", powershell],
                1_800,
                &[],
                vec![command(
                    powershell,
                    &[
                        "-NoProfile",
                        "-File",
                        "scripts/trace-viewer-spike.ps1",
                        "-Viewer",
                        "jaeger",
                    ],
                )],
            )
        },
        // Not in a profile: this one needs a Wasmtime runtime and a WASI Python
        // build that a normal checkout does not have. It runs nightly by name,
        // so its pass/fail still lives here rather than in YAML.
        // The test is `#[ignore]`d, so it shows as not run anywhere else; here
        // it has to run, and fails without the runtime.
        every_test_must_run(gate(
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
                    "--ignored",
                    "--nocapture",
                ],
            )],
        )),
        // A cluster of the run's own, named for it, which the teardown deletes
        // even when the smoke did not get that far; see `kind.rs`.
        Gate {
            teardown: vec![xtask(&["kind", "delete"])],
            ..gate(
                "kind",
                "The chart installed on a Kind cluster of its own from images built here, its \
                 Helm tests, and a smoke that logs in, runs a job and an agent and checks project \
                 isolation; then uninstalled with nothing left behind",
                &["kind", "kubectl", "helm", "docker", "cargo", "git"],
                3_600,
                &[Full],
                vec![xtask(&["kind", "smoke"])],
            )
        },
    ]
}

/// `cargo run -p capsulet-xtask --locked -- <arguments>`.
fn xtask(arguments: &[&str]) -> CommandSpec {
    let mut all = vec!["run", "-p", "capsulet-xtask", "--locked", "--"];
    all.extend_from_slice(arguments);
    command("cargo", &all)
}
