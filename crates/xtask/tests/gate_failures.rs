//! The lint, dashboard and security gates fail on the defects they exist to catch.
//!
//! Each test takes a gate's own command from the gate graph, runs it against a
//! small fixture that has the defect, and expects it to fail for that reason. A
//! clean fixture beside it has to pass the same command, so a failure cannot come
//! from a missing tool or a broken environment instead. The policy files the
//! gates read (`deny.toml`, `.cargo/audit.toml`) are the repository's own, copied
//! into each fixture, so a weakened policy or a weakened command fails here.
//!
//! These need Clippy, `cargo-deny`, `cargo-audit` with network access for the
//! advisory database, and the dashboard's installed `node_modules`. They are
//! ignored by default and run by the `gate-regressions` gate, which declares
//! those prerequisites. They never pass by skipping.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// One command of a gate, as the verifier would run it.
struct GateCommand {
    program: String,
    arguments: Vec<String>,
}

impl GateCommand {
    /// Finds the command of `gate` whose arguments start with `prefix`.
    fn of(gate: &str, program: &str, prefix: &[&str]) -> Self {
        let output = Command::new(env!("CARGO_BIN_EXE_capsulet-xtask"))
            .args(["verify", "--list", "--format", "json"])
            .output()
            .expect("the verifier lists its gates");
        assert!(output.status.success(), "{}", text(&output));
        let graph: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("gate graph JSON");
        let commands = graph
            .as_array()
            .expect("gates")
            .iter()
            .find(|candidate| candidate["name"] == gate)
            .unwrap_or_else(|| panic!("no `{gate}` gate"))["commands"]
            .as_array()
            .expect("commands")
            .clone();
        commands
            .iter()
            .find_map(|command| {
                let arguments = command["arguments"]
                    .as_array()?
                    .iter()
                    .map(|argument| argument.as_str().map(str::to_string))
                    .collect::<Option<Vec<_>>>()?;
                (command["program"] == program
                    && arguments.len() >= prefix.len()
                    && arguments
                        .iter()
                        .zip(prefix)
                        .all(|(left, right)| left == right))
                .then(|| Self {
                    program: program.to_string(),
                    arguments,
                })
            })
            .unwrap_or_else(|| {
                panic!(
                    "the `{gate}` gate no longer runs `{program} {}`",
                    prefix.join(" ")
                )
            })
    }

    fn run_in(&self, directory: &Path) -> Output {
        let program = if cfg!(windows) && matches!(self.program.as_str(), "npm" | "npx") {
            format!("{}.cmd", self.program)
        } else {
            self.program.clone()
        };
        Command::new(program)
            .args(&self.arguments)
            .current_dir(directory)
            .output()
            .unwrap_or_else(|error| panic!("could not run {}: {error}", self.program))
    }
}

fn text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned() + &String::from_utf8_lossy(&output.stderr)
}

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().expect("parent")).expect("fixture directory");
    fs::write(path, contents).expect("fixture file");
}

fn generate_lockfile(directory: &Path) {
    let output = Command::new("cargo")
        .args(["generate-lockfile", "--offline"])
        .current_dir(directory)
        .output()
        .expect("cargo runs");
    assert!(output.status.success(), "{}", text(&output));
}

/// A crate that is its own workspace, with `body` as its library.
fn rust_crate(directory: &Path, body: &str) {
    write(
        &directory.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\nlicense = \"MIT\"\npublish = false\n\n[workspace]\n",
    );
    write(&directory.join("src/lib.rs"), body);
    generate_lockfile(directory);
}

#[test]
#[ignore = "needs Clippy, cargo-deny, cargo-audit and the dashboard's node_modules; run by the gate-regressions gate"]
fn a_clippy_warning_fails_the_lint_gate() {
    let lint = GateCommand::of("lint", "cargo", &["clippy"]);
    let clean = tempfile::tempdir().expect("temp directory");
    rust_crate(clean.path(), "pub fn answer() -> u32 {\n    42\n}\n");
    let passed = lint.run_in(clean.path());
    assert!(passed.status.success(), "clean crate: {}", text(&passed));

    let warned = tempfile::tempdir().expect("temp directory");
    rust_crate(
        warned.path(),
        "pub fn answer() -> u32 {\n    return 42;\n}\n",
    );
    let failed = lint.run_in(warned.path());
    assert!(
        !failed.status.success(),
        "a Clippy warning passed the lint gate"
    );
    assert!(
        text(&failed).contains("needless_return"),
        "{}",
        text(&failed)
    );
}

/// Removes the lint fixture, which has to live inside the dashboard so that
/// it resolves the linter the dashboard has installed.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
#[ignore = "needs Clippy, cargo-deny, cargo-audit and the dashboard's node_modules; run by the gate-regressions gate"]
fn an_eslint_configuration_crash_fails_the_dashboard_gate() {
    let lint = GateCommand::of("dashboard", "npm", &["run", "lint"]);
    let dashboard = repository().join("dashboard");
    assert!(
        dashboard.join("node_modules/eslint").is_dir(),
        "the dashboard's dependencies are not installed; run `npm ci` in dashboard/"
    );
    let scratch =
        Scratch(dashboard.join(format!(".capsulet-eslint-fixture-{}", std::process::id())));
    write(
        &scratch.0.join("package.json"),
        r#"{"name":"fixture","private":true,"type":"module","scripts":{"lint":"eslint ."}}"#,
    );
    write(&scratch.0.join("index.js"), "export const value = 1;\n");

    write(&scratch.0.join("eslint.config.mjs"), "export default [];\n");
    let passed = lint.run_in(&scratch.0);
    assert!(
        passed.status.success(),
        "valid configuration: {}",
        text(&passed)
    );

    write(
        &scratch.0.join("eslint.config.mjs"),
        "throw new Error(\"fixture configuration crash\");\n",
    );
    let failed = lint.run_in(&scratch.0);
    assert!(
        !failed.status.success(),
        "a crashing ESLint configuration passed the dashboard gate"
    );
    assert!(
        text(&failed).contains("fixture configuration crash"),
        "{}",
        text(&failed)
    );
}

/// A workspace whose root depends on `dup` once, or on two versions of it.
fn duplicate_fixture(directory: &Path, both: bool) {
    let mut dependencies = "dup_old = { package = \"dup\", path = \"../old\" }\n".to_string();
    if both {
        dependencies.push_str("dup_new = { package = \"dup\", path = \"../new\" }\n");
    }
    for (folder, version) in [("old", "0.1.0"), ("new", "0.2.0")] {
        write(
            &directory.join(folder).join("Cargo.toml"),
            &format!(
                "[package]\nname = \"dup\"\nversion = \"{version}\"\nedition = \"2021\"\nlicense = \"MIT\"\npublish = false\n"
            ),
        );
        write(&directory.join(folder).join("src/lib.rs"), "\n");
    }
    let root = directory.join("root");
    write(
        &root.join("Cargo.toml"),
        &format!(
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\nlicense = \"MIT\"\npublish = false\n\n[dependencies]\n{dependencies}\n[workspace]\n"
        ),
    );
    write(&root.join("src/lib.rs"), "\n");
    fs::copy(repository().join("deny.toml"), root.join("deny.toml")).expect("policy copy");
    generate_lockfile(&root);
}

#[test]
#[ignore = "needs Clippy, cargo-deny, cargo-audit and the dashboard's node_modules; run by the gate-regressions gate"]
fn an_unreviewed_duplicate_dependency_fails_the_security_gate() {
    let deny = GateCommand::of("security", "cargo", &["deny", "check"]);
    let single = tempfile::tempdir().expect("temp directory");
    duplicate_fixture(single.path(), false);
    let passed = deny.run_in(&single.path().join("root"));
    assert!(passed.status.success(), "one version: {}", text(&passed));

    let doubled = tempfile::tempdir().expect("temp directory");
    duplicate_fixture(doubled.path(), true);
    let failed = deny.run_in(&doubled.path().join("root"));
    assert!(
        !failed.status.success(),
        "an unreviewed duplicate passed the security gate"
    );
    assert!(
        text(&failed).contains("duplicate entries for crate 'dup'"),
        "{}",
        text(&failed)
    );
}

/// A lockfile for `fixture`, optionally depending on a release of smallvec
/// with a published buffer overflow (RUSTSEC-2021-0003).
fn audit_fixture(directory: &Path, vulnerable: bool) {
    let mut lockfile =
        "version = 4\n\n[[package]]\nname = \"fixture\"\nversion = \"0.1.0\"\n".to_string();
    if vulnerable {
        lockfile.push_str(
            "dependencies = [\n \"smallvec\",\n]\n\n[[package]]\nname = \"smallvec\"\nversion = \"1.6.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n",
        );
    }
    write(&directory.join("Cargo.lock"), &lockfile);
    // The repository's exceptions apply: an advisory it has not registered
    // must fail, whatever else it has chosen to ignore.
    fs::create_dir_all(directory.join(".cargo")).expect("config directory");
    fs::copy(
        repository().join(".cargo/audit.toml"),
        directory.join(".cargo/audit.toml"),
    )
    .expect("policy copy");
}

#[test]
#[ignore = "needs Clippy, cargo-deny, cargo-audit and the dashboard's node_modules; run by the gate-regressions gate"]
fn a_vulnerable_rust_dependency_fails_the_security_gate() {
    let audit = GateCommand::of("security", "cargo", &["audit"]);
    let clean = tempfile::tempdir().expect("temp directory");
    audit_fixture(clean.path(), false);
    let passed = audit.run_in(clean.path());
    assert!(passed.status.success(), "clean lockfile: {}", text(&passed));

    let vulnerable = tempfile::tempdir().expect("temp directory");
    audit_fixture(vulnerable.path(), true);
    let failed = audit.run_in(vulnerable.path());
    assert!(
        !failed.status.success(),
        "a vulnerable dependency passed the security gate"
    );
    assert!(
        text(&failed).contains("RUSTSEC-2021-0003"),
        "{}",
        text(&failed)
    );
}
