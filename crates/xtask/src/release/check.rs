//! `release check`: every manifest that carries the release identity agrees
//! with `release.toml`.
//!
//! Each manifest is read with a real parser, independently of how `release
//! set` rewrites it, so a broken rewrite cannot vouch for itself.

use std::{
    collections::BTreeSet,
    fmt::Display,
    fs,
    path::{Component as PathComponent, Path},
};

use serde_json::Value as Json;
use serde_yaml::Value as Yaml;

use super::{RELEASE_FILE, Release, git};

pub(super) const CARGO_MANIFEST: &str = "Cargo.toml";
pub(super) const CARGO_LOCK: &str = "Cargo.lock";
pub(super) const PYPROJECT: &str = "sdk/python/pyproject.toml";
pub(super) const PYTHON_VERSION_FILE: &str = "sdk/python/src/capsulet/_version.py";
pub(super) const DASHBOARD_PACKAGE: &str = "dashboard/package.json";
pub(super) const DASHBOARD_LOCK: &str = "dashboard/package-lock.json";
pub(super) const CHART: &str = "charts/capsulet/Chart.yaml";
pub(super) const CHART_VALUES: &str = "charts/capsulet/values.yaml";
pub(super) const OPENAPI: &str = "crates/api/openapi.json";
pub(super) const COMPOSE: &str = "compose.yaml";

/// Third-party images the chart bundles, each of which must be pinned.
pub(super) const DEPENDENCY_IMAGES: &[&str] = &["postgresql", "seaweedfs", "tests"];

/// Compose services that run an image the chart bundles too, which must be the
/// same image by the same digest.
const SHARED_DEPENDENCIES: &[(&str, &str)] =
    &[("postgres", "postgresql"), ("seaweedfs", "seaweedfs")];

/// Tracked paths whose references are not held to the release: design history
/// and tool configuration, which record what was true when they were written.
pub(super) const HISTORICAL: &[&str] = &[
    "docs/superpowers/",
    "planning/",
    ".agents/",
    ".claude/",
    ".idea/",
];

pub(super) fn check(root: &Path, release: &Release) -> Vec<String> {
    let mut findings = Findings::default();
    cargo(root, release, &mut findings);
    python_sdk(root, release, &mut findings);
    dashboard(root, release, &mut findings);
    let values = chart(root, release, &mut findings);
    openapi(root, release, &mut findings);
    compose(root, release, values.as_ref(), &mut findings);
    let files = match tracked_files(root) {
        Ok(files) => files,
        Err(error) => {
            findings.push(".", error);
            Vec::new()
        }
    };
    dockerfiles(root, release, &files, &mut findings);
    references(root, release, &files, &mut findings);
    findings.0
}

#[derive(Default)]
struct Findings(Vec<String>);

impl Findings {
    fn push(&mut self, path: &str, message: impl Display) {
        let finding = format!("{path}: {message}");
        if !self.0.contains(&finding) {
            self.0.push(finding);
        }
    }

    fn expect(&mut self, path: &str, field: &str, actual: Option<&str>, expected: &str) {
        match actual {
            Some(actual) if actual == expected => {}
            Some(actual) => self.push(
                path,
                format!("{field} is {actual:?}, {RELEASE_FILE} says {expected:?}"),
            ),
            None => self.push(
                path,
                format!("{field} is missing; {RELEASE_FILE} says {expected:?}"),
            ),
        }
    }
}

fn read(root: &Path, path: &str, findings: &mut Findings) -> Option<String> {
    match fs::read_to_string(root.join(path)) {
        Ok(text) => Some(text),
        Err(error) => {
            findings.push(path, format!("cannot be read: {error}"));
            None
        }
    }
}

fn read_toml(root: &Path, path: &str, findings: &mut Findings) -> Option<toml::Table> {
    let text = read(root, path, findings)?;
    text.parse::<toml::Table>()
        .map_err(|error| findings.push(path, format!("is not valid TOML: {error}")))
        .ok()
}

fn read_json(root: &Path, path: &str, findings: &mut Findings) -> Option<Json> {
    let text = read(root, path, findings)?;
    serde_json::from_str(&text)
        .map_err(|error| findings.push(path, format!("is not valid JSON: {error}")))
        .ok()
}

pub(super) fn read_yaml(root: &Path, path: &str) -> Result<Yaml, String> {
    let text = fs::read_to_string(root.join(path))
        .map_err(|error| format!("{path}: cannot be read: {error}"))?;
    let mut value: Yaml = serde_yaml::from_str(&text)
        .map_err(|error| format!("{path}: is not valid YAML: {error}"))?;
    value
        .apply_merge()
        .map_err(|error| format!("{path}: {error}"))?;
    Ok(value)
}

/// A scalar as text: YAML reads `version: 1.0` as a number, which is still a
/// version that can disagree.
pub(super) fn yaml_text(value: &Yaml) -> Option<String> {
    match value {
        Yaml::String(text) => Some(text.clone()),
        Yaml::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn cargo(root: &Path, release: &Release, findings: &mut Findings) {
    let Some(manifest) = read_toml(root, CARGO_MANIFEST, findings) else {
        return;
    };
    let workspace = manifest.get("workspace");
    let package = workspace.and_then(|workspace| workspace.get("package"));
    let field = |key: &str| {
        package
            .and_then(|package| package.get(key))
            .and_then(toml::Value::as_str)
    };
    let product = &release.product;
    findings.expect(
        CARGO_MANIFEST,
        "workspace.package.version",
        field("version"),
        &product.version,
    );
    findings.expect(
        CARGO_MANIFEST,
        "workspace.package.repository",
        field("repository"),
        &product.source,
    );
    findings.expect(
        CARGO_MANIFEST,
        "workspace.package.license",
        field("license"),
        &product.license,
    );
    if package
        .and_then(|package| package.get("publish"))
        .and_then(toml::Value::as_bool)
        != Some(false)
    {
        findings.push(
            CARGO_MANIFEST,
            "workspace.package.publish must be false: no crate is published on its own",
        );
    }
    let names = members(root, workspace, findings);
    let Some(lock) = read_toml(root, CARGO_LOCK, findings) else {
        return;
    };
    for package in lock
        .get("package")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = package
            .get("name")
            .and_then(toml::Value::as_str)
            .unwrap_or_default();
        if names.contains(name) && package.get("source").is_none() {
            findings.expect(
                CARGO_LOCK,
                &format!("{name} version"),
                package.get("version").and_then(toml::Value::as_str),
                &product.version,
            );
        }
    }
}

/// Holds every workspace member to the workspace's package metadata, and
/// returns their package names.
fn members(
    root: &Path,
    workspace: Option<&toml::Value>,
    findings: &mut Findings,
) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let members = workspace
        .and_then(|workspace| workspace.get("members"))
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str);
    for member in members {
        let path = format!("{member}/Cargo.toml");
        let Some(manifest) = read_toml(root, &path, findings) else {
            continue;
        };
        let Some(package) = manifest.get("package") else {
            findings.push(&path, "has no [package]");
            continue;
        };
        if let Some(name) = package.get("name").and_then(toml::Value::as_str) {
            names.insert(name.to_string());
        }
        for key in ["version", "license", "repository", "publish"] {
            let inherited = package
                .get(key)
                .and_then(|value| value.get("workspace"))
                .and_then(toml::Value::as_bool)
                == Some(true);
            if !inherited {
                let declared = package
                    .get(key)
                    .map_or_else(|| "nothing".to_string(), ToString::to_string);
                findings.push(
                    &path,
                    format!(
                        "package.{key} must be inherited with `{key}.workspace = true`, not \
                         {declared}"
                    ),
                );
            }
        }
    }
    names
}

fn python_sdk(root: &Path, release: &Release, findings: &mut Findings) {
    if let Some(pyproject) = read_toml(root, PYPROJECT, findings) {
        let project = pyproject.get("project");
        if let Some(version) = project.and_then(|project| project.get("version")) {
            findings.push(
                PYPROJECT,
                format!(
                    "project.version is {version}; it must be dynamic, read from \
                     {PYTHON_VERSION_FILE}"
                ),
            );
        }
        let dynamic = project
            .and_then(|project| project.get("dynamic"))
            .and_then(toml::Value::as_array)
            .is_some_and(|dynamic| {
                dynamic
                    .iter()
                    .any(|value| value.as_str() == Some("version"))
            });
        if !dynamic {
            findings.push(PYPROJECT, "project.dynamic must list \"version\"");
        }
        let path = pyproject
            .get("tool")
            .and_then(|tool| tool.get("hatch"))
            .and_then(|hatch| hatch.get("version"))
            .and_then(|version| version.get("path"))
            .and_then(toml::Value::as_str);
        findings.expect(
            PYPROJECT,
            "tool.hatch.version.path",
            path,
            PYTHON_VERSION_FILE
                .strip_prefix("sdk/python/")
                .unwrap_or(PYTHON_VERSION_FILE),
        );
    }
    if let Some(text) = read(root, PYTHON_VERSION_FILE, findings) {
        findings.expect(
            PYTHON_VERSION_FILE,
            "__version__",
            python_string(&text, "__version__"),
            &release.product.version,
        );
        findings.expect(
            PYTHON_VERSION_FILE,
            "SUPPORTED_API_VERSIONS",
            python_string(&text, "SUPPORTED_API_VERSIONS"),
            &release.compatibility.api,
        );
    }
}

/// The value of a top-level `name = "value"` assignment.
pub(super) fn python_string<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        line.strip_prefix(name)?
            .trim_start()
            .strip_prefix('=')?
            .trim()
            .strip_prefix('"')?
            .strip_suffix('"')
    })
}

fn dashboard(root: &Path, release: &Release, findings: &mut Findings) {
    let version = &release.product.version;
    if let Some(package) = read_json(root, DASHBOARD_PACKAGE, findings) {
        findings.expect(
            DASHBOARD_PACKAGE,
            "version",
            package["version"].as_str(),
            version,
        );
    }
    if let Some(lock) = read_json(root, DASHBOARD_LOCK, findings) {
        findings.expect(DASHBOARD_LOCK, "version", lock["version"].as_str(), version);
        findings.expect(
            DASHBOARD_LOCK,
            "packages[\"\"].version",
            lock["packages"][""]["version"].as_str(),
            version,
        );
    }
}

/// Checks `Chart.yaml` and the image values, and returns the values for the
/// checks that compare against them.
fn chart(root: &Path, release: &Release, findings: &mut Findings) -> Option<Yaml> {
    let version = &release.product.version;
    match read_yaml(root, CHART) {
        Ok(chart) => {
            findings.expect(
                CHART,
                "version",
                yaml_text(&chart["version"]).as_deref(),
                version,
            );
            findings.expect(
                CHART,
                "appVersion",
                yaml_text(&chart["appVersion"]).as_deref(),
                version,
            );
            findings.expect(
                CHART,
                "home",
                chart["home"].as_str(),
                &release.product.source,
            );
            let lists_source = chart["sources"].as_sequence().is_some_and(|sources| {
                sources
                    .iter()
                    .any(|source| source.as_str() == Some(release.product.source.as_str()))
            });
            if !lists_source {
                findings.push(
                    CHART,
                    format!("sources does not list {}", release.product.source),
                );
            }
        }
        Err(error) => findings.0.push(error),
    }
    let values = match read_yaml(root, CHART_VALUES) {
        Ok(values) => values,
        Err(error) => {
            findings.0.push(error);
            return None;
        }
    };
    let image = &values["image"];
    findings.expect(
        CHART_VALUES,
        "image.registry",
        image["registry"].as_str(),
        &release.images.registry,
    );
    findings.expect(
        CHART_VALUES,
        "image.namespace",
        image["namespace"].as_str(),
        &release.images.namespace,
    );
    findings.expect(
        CHART_VALUES,
        "image.tag",
        yaml_text(&image["tag"]).as_deref(),
        version,
    );
    let mut keys = BTreeSet::new();
    for (key, component) in &release.images.components {
        let values_key = component.values_key(key);
        keys.insert(values_key);
        findings.expect(
            CHART_VALUES,
            &format!("{values_key}.image.name"),
            values[values_key]["image"]["name"].as_str(),
            &component.image,
        );
    }
    for (key, value) in values.as_mapping().into_iter().flatten() {
        let (Some(key), Some(name)) = (key.as_str(), value["image"]["name"].as_str()) else {
            continue;
        };
        if !keys.contains(key) {
            findings.push(
                CHART_VALUES,
                format!("{key}.image.name {name:?} is not a component in {RELEASE_FILE}"),
            );
        }
    }
    for dependency in DEPENDENCY_IMAGES {
        if let Err(problem) = dependency_reference(&values[*dependency]["image"]) {
            findings.push(CHART_VALUES, format!("{dependency}.image {problem}"));
        }
    }
    Some(values)
}

/// `repository:tag@digest` of a third-party image in the chart's values. It
/// must name a tag other than `latest` and be pinned by digest.
pub(super) fn dependency_reference(image: &Yaml) -> Result<String, String> {
    let repository = image["repository"]
        .as_str()
        .filter(|repository| !repository.is_empty())
        .ok_or("has no repository")?;
    let tag = yaml_text(&image["tag"])
        .filter(|tag| !tag.is_empty())
        .ok_or("has no tag")?;
    if tag == "latest" {
        return Err(format!("{repository} is tagged latest; pin a version"));
    }
    let digest = image["digest"].as_str().unwrap_or_default();
    if !is_digest(digest) {
        return Err(format!(
            "{repository}:{tag} has no sha256 digest; pin it by digest"
        ));
    }
    Ok(format!("{repository}:{tag}@{digest}"))
}

pub(super) fn is_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .chars()
                .all(|character| matches!(character, '0'..='9' | 'a'..='f'))
    })
}

/// Whether a full image reference names a tag other than `latest` and a digest.
fn pinned(image: &str) -> bool {
    let Some((reference, digest)) = image.split_once('@') else {
        return false;
    };
    is_digest(digest)
        && reference
            .rsplit_once(':')
            .is_some_and(|(_, tag)| !tag.is_empty() && tag != "latest" && !tag.contains('/'))
}

fn openapi(root: &Path, release: &Release, findings: &mut Findings) {
    if let Some(document) = read_json(root, OPENAPI, findings) {
        findings.expect(
            OPENAPI,
            "info.version",
            document["info"]["version"].as_str(),
            &release.product.version,
        );
    }
}

fn compose(root: &Path, release: &Release, values: Option<&Yaml>, findings: &mut Findings) {
    let compose = match read_yaml(root, COMPOSE) {
        Ok(compose) => compose,
        Err(error) => {
            findings.0.push(error);
            return;
        }
    };
    let Some(services) = compose["services"].as_mapping() else {
        findings.push(COMPOSE, "has no services");
        return;
    };
    let mut components_built = BTreeSet::new();
    for (name, service) in services {
        let Some(name) = name.as_str() else {
            continue;
        };
        if let Some(image) = service["image"].as_str()
            && !pinned(image)
        {
            findings.push(
                COMPOSE,
                format!(
                    "services.{name}.image {image:?} must name a tag other than latest and a \
                     sha256 digest"
                ),
            );
        }
        if service["build"].is_null() {
            continue;
        }
        let Some(component) = release.images.components.get(name) else {
            findings.push(
                COMPOSE,
                format!(
                    "services.{name} builds an image that is not a component in {RELEASE_FILE}"
                ),
            );
            continue;
        };
        components_built.insert(name.to_string());
        let build = &service["build"];
        let dockerfile = normalize(
            &Path::new(build["context"].as_str().unwrap_or("."))
                .join(build["dockerfile"].as_str().unwrap_or("Dockerfile")),
        );
        if dockerfile != component.dockerfile {
            findings.push(
                COMPOSE,
                format!(
                    "services.{name} builds from {dockerfile}, but {RELEASE_FILE} builds it from {}",
                    component.dockerfile
                ),
            );
        }
        if build["target"].as_str() != component.target.as_deref() {
            findings.push(
                COMPOSE,
                format!(
                    "services.{name}.build.target is {:?}, {RELEASE_FILE} says {:?}",
                    build["target"].as_str(),
                    component.target
                ),
            );
        }
        for argument in ["VERSION", "REVISION", "CREATED", "SOURCE"] {
            if build["args"][argument].is_null() {
                findings.push(
                    COMPOSE,
                    format!(
                        "services.{name}.build.args does not pass {argument}, so the image would \
                         carry no {argument} label"
                    ),
                );
            }
        }
    }
    for key in release.images.components.keys() {
        if !components_built.contains(key) {
            findings.push(COMPOSE, format!("no service builds the {key} component"));
        }
    }
    let Some(values) = values else {
        return;
    };
    for (service, dependency) in SHARED_DEPENDENCIES {
        if let Ok(expected) = dependency_reference(&values[*dependency]["image"]) {
            findings.expect(
                COMPOSE,
                &format!("services.{service}.image"),
                compose["services"][*service]["image"].as_str(),
                &expected,
            );
        }
    }
}

/// A relative path as `/`-separated text without `.` segments.
fn normalize(path: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    for part in path.components() {
        match part {
            PathComponent::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            PathComponent::ParentDir => {
                parts.pop();
            }
            _ => {}
        }
    }
    parts.join("/")
}

/// Every file Git tracks or would add, relative to `root` with `/` separators.
pub(super) fn tracked_files(root: &Path) -> Result<Vec<String>, String> {
    let listing = git(
        root,
        &["ls-files", "--cached", "--others", "--exclude-standard"],
    )?;
    Ok(listing
        .lines()
        .map(str::to_string)
        .filter(|file| root.join(file).is_file())
        .collect())
}

pub(super) fn historical(file: &str) -> bool {
    HISTORICAL.iter().any(|prefix| file.starts_with(prefix))
}

fn is_dockerfile(file: &str) -> bool {
    let name = file.rsplit('/').next().unwrap_or(file);
    name == "Dockerfile"
        || name.starts_with("Dockerfile.")
        || name.ends_with(".Dockerfile")
        || name.ends_with(".dockerfile")
}

fn dockerfiles(root: &Path, release: &Release, files: &[String], findings: &mut Findings) {
    let expected: BTreeSet<&str> = release
        .images
        .components
        .values()
        .map(|component| component.dockerfile.as_str())
        .collect();
    for file in files {
        if is_dockerfile(file) && !historical(file) && !expected.contains(file.as_str()) {
            findings.push(
                file,
                format!(
                    "no component in {RELEASE_FILE} builds from this Dockerfile; a second \
                     definition of an image drifts from the first, so remove it"
                ),
            );
        }
    }
    for (key, component) in &release.images.components {
        let path = &component.dockerfile;
        let Some(text) = read(root, path, findings) else {
            continue;
        };
        if let Some(target) = &component.target {
            let declares = text.lines().any(|line| {
                let words: Vec<&str> = line.split_whitespace().collect();
                words.len() >= 4
                    && words[0].eq_ignore_ascii_case("FROM")
                    && words[words.len() - 2].eq_ignore_ascii_case("AS")
                    && words[words.len() - 1] == target
            });
            if !declares {
                findings.push(
                    path,
                    format!("has no `{target}` stage for the {key} component"),
                );
            }
        }
        let title = format!("org.opencontainers.image.title=\"{}\"", component.image);
        if !text.contains(&title) {
            findings.push(path, format!("does not label the {key} image with {title}"));
        }
        for (label, argument) in [
            ("version", "VERSION"),
            ("revision", "REVISION"),
            ("created", "CREATED"),
            ("source", "SOURCE"),
        ] {
            let needle = format!("org.opencontainers.image.{label}=\"${{{argument}}}\"");
            if !text.contains(&needle) {
                findings.push(path, format!("does not set {needle}"));
            }
        }
        let licenses = format!(
            "org.opencontainers.image.licenses=\"{}\"",
            release.product.license
        );
        if !text.contains(&licenses) {
            findings.push(path, format!("does not set {licenses}"));
        }
    }
}

/// Every image under the release's registry, and every link to this product's
/// repository, in a tracked file must be the one `release.toml` names.
fn references(root: &Path, release: &Release, files: &[String], findings: &mut Findings) {
    let images: BTreeSet<String> = release
        .images
        .components
        .values()
        .map(|component| release.image(component))
        .collect();
    let registry = format!("{}/", release.images.registry);
    let namespace = format!("{registry}{}/", release.images.namespace);
    let source = release
        .product
        .source
        .trim_start_matches("https://")
        .to_string();
    let repository = source.rsplit('/').next().unwrap_or_default().to_string();
    for file in files {
        if historical(file) || file == RELEASE_FILE {
            continue;
        }
        let Ok(text) = fs::read_to_string(root.join(file)) else {
            continue;
        };
        for (index, line) in text.lines().enumerate() {
            let location = format!("{file}:{}", index + 1);
            // Under the release's namespace, everywhere. Anywhere in the registry
            // in a workflow, which is where `ghcr.io/${{ github.repository }}/...`
            // grows a duplicate repository segment; elsewhere, other images in
            // the registry are users' images, as tests and examples use them.
            let prefix = if file.starts_with(".github/") {
                &registry
            } else {
                &namespace
            };
            for reference in tokens(line, prefix) {
                let (path, tag) = split_reference(reference);
                if !images.contains(path) {
                    findings.push(
                        &location,
                        format!("{reference} is not an image {RELEASE_FILE} names"),
                    );
                } else if let Some(tag) = tag
                    && tag != release.product.version
                {
                    findings.push(
                        &location,
                        format!(
                            "{reference} is tagged {tag}, {RELEASE_FILE} says {}",
                            release.product.version
                        ),
                    );
                }
            }
            for reference in tokens(line, "github.com/") {
                let path = reference.strip_prefix("github.com/").unwrap_or(reference);
                let mut parts = path.splitn(3, '/');
                let owner = parts.next().unwrap_or_default();
                let name = parts.next().unwrap_or_default().trim_end_matches(".git");
                let link = format!("github.com/{owner}/{name}");
                if name == repository && link != source {
                    findings.push(
                        &location,
                        format!("links to {link}, but {RELEASE_FILE} says the source is {source}"),
                    );
                }
            }
        }
    }
}

/// Each run of reference characters that starts with `prefix`, prefix included.
fn tokens<'a>(line: &'a str, prefix: &str) -> Vec<&'a str> {
    let mut found = Vec::new();
    let mut rest = line;
    let mut offset = 0;
    while let Some(start) = rest.find(prefix) {
        let begin = offset + start;
        let after = &line[begin..];
        let end = after
            .find(|character: char| {
                character.is_whitespace() || "\"'`()[]<>,;|\\*".contains(character)
            })
            .unwrap_or(after.len());
        let token = after[..end].trim_end_matches(['.', ':']);
        if token.len() > prefix.len() {
            found.push(&line[begin..begin + token.len()]);
        }
        offset = begin + prefix.len();
        rest = &line[offset..];
    }
    found
}

/// `registry/namespace/name` and the tag, if any, of an image reference.
fn split_reference(reference: &str) -> (&str, Option<&str>) {
    let without_digest = reference.split('@').next().unwrap_or(reference);
    let last_slash = without_digest.rfind('/').unwrap_or(0);
    match without_digest[last_slash..].find(':') {
        Some(colon) => {
            let colon = last_slash + colon;
            (&without_digest[..colon], Some(&without_digest[colon + 1..]))
        }
        None => (without_digest, None),
    }
}

#[cfg(test)]
mod tests {
    use super::{pinned, split_reference, tokens};

    #[test]
    fn a_reference_ends_at_its_delimiter() {
        assert_eq!(
            tokens(
                "run `ghcr.io/acme/capsulet-api:0.1.0` and ghcr.io/acme/capsulet-worker.",
                "ghcr.io/"
            ),
            vec![
                "ghcr.io/acme/capsulet-api:0.1.0",
                "ghcr.io/acme/capsulet-worker"
            ]
        );
        assert_eq!(
            tokens("images: ghcr.io/${{ github.repository }}/x", "ghcr.io/"),
            vec!["ghcr.io/${{"]
        );
    }

    #[test]
    fn a_reference_splits_into_its_path_and_tag() {
        assert_eq!(
            split_reference("ghcr.io/acme/capsulet-api:0.1.0@sha256:00"),
            ("ghcr.io/acme/capsulet-api", Some("0.1.0"))
        );
        assert_eq!(
            split_reference("ghcr.io/acme/capsulet-api"),
            ("ghcr.io/acme/capsulet-api", None)
        );
    }

    #[test]
    fn only_a_versioned_digest_is_pinned() {
        let digest = format!("sha256:{}", "a".repeat(64));
        assert!(pinned(&format!("postgres:16-alpine@{digest}")));
        assert!(!pinned(&format!("postgres:latest@{digest}")));
        assert!(!pinned("postgres:16-alpine"));
        assert!(!pinned(&format!("registry:5000/postgres@{digest}")));
    }
}
