//! `supply-chain check` on a copy of this repository's workflows, Dependabot
//! configuration and Dockerfiles. Each test breaks one rule and requires the
//! check to name it, so a rule that stops being enforced fails here.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const IMAGE_RELEASE: &str = ".github/workflows/image-release.yml";
const RELEASE: &str = ".github/workflows/release.yml";
const SUPPLY_CHAIN: &str = ".github/workflows/supply-chain.yml";
const CI: &str = ".github/workflows/ci.yml";

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn git(root: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        .args([
            "-c",
            "user.name=supply chain contract",
            "-c",
            "user.email=supply-chain@example.invalid",
            "-c",
            "core.autocrlf=false",
        ])
        .args(arguments)
        .current_dir(root)
        .status()
        .expect("git runs");
    assert!(status.success(), "git {arguments:?}");
}

struct Fixture {
    directory: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("temporary directory");
        let source = repository();
        let mut files = vec![
            ".github/dependabot.yml".to_string(),
            "crates/Dockerfile".to_string(),
            "dashboard/Dockerfile".to_string(),
        ];
        for entry in fs::read_dir(source.join(".github/workflows")).expect("workflows") {
            let name = entry
                .expect("workflow")
                .file_name()
                .to_string_lossy()
                .into_owned();
            files.push(format!(".github/workflows/{name}"));
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

    /// Replaces the first `from` in `file`, which must contain it.
    fn edit(&self, file: &str, from: &str, to: &str) {
        let path = self.root().join(file);
        let text = fs::read_to_string(&path).unwrap_or_else(|error| panic!("{file}: {error}"));
        assert!(text.contains(from), "{file} has no {from:?}");
        fs::write(path, text.replacen(from, to, 1)).expect("write");
    }

    fn check(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_capsulet-xtask"))
            .args(["supply-chain", "check", "--root"])
            .arg(self.root())
            .output()
            .expect("xtask runs")
    }

    /// The check must fail and say every `expected` text.
    fn refuses(&self, expected: &[&str]) {
        let output = self.check();
        let report = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "the check passed:\n{}",
            String::from_utf8_lossy(&output.stdout)
        );
        for text in expected {
            assert!(
                report.contains(text),
                "the check did not say {text:?}:\n{report}"
            );
        }
    }
}

#[test]
fn the_workflows_as_checked_in_hold_to_the_contract() {
    let fixture = Fixture::new();
    let output = fixture.check();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn an_action_pinned_to_a_tag_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        SUPPLY_CHAIN,
        "actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09 # v5.1.0",
        "actions/checkout@v5",
    );
    fixture.refuses(&["uses actions/checkout@v5, which is not pinned to a full commit SHA"]);
}

#[test]
fn a_pin_without_its_release_comment_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        RELEASE,
        "azure/setup-helm@1a275c3b69536ee54be43f2070a358922e12c8d4 # v4.3.1",
        "azure/setup-helm@1a275c3b69536ee54be43f2070a358922e12c8d4",
    );
    fixture.refuses(&["pins azure/setup-helm@1a275c3b69536ee54be43f2070a358922e12c8d4 without a `# vX.Y.Z` comment"]);
}

#[test]
fn an_image_rebuilt_after_it_was_scanned_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        IMAGE_RELEASE,
        "      - name: Upload the scan to code scanning",
        "      - name: Build it again to push\n        run: docker buildx build --push .\n\n      - name: Upload the scan to code scanning",
    );
    fixture.refuses(&["job publish builds an image 2 times"]);
}

#[test]
fn an_image_pushed_with_tags_before_it_is_verified_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        IMAGE_RELEASE,
        "          outputs: type=image,name=${{ inputs.image }},push-by-digest=true,name-canonical=true,push=true",
        "          tags: ${{ inputs.image }}:main\n          push: true",
    );
    fixture.refuses(&[
        "job publish pushes tags while it builds",
        "job publish does not push by digest",
    ]);
}

#[test]
fn a_scan_of_a_mutable_tag_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        IMAGE_RELEASE,
        "          image-ref: ${{ inputs.image }}@${{ steps.build.outputs.digest }}\n          trivy-config: trivy.yaml",
        "          image-ref: ${{ inputs.image }}:main\n          trivy-config: trivy.yaml",
    );
    fixture
        .refuses(&["reads \"${{ inputs.image }}:main\" rather than the digest the build pushed"]);
}

#[test]
fn signing_a_mutable_tag_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        IMAGE_RELEASE,
        "cosign sign --yes \"${{ inputs.image }}@${{ steps.build.outputs.digest }}\"",
        "cosign sign --yes \"${{ inputs.image }}:main\"",
    );
    fixture.refuses(&["Sign the digest signs a reference that is not a digest"]);
}

#[test]
fn a_publication_without_its_sbom_attestation_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        IMAGE_RELEASE,
        "        uses: actions/attest-sbom@c604332985a26aa8cf1bdc465b92731239ec6b9e # v4.1.0",
        "        uses: ./.github/workflows/image-release.yml",
    );
    fixture.refuses(&["job publish publishes an image without an SBOM attestation"]);
}

#[test]
fn a_publication_without_provenance_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        IMAGE_RELEASE,
        "          subject-digest: ${{ steps.build.outputs.digest }}\n          push-to-registry: true\n\n      - name: Attest the SBOM",
        "          subject-digest: ${{ steps.build.outputs.digest }}\n          push-to-registry: false\n\n      - name: Attest the SBOM",
    );
    fixture
        .refuses(&["publishes an image without a provenance attestation pushed to the registry"]);
}

#[test]
fn tagging_before_verification_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        IMAGE_RELEASE,
        "          cosign verify \"$REFERENCE\" \\",
        "          cosign verify-blob \"$REFERENCE\" \\",
    );
    fixture.refuses(&[
        "publishes an image without a verification of its signature, provenance and SBOM",
        "tags the image before it is verified",
    ]);
}

#[test]
fn a_manifest_written_before_verification_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        RELEASE,
        "            cosign verify \"$reference\" \\",
        "            cosign triangulate \"$reference\" \\",
    );
    fixture.refuses(&["writes the release manifest without verifying every image"]);
}

#[test]
fn a_privileged_pull_request_workflow_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        SUPPLY_CHAIN,
        "permissions:\n  contents: read\n\njobs:",
        "permissions:\n  contents: read\n  packages: write\n  id-token: write\n\njobs:",
    );
    fixture.refuses(&[
        "supply-chain.yml grants `packages: write` to pull request code",
        "supply-chain.yml grants `id-token: write` to pull request code",
    ]);
}

#[test]
fn a_pull_request_reaching_the_publishing_workflow_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        SUPPLY_CHAIN,
        "    uses: ./.github/workflows/image-scan.yml",
        "    uses: ./.github/workflows/image-release.yml",
    );
    fixture.refuses(&["image-release.yml, which .github/workflows/supply-chain.yml calls for pull requests, job publish grants `packages: write`"]);
}

#[test]
fn pull_request_target_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        SUPPLY_CHAIN,
        "  pull_request:\n",
        "  pull_request_target:\n",
    );
    fixture.refuses(&["runs on pull_request_target"]);
}

#[test]
fn a_scan_allowed_to_fail_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        IMAGE_RELEASE,
        "        id: gate\n",
        "        id: gate\n        continue-on-error: true\n",
    );
    fixture.refuses(&["may fail without failing the job (`continue-on-error`)"]);
}

#[test]
fn a_gate_turned_into_a_warning_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        CI,
        "        run: cargo run -p capsulet-xtask --locked -- verify --gate secrets",
        "        run: cargo run -p capsulet-xtask --locked -- verify --gate secrets || true",
    );
    fixture.refuses(&["turns a failed check into a pass"]);
}

#[test]
fn secret_scanning_of_only_the_latest_commit_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        CI,
        "          fetch-depth: 0\n",
        "          fetch-depth: 1\n",
    );
    fixture.refuses(&["no CI job runs the secrets gate over the whole Git history"]);
}

#[test]
fn ci_that_never_scans_the_dependency_images_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        CI,
        "        run: cargo run -p capsulet-xtask --locked -- verify --gate dependency-images",
        "        run: cargo run -p capsulet-xtask --locked -- release dependency-images",
    );
    fixture.refuses(&[
        "no CI job runs the dependency-images gate, so nothing scans the third-party images the \
         chart and Compose pull",
    ]);
}

#[test]
fn a_base_image_pinned_by_tag_alone_is_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        "dashboard/Dockerfile",
        "FROM node:20-alpine@sha256:fb4cd12c85ee03686f6af5362a0b0d56d50c58a04632e6c0fb8363f609372293 AS deps",
        "FROM node:20-alpine AS deps",
    );
    fixture.refuses(&["dashboard/Dockerfile:1: FROM node:20-alpine is not pinned by digest"]);
}

#[test]
fn pins_nothing_refreshes_are_refused() {
    let fixture = Fixture::new();
    fixture.edit(
        ".github/dependabot.yml",
        "  - package-ecosystem: github-actions",
        "  - package-ecosystem: gitsubmodule",
    );
    fixture.refuses(&["does not update the pinned GitHub Actions"]);
}
