//! `release check` and `release set` on a copy of this repository's release
//! manifests. Each test breaks one surface and requires the check to name it,
//! so a manifest that stops being checked fails here rather than drifting.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

/// Every file that carries the release identity, besides the crate manifests.
const FILES: &[&str] = &[
    "release.toml",
    "Cargo.toml",
    "Cargo.lock",
    "sdk/python/pyproject.toml",
    "sdk/python/src/capsulet/_version.py",
    "dashboard/package.json",
    "dashboard/package-lock.json",
    "charts/capsulet/Chart.yaml",
    "charts/capsulet/values.yaml",
    "crates/api/openapi.json",
    "compose.yaml",
    "crates/Dockerfile",
    "dashboard/Dockerfile",
    ".github/workflows/ci.yml",
    ".github/workflows/supply-chain.yml",
    "docs/contracts/release-process.md",
];

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn git(root: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        .args([
            "-c",
            "user.name=release contract",
            "-c",
            "user.email=release@example.invalid",
            "-c",
            "core.autocrlf=false",
        ])
        .args(arguments)
        .current_dir(root)
        .status()
        .expect("git runs");
    assert!(status.success(), "git {arguments:?}");
}

/// A Git repository holding a copy of the release manifests, committed.
struct Fixture {
    directory: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("temporary directory");
        let source = repository();
        let mut files: Vec<String> = FILES.iter().map(|file| (*file).to_string()).collect();
        for entry in fs::read_dir(source.join("crates")).expect("crates") {
            let member = entry.expect("crate").path();
            if member.join("Cargo.toml").is_file() {
                let name = member
                    .file_name()
                    .expect("name")
                    .to_string_lossy()
                    .into_owned();
                files.push(format!("crates/{name}/Cargo.toml"));
            }
        }
        for file in &files {
            let target = directory.path().join(file);
            fs::create_dir_all(target.parent().expect("parent")).expect("directory");
            fs::copy(source.join(file), &target).unwrap_or_else(|error| panic!("{file}: {error}"));
        }
        git(directory.path(), &["init", "--quiet"]);
        git(directory.path(), &["add", "--all"]);
        git(
            directory.path(),
            &["commit", "--quiet", "--message", "fixture"],
        );
        Self { directory }
    }

    fn root(&self) -> &Path {
        self.directory.path()
    }

    fn read(&self, file: &str) -> String {
        fs::read_to_string(self.root().join(file)).unwrap_or_else(|error| panic!("{file}: {error}"))
    }

    /// Replaces the first `from` in `file`, which must contain it.
    fn edit(&self, file: &str, from: &str, to: &str) {
        let text = self.read(file);
        assert!(text.contains(from), "{file} has no {from:?}");
        fs::write(self.root().join(file), text.replacen(from, to, 1)).expect("write");
    }

    fn commit(&self) {
        git(self.root(), &["add", "--all"]);
        git(self.root(), &["commit", "--quiet", "--message", "change"]);
    }

    fn release(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_capsulet-xtask"))
            .arg("release")
            .args(arguments)
            .arg("--root")
            .arg(self.root())
            .output()
            .expect("xtask runs")
    }

    /// Runs `release check`, which must fail and name every `expected` text.
    fn refuses(&self, expected: &[&str]) {
        let output = self.release(&["check"]);
        let report = stderr(&output);
        assert!(
            !output.status.success(),
            "the check passed:\n{}",
            stdout(&output)
        );
        for text in expected {
            assert!(
                report.contains(text),
                "the check did not say {text:?}:\n{report}"
            );
        }
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn the_release_manifests_as_checked_in_agree() {
    let fixture = Fixture::new();
    let output = fixture.release(&["check"]);
    assert!(output.status.success(), "{}", stderr(&output));
}

#[test]
fn a_crate_with_a_version_of_its_own_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        "crates/kernel/Cargo.toml",
        "version.workspace = true",
        "version = \"0.2.0\"",
    );
    fixture.refuses(&["crates/kernel/Cargo.toml: package.version must be inherited"]);
}

#[test]
fn a_cli_with_a_version_of_its_own_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        "crates/cli/Cargo.toml",
        "version.workspace = true",
        "version = \"9.9.9\"",
    );
    fixture.refuses(&["crates/cli/Cargo.toml: package.version must be inherited"]);
}

#[test]
fn a_workspace_on_another_version_is_refused() {
    let fixture = Fixture::new();
    fixture.edit("Cargo.toml", "version = \"0.1.0\"", "version = \"0.1.1\"");
    fixture.refuses(&["Cargo.toml: workspace.package.version is \"0.1.1\""]);
}

#[test]
fn an_sdk_built_for_other_api_versions_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        "sdk/python/src/capsulet/_version.py",
        "SUPPORTED_API_VERSIONS = \">=0.1.0, <0.2.0\"",
        "SUPPORTED_API_VERSIONS = \">=0.2.0, <0.3.0\"",
    );
    fixture.refuses(&["_version.py: SUPPORTED_API_VERSIONS is \">=0.2.0, <0.3.0\""]);
}

#[test]
fn an_sdk_that_writes_its_version_down_twice_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        "sdk/python/pyproject.toml",
        "dynamic = [\"version\"]",
        "version = \"0.1.0\"",
    );
    fixture.refuses(&[
        "pyproject.toml: project.version is \"0.1.0\"; it must be dynamic",
        "project.dynamic must list \"version\"",
    ]);
}

#[test]
fn a_dashboard_on_another_version_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        "dashboard/package.json",
        "\"version\": \"0.1.0\"",
        "\"version\": \"0.1.9\"",
    );
    fixture.refuses(&["dashboard/package.json: version is \"0.1.9\""]);
}

#[test]
fn a_chart_app_version_that_disagrees_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        "charts/capsulet/Chart.yaml",
        "appVersion: \"0.1.0\"",
        "appVersion: \"0.0.9\"",
    );
    fixture.refuses(&["Chart.yaml: appVersion is \"0.0.9\""]);
}

#[test]
fn an_image_tag_that_disagrees_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        "charts/capsulet/values.yaml",
        "  tag: \"0.1.0\"",
        "  tag: \"0.0.9\"",
    );
    fixture.refuses(&["values.yaml: image.tag is \"0.0.9\""]);
}

#[test]
fn an_openapi_version_that_disagrees_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        "crates/api/openapi.json",
        "\"title\": \"Capsulet API\",\n    \"version\": \"0.1.0\"",
        "\"title\": \"Capsulet API\",\n    \"version\": \"0.0.9\"",
    );
    fixture.refuses(&["openapi.json: info.version is \"0.0.9\""]);
}

#[test]
fn an_image_under_a_doubled_repository_segment_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        ".github/workflows/supply-chain.yml",
        "images: ${{ matrix.image }}",
        "images: ghcr.io/${{ github.repository }}/capsulet-${{ matrix.component }}",
    );
    // Assembled here, so this file does not hold the reference it tests.
    let doubled = ["ghcr.io/mohripan", "capsulet", "capsulet-api:0.1.0"].join("/");
    fs::write(
        fixture.root().join("docs/pull.md"),
        format!("docker pull {doubled}\n"),
    )
    .expect("write");
    fixture.refuses(&[
        "supply-chain.yml",
        "ghcr.io/${{ is not an image release.toml names",
        &format!("docs/pull.md:1: {doubled} is not an image"),
    ]);
}

#[test]
fn a_component_image_shown_at_another_version_is_refused() {
    let fixture = Fixture::new();
    let stale = ["ghcr.io/mohripan", "capsulet-api:0.0.9"].join("/");
    fs::write(
        fixture.root().join("docs/pull.md"),
        format!("docker pull {stale}\n"),
    )
    .expect("write");
    fixture.refuses(&[&format!("docs/pull.md:1: {stale} is tagged 0.0.9")]);
}

#[test]
fn a_latest_or_unpinned_dependency_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        "charts/capsulet/values.yaml",
        "tag: \"4.53\"",
        "tag: latest",
    );
    fixture.edit(
        "compose.yaml",
        "postgres:16-alpine@sha256:",
        "postgres:16-alpine@sha256:0",
    );
    fixture.refuses(&[
        "values.yaml: seaweedfs.image chrislusf/seaweedfs is tagged latest",
        "compose.yaml: services.postgres.image",
    ]);
}

#[test]
fn a_second_dockerfile_is_refused() {
    let fixture = Fixture::new();
    fs::write(fixture.root().join("Dockerfile.rust"), "FROM scratch\n").expect("write");
    fixture.refuses(&["Dockerfile.rust: no component in release.toml builds from this Dockerfile"]);
}

#[test]
fn an_image_missing_its_release_labels_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        "dashboard/Dockerfile",
        "org.opencontainers.image.revision=\"${REVISION}\"",
        "org.opencontainers.image.revision=\"unknown\"",
    );
    fixture.refuses(&["dashboard/Dockerfile: does not set org.opencontainers.image.revision"]);
}

#[test]
fn a_link_to_another_repository_is_refused() {
    let fixture = Fixture::new();
    // Assembled here, so this file does not hold the link it tests.
    let other = ["github.com", "someone-else", "capsulet"].join("/");
    fixture.edit(
        "charts/capsulet/Chart.yaml",
        "home: https://github.com/mohripan/capsulet",
        &format!("home: https://{other}"),
    );
    fixture.refuses(&[
        &format!("Chart.yaml: home is \"https://{other}\""),
        &format!("links to {other}"),
    ]);
}

#[test]
fn a_dry_run_names_every_derived_file_and_writes_nothing() {
    let fixture = Fixture::new();
    let before: Vec<String> = FILES.iter().map(|file| fixture.read(file)).collect();
    let output = fixture.release(&["set", "0.1.1", "--dry-run"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let plan = stdout(&output);
    for file in [
        "release.toml",
        "Cargo.toml",
        "Cargo.lock",
        "sdk/python/src/capsulet/_version.py",
        "dashboard/package.json",
        "dashboard/package-lock.json",
        "charts/capsulet/Chart.yaml",
        "charts/capsulet/values.yaml",
        "crates/api/openapi.json",
    ] {
        assert!(
            plan.contains(&format!("would rewrite {file}\n")),
            "{file}:\n{plan}"
        );
    }
    let after: Vec<String> = FILES.iter().map(|file| fixture.read(file)).collect();
    assert_eq!(before, after, "a dry run wrote a file");
}

#[test]
fn writing_a_version_moves_every_manifest_and_only_its_version() {
    let fixture = Fixture::new();
    let output = fixture.release(&["set", "0.1.1", "--write"]);
    assert!(
        output.status.success(),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    let check = fixture.release(&["check"]);
    assert!(check.status.success(), "{}", stderr(&check));
    // Every changed line is a version, and nothing else moved.
    let diff = Command::new("git")
        .args(["diff", "--unified=0", "--no-color"])
        .current_dir(fixture.root())
        .output()
        .expect("git diff");
    let diff = String::from_utf8_lossy(&diff.stdout);
    let changed: Vec<&str> = diff
        .lines()
        .filter(|line| {
            (line.starts_with('+') || line.starts_with('-'))
                && !line.starts_with("+++")
                && !line.starts_with("---")
        })
        .collect();
    assert!(!changed.is_empty());
    for line in &changed {
        assert!(line.contains("0.1.1") || line.contains("0.1.0"), "{line}");
    }
    let lock = fixture.read("Cargo.lock");
    assert!(lock.contains("name = \"capsulet-api\"\nversion = \"0.1.1\""));
    // Deterministic: the same version again changes nothing.
    fixture.commit();
    let again = fixture.release(&["set", "0.1.1", "--write"]);
    assert!(again.status.success(), "{}", stderr(&again));
    assert!(
        stdout(&again).contains("already at 0.1.1"),
        "{}",
        stdout(&again)
    );
}

#[test]
fn writing_refuses_a_dirty_worktree_unless_the_maintainer_overrides_it() {
    let fixture = Fixture::new();
    fixture.edit("compose.yaml", "services:", "services: # an unrelated edit");
    let refused = fixture.release(&["set", "0.1.1", "--write"]);
    assert!(!refused.status.success());
    assert!(
        stderr(&refused).contains("uncommitted changes"),
        "{}",
        stderr(&refused)
    );
    assert!(fixture.read("release.toml").contains("version = \"0.1.0\""));
    let overridden = fixture.release(&["set", "0.1.1", "--write", "--allow-dirty"]);
    assert!(overridden.status.success(), "{}", stderr(&overridden));
    assert!(fixture.read("release.toml").contains("version = \"0.1.1\""));
}

#[test]
fn a_version_outside_the_api_compatibility_range_is_refused() {
    let fixture = Fixture::new();
    let output = fixture.release(&["set", "0.2.0", "--dry-run"]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("0.2.0 is outside compatibility.api"),
        "{}",
        stderr(&output)
    );
}
