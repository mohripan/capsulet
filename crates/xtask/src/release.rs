//! The repository's release identity, declared once in `release.toml`.
//!
//! - `release check` holds every derived manifest to it: crate, Python SDK,
//!   dashboard, chart, `OpenAPI`, Compose, Dockerfile and the image and source
//!   references in tracked files.
//! - `release check-chart` renders the chart and proves every workload runs the
//!   image the release names.
//! - `release images` builds each component's image the way CI does and checks
//!   its labels. With `--plan`, it prints that build matrix instead.
//! - `release dependency-images` lists the third-party images the chart and
//!   Compose pull, by digest, which the `dependency-images` gate scans.
//! - `release set <version>` rewrites every derived manifest at once.

mod chart;
mod check;
mod dependencies;
mod images;
mod manifest;
mod set;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use chrono::{DateTime, Utc};
use semver::{Version, VersionReq};
use serde::Deserialize;

pub(crate) use dependencies::{dependency_images, repository};

pub(crate) const RELEASE_FILE: &str = "release.toml";

/// `release.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Release {
    pub(crate) product: Product,
    pub(crate) images: Images,
    pub(crate) chart: Chart,
    pub(crate) compatibility: Compatibility,
}

/// Where a version tag publishes the Helm chart.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Chart {
    pub(crate) repository: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Product {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) source: String,
    pub(crate) license: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Images {
    pub(crate) registry: String,
    pub(crate) namespace: String,
    pub(crate) components: BTreeMap<String, Component>,
}

/// How one component's image is named and built.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Component {
    pub(crate) image: String,
    pub(crate) dockerfile: String,
    #[serde(default)]
    pub(crate) target: Option<String>,
    #[serde(default = "default_context")]
    pub(crate) context: String,
    /// The chart's values key for the component, when it is not the name.
    #[serde(default)]
    pub(crate) values: Option<String>,
}

impl Component {
    /// Where the chart's values configure this component.
    pub(crate) fn values_key<'a>(&'a self, name: &'a str) -> &'a str {
        self.values.as_deref().unwrap_or(name)
    }
}

fn default_context() -> String {
    ".".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Compatibility {
    pub(crate) api: String,
}

impl Release {
    pub(crate) fn load(root: &Path) -> Result<Self, String> {
        let path = root.join(RELEASE_FILE);
        let text =
            fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        let release: Self =
            toml::from_str(&text).map_err(|error| format!("{RELEASE_FILE}: {error}"))?;
        release.validate()?;
        Ok(release)
    }

    fn validate(&self) -> Result<(), String> {
        let version = Version::parse(&self.product.version).map_err(|error| {
            format!(
                "{RELEASE_FILE}: product.version {:?} is not a semantic version: {error}",
                self.product.version
            )
        })?;
        let range = self.compatibility_range()?;
        if !range.matches(&version) {
            return Err(format!(
                "{RELEASE_FILE}: product.version {version} is outside compatibility.api \
                 {range}; widen the range deliberately before releasing a version outside it"
            ));
        }
        let source = self
            .product
            .source
            .strip_prefix("https://github.com/")
            .unwrap_or_default();
        if source.split('/').count() != 2 || source.split('/').any(str::is_empty) {
            return Err(format!(
                "{RELEASE_FILE}: product.source must be https://github.com/<owner>/<repository>, \
                 not {:?}",
                self.product.source
            ));
        }
        for (field, value) in [
            ("images.registry", &self.images.registry),
            ("images.namespace", &self.images.namespace),
        ] {
            if !value.split('/').all(is_image_segment) {
                return Err(format!(
                    "{RELEASE_FILE}: {field} {value:?} is not a lowercase image path"
                ));
            }
        }
        if !self.chart.repository.split('/').all(is_image_segment) {
            return Err(format!(
                "{RELEASE_FILE}: chart.repository {:?} is not a lowercase repository path",
                self.chart.repository
            ));
        }
        if self.images.components.is_empty() {
            return Err(format!("{RELEASE_FILE}: images.components is empty"));
        }
        for (key, component) in &self.images.components {
            // One segment: a name under the namespace, never a path of its own,
            // which is how `<namespace>/capsulet/capsulet-api` happens.
            if !is_image_segment(&component.image) {
                return Err(format!(
                    "{RELEASE_FILE}: images.components.{key}.image {:?} must be one lowercase \
                     name, with no `/`",
                    component.image
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn compatibility_range(&self) -> Result<VersionReq, String> {
        VersionReq::parse(&self.compatibility.api).map_err(|error| {
            format!(
                "{RELEASE_FILE}: compatibility.api {:?} is not a version range: {error}",
                self.compatibility.api
            )
        })
    }

    /// `<registry>/<namespace>/<image>` of a component, without a tag.
    pub(crate) fn image(&self, component: &Component) -> String {
        format!(
            "{}/{}/{}",
            self.images.registry, self.images.namespace, component.image
        )
    }

    /// `<registry>/<namespace>/<chart repository>`, the OCI repository a version
    /// tag pushes the chart to.
    pub(crate) fn chart_repository(&self) -> String {
        format!(
            "{}/{}/{}",
            self.images.registry, self.images.namespace, self.chart.repository
        )
    }

    /// The tagged reference a component's image is released under.
    pub(crate) fn tagged_image(&self, component: &Component) -> String {
        format!("{}:{}", self.image(component), self.product.version)
    }
}

fn is_image_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| matches!(character, 'a'..='z' | '0'..='9' | '.' | '_' | '-'))
        && value.starts_with(|character: char| character.is_ascii_alphanumeric())
        && value.ends_with(|character: char| character.is_ascii_alphanumeric())
}

pub(crate) use chart::{
    referenced_secrets as secrets_read, render as chart_documents, workload_findings,
    workload_images,
};

/// Builds every component's image as the `containers` gate does, tagged
/// `capsulet-verify/<image>:<version>-<pid>`, and checks each one's labels and
/// user. Every reference it built is in `built`, even when a later one fails,
/// so the caller can remove them.
pub(crate) fn build_verified(root: &Path, built: &mut Vec<String>) -> Result<(), String> {
    let release = Release::load(root)?;
    let identity = BuildIdentity::current(root)?;
    let entries = images::plan(&release, &identity);
    images::build_all(root, &release, &identity, &entries, built)
}

/// What an image built from this checkout is labelled with.
pub(crate) struct BuildIdentity {
    pub(crate) version: String,
    pub(crate) revision: String,
    pub(crate) created: String,
    pub(crate) source: String,
}

impl BuildIdentity {
    /// The release version and source, the commit checked out (suffixed with
    /// `-dirty` when the worktree has changes, because then no commit is what
    /// was built), and that commit's time, so a rebuild of a commit carries the
    /// same labels.
    pub(crate) fn current(root: &Path) -> Result<Self, String> {
        let release = Release::load(root)?;
        let revision = git(root, &["rev-parse", "HEAD"])?;
        let dirty = !git(root, &["status", "--porcelain"])?.is_empty();
        let seconds: i64 = git(root, &["log", "-1", "--format=%ct", "HEAD"])?
            .parse()
            .map_err(|error| format!("git log printed no commit time: {error}"))?;
        let created = DateTime::<Utc>::from_timestamp(seconds, 0)
            .ok_or_else(|| format!("commit time {seconds} is out of range"))?
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        Ok(Self {
            version: release.product.version,
            revision: if dirty {
                format!("{revision}-dirty")
            } else {
                revision
            },
            created,
            source: release.product.source,
        })
    }

    /// The build arguments every Dockerfile turns into its OCI labels.
    pub(crate) fn build_args(&self) -> [(&'static str, &str); 4] {
        [
            ("VERSION", &self.version),
            ("REVISION", &self.revision),
            ("CREATED", &self.created),
            ("SOURCE", &self.source),
        ]
    }
}

pub(crate) fn git(root: &Path, arguments: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not run git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

const USAGE: &str = "usage: capsulet-xtask release check | check-chart | images [--plan] | \
                     dependency-images | inspect <component> <image reference> | \
                     manifest --digests <directory> --chart <directory> --source-sbom <file> \
                     --output <file> | set <version> (--dry-run | --write) [--allow-dirty] \
                     [--root <directory>]";

pub(crate) fn run(arguments: &[String]) -> Result<(), String> {
    let (root, arguments) = split_root(arguments)?;
    let release = Release::load(&root)?;
    match arguments.first().map(String::as_str) {
        Some("check") if arguments.len() == 1 => {
            let findings = check::check(&root, &release);
            if findings.is_empty() {
                println!(
                    "[release] every manifest matches {RELEASE_FILE}: {} {}",
                    release.product.name, release.product.version
                );
                Ok(())
            } else {
                Err(report(&findings))
            }
        }
        Some("check-chart") if arguments.len() == 1 => {
            let findings = chart::check(&root, &release)?;
            if findings.is_empty() {
                println!(
                    "[release] every chart workload runs the image {RELEASE_FILE} names, by tag \
                     and by digest"
                );
                Ok(())
            } else {
                Err(report(&findings))
            }
        }
        Some("images") => match &arguments[1..] {
            [] => images::build_and_check(&root, &release),
            [flag] if flag == "--plan" => images::print_plan(&root, &release),
            _ => Err(USAGE.to_string()),
        },
        Some("dependency-images") if arguments.len() == 1 => dependencies::print(&root),
        Some("inspect") => match &arguments[1..] {
            [component, reference] => {
                images::inspect_reference(&root, &release, component, reference)
            }
            _ => Err(USAGE.to_string()),
        },
        Some("manifest") => manifest::write(&root, &release, &arguments[1..]),
        Some("set") => set::run(&root, &release, &arguments[1..]),
        _ => Err(USAGE.to_string()),
    }
}

/// Takes `--root <directory>` out of the arguments; the repository otherwise.
pub(crate) fn split_root(arguments: &[String]) -> Result<(PathBuf, Vec<String>), String> {
    let mut root = None;
    let mut rest = Vec::new();
    let mut iterator = arguments.iter();
    while let Some(argument) = iterator.next() {
        if argument == "--root" {
            let value = iterator
                .next()
                .ok_or_else(|| "--root needs a directory".to_string())?;
            root = Some(PathBuf::from(value));
        } else {
            rest.push(argument.clone());
        }
    }
    let root = root.unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."));
    Ok((root, rest))
}

fn report(findings: &[String]) -> String {
    let mut message = format!("{} disagreement(s) with {RELEASE_FILE}:\n", findings.len());
    for finding in findings {
        message.push_str("  - ");
        message.push_str(finding);
        message.push('\n');
    }
    message.push_str(
        "Run `cargo run -p capsulet-xtask -- release set <version> --dry-run` to see what a \
         version change rewrites; docs/contracts/release-process.md lists every derived file.",
    );
    message
}
